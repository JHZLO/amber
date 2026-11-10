// 벡터 그림을 어느 배율에서도 선명하게 — 다이어그램 캔버스와 ERD 캔버스가 함께 쓴다. 그림 전체는 판에 맞춘 해상도로 한 번
// 굽고(옮기고 키우는 동안 바로 보인다), 확대가 멈추면 보이는 곳(+여백)만 그 배율로 다시 구워 위에 얹는다. 굽기는 뒤에서 돈다.
// 트리가 바뀌면(칠하기, 흐리기) stale 로 표시하고 새로 구울 때까지 옛 그림을 그대로 둔다.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::*;
use resvg::usvg;

use crate::Spike;

/// 전체 그림을 굽는 한 변의 상한(픽셀)
const FULL_MAX_PX: f32 = 4096.;
/// 선명하게 다시 구울 때 보이는 곳 둘레에 더 굽는 몫(판 한 변의 비율)
const MARGIN: f32 = 0.5;

/// 보이는 곳(그림 좌표)에 둘레를 더하고 그림 경계로 자른 구간 (x, y, w, h)
pub fn visible_region(
    view: (f32, f32),
    pan: (f32, f32),
    scale: f32,
    size: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (vx, vy) = (-pan.0 / scale, -pan.1 / scale);
    let (vw, vh) = (view.0 / scale, view.1 / scale);
    let x0 = (vx - vw * MARGIN).max(0.);
    let y0 = (vy - vh * MARGIN).max(0.);
    let x1 = (vx + vw * (1. + MARGIN)).min(size.0);
    let y1 = (vy + vh * (1. + MARGIN)).min(size.1);
    (x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
}

#[derive(Default)]
pub struct Raster {
    /// 전체(낮은 해상도)와 그 배율
    pub full: Option<Arc<RenderImage>>,
    full_scale: f32,
    /// 보이는 곳(선명하게): 구간, 배율, 그림
    pub sharp: Option<((f32, f32, f32, f32), f32, Arc<RenderImage>)>,
    generation: u64,
    task: Option<Task<()>>,
    /// 굽고 있는 것 — 같은 것을 또 청하면 그대로 둔다. 판은 그림마다 청하는데(유리가 도는 ERD 는 매 프레임),
    /// 그때마다 새로 시작하면 끝나기 전에 버려져 그림이 끝내 안 나온다
    pending: Option<Want>,
    /// 트리가 바뀌었다 — 전체부터 다시
    pub stale: bool,
    /// 밀려난 그림 — 그림판(atlas)에서 내려야 메모리가 돌아온다. 창을 고치는 도중이 아닌 굽기 끝에서 내린다
    retired: Vec<Arc<RenderImage>>,
}

/// 굽기 한 건 — 전체인가, 어느 트리의 어느 구간을 어떤 배율로
#[derive(Clone, Copy, PartialEq)]
struct Want {
    full: bool,
    tree: *const usvg::Tree,
    region: (f32, f32, f32, f32),
    scale: f32,
}

/// 굽는 데 필요한 것 — 화면 배율(그림 단위 → 논리 픽셀), 그림 왼쪽 위의 판 안 자리, 판 크기, 화면 배율, 전체를 굽는 배율
pub struct Geometry {
    pub size: (f32, f32),
    pub scale: f32,
    pub pan: (f32, f32),
    pub view: (f32, f32),
    pub dpr: f32,
    pub full_base: f32,
    /// 흰 판 위에 놓인다(figure::raster 의 paper)
    pub paper: bool,
}

impl Raster {
    /// 새 그림 — 다 버린다(굽던 것도)
    pub fn reset(&mut self) {
        let mut retired = std::mem::take(&mut self.retired);
        retired.extend(self.full.take());
        retired.extend(self.sharp.take().map(|s| s.2));
        *self = Raster {
            retired,
            ..Raster::default()
        };
    }

    /// 판 위의 그림 층(전체, 그 위에 선명한 곳). 선명한 층이 보이는 곳을 다 덮으면 그것만 — 투명한 그림(ERD)은
    /// 밑에 깔린 흐린 전체 그림의 글자가 비쳐 선명한 글자 둘레에 번진 테가 생긴다
    pub fn layers(
        &self,
        size: (f32, f32),
        pan: (f32, f32),
        scale: f32,
        view: (f32, f32),
    ) -> Vec<Img> {
        let layer = |i: Arc<RenderImage>, x: f32, y: f32, w: f32, h: f32| {
            img(i).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
        };
        let covered = self.sharp.as_ref().is_some_and(|((rx, ry, rw, rh), _, _)| {
            let (x0, y0) = ((-pan.0 / scale).max(0.), (-pan.1 / scale).max(0.));
            let x1 = ((view.0 - pan.0) / scale).min(size.0);
            let y1 = ((view.1 - pan.1) / scale).min(size.1);
            x0 >= rx - 0.5 && y0 >= ry - 0.5 && x1 <= rx + rw + 0.5 && y1 <= ry + rh + 0.5
        });
        let mut out = Vec::new();
        if let Some(full) = &self.full
            && !covered
        {
            out.push(layer(
                full.clone(),
                pan.0,
                pan.1,
                size.0 * scale,
                size.1 * scale,
            ));
        }
        if let Some(((rx, ry, rw, rh), _, sharp)) = &self.sharp {
            out.push(layer(
                sharp.clone(),
                pan.0 + rx * scale,
                pan.1 + ry * scale,
                rw * scale,
                rh * scale,
            ));
        }
        out
    }
}

/// 전체 그림이 없거나 낡았으면 굽고, 확대가 전체 해상도를 넘으면 보이는 곳을 선명하게 — 멈추고 120ms 뒤에.
/// 끝나면 get 으로 같은 Raster 를 다시 찾아 넣는다(세대가 바뀌었으면 버린다)
pub fn schedule(
    raster: &mut Raster,
    tree: Arc<usvg::Tree>,
    g: Geometry,
    get: fn(&mut Spike) -> &mut Raster,
    cx: &mut Context<Spike>,
) {
    let dpr = g.dpr.max(1.);
    let want = g.scale * dpr;
    let need_full = raster.full.is_none() || raster.stale;
    // 전체 해상도로 충분하면 선명 층은 필요 없다
    let need_sharp = !need_full
        && want > raster.full_scale * 1.05
        && match &raster.sharp {
            None => true,
            Some((region, scale, _)) => {
                let vis = visible_region(g.view, g.pan, g.scale, g.size);
                let inside = vis.0 >= region.0 - 0.5
                    && vis.1 >= region.1 - 0.5
                    && vis.0 + vis.2 <= region.0 + region.2 + 0.5
                    && vis.1 + vis.3 <= region.1 + region.3 + 0.5;
                // 보이는 곳이 구운 곳을 벗어났거나 배율이 크게 달라졌다
                (*scale - want).abs() / want > 0.2 || !inside
            }
        };
    if !need_full && !need_sharp {
        return;
    }
    let size = g.size;
    let full_scale = (g.full_base * dpr)
        .min(FULL_MAX_PX / size.0.max(size.1))
        .max(0.05);
    let region = visible_region(g.view, g.pan, g.scale, size);
    let paper = g.paper;
    let ask = Want {
        full: need_full,
        tree: Arc::as_ptr(&tree),
        region: if need_full {
            (0., 0., size.0, size.1)
        } else {
            region
        },
        scale: if need_full { full_scale } else { want },
    };
    if raster.pending == Some(ask) {
        return;
    }
    raster.pending = Some(ask);
    raster.generation += 1;
    let generation = raster.generation;
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(if need_full { 0 } else { 120 }))
            .await;
        let ex = cx.background_executor().clone();
        let out = ex
            .spawn(async move {
                if need_full {
                    crate::figure::raster(&tree, (0., 0., size.0, size.1), full_scale, paper)
                        .map(|i| (true, region, full_scale, i))
                } else {
                    crate::figure::raster(&tree, region, want, paper)
                        .map(|i| (false, region, want, i))
                }
            })
            .await;
        let _ = this.update(cx, |s, cx| {
            let r = get(s);
            if r.generation != generation {
                return;
            }
            r.pending = None;
            let mut gone = std::mem::take(&mut r.retired);
            match out {
                Ok((true, _, scale, img)) => {
                    gone.extend(r.full.replace(img));
                    r.full_scale = scale;
                    // 낡은 선명 층이 새 그림을 덮지 않게
                    if r.stale {
                        r.stale = false;
                        gone.extend(r.sharp.take().map(|s| s.2));
                    }
                }
                Ok((false, region, scale, img)) => {
                    gone.extend(r.sharp.replace((region, scale, img)).map(|s| s.2));
                }
                Err(e) => eprintln!("그림 굽기: {e}"),
            }
            for img in gone {
                cx.drop_image(img, None);
            }
            cx.notify();
        });
    });
    raster.task = Some(task);
}

#[cfg(test)]
mod tests {
    use super::visible_region;

    #[test]
    fn visible_region_is_clipped_to_the_figure() {
        // 100% 에서 다 보이면 그림 전체
        assert_eq!(
            visible_region((400., 400.), (0., 0.), 1., (400., 400.)),
            (0., 0., 400., 400.)
        );
        // 4배로 왼쪽 위만 보이면 그 둘레만
        let r = visible_region((400., 400.), (0., 0.), 4., (400., 400.));
        assert_eq!(r, (0., 0., 150., 150.));
    }
}
