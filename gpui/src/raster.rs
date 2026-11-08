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
    pub full: Option<Arc<Image>>,
    full_scale: f32,
    /// 보이는 곳(선명하게): 구간, 배율, 그림
    pub sharp: Option<((f32, f32, f32, f32), f32, Arc<Image>)>,
    generation: u64,
    task: Option<Task<()>>,
    /// 트리가 바뀌었다 — 전체부터 다시
    pub stale: bool,
}

/// 굽는 데 필요한 것 — 화면 배율(그림 단위 → 논리 픽셀), 그림 왼쪽 위의 판 안 자리, 판 크기, 화면 배율, 전체를 굽는 배율
pub struct Geometry {
    pub size: (f32, f32),
    pub scale: f32,
    pub pan: (f32, f32),
    pub view: (f32, f32),
    pub dpr: f32,
    pub full_base: f32,
}

impl Raster {
    /// 새 그림 — 다 버린다(굽던 것도)
    pub fn reset(&mut self) {
        *self = Raster::default();
    }

    /// 판 위의 그림 층(전체, 그 위에 선명한 곳)
    pub fn layers(&self, size: (f32, f32), pan: (f32, f32), scale: f32) -> Vec<Img> {
        let layer = |i: Arc<Image>, x: f32, y: f32, w: f32, h: f32| {
            img(i).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
        };
        let mut out = Vec::new();
        if let Some(full) = &self.full {
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
    raster.generation += 1;
    let generation = raster.generation;
    let size = g.size;
    let full_scale = (g.full_base * dpr)
        .min(FULL_MAX_PX / size.0.max(size.1))
        .max(0.05);
    let region = visible_region(g.view, g.pan, g.scale, size);
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(if need_full { 0 } else { 120 }))
            .await;
        let ex = cx.background_executor().clone();
        let out = ex
            .spawn(async move {
                if need_full {
                    crate::figure::raster(&tree, (0., 0., size.0, size.1), full_scale)
                        .map(|i| (true, region, full_scale, i))
                } else {
                    crate::figure::raster(&tree, region, want).map(|i| (false, region, want, i))
                }
            })
            .await;
        let _ = this.update(cx, |s, cx| {
            let r = get(s);
            if r.generation != generation {
                return;
            }
            match out {
                Ok((true, _, scale, img)) => {
                    r.full = Some(img);
                    r.full_scale = scale;
                    // 낡은 선명 층이 새 그림을 덮지 않게
                    if r.stale {
                        r.stale = false;
                        r.sharp = None;
                    }
                }
                Ok((false, region, scale, img)) => r.sharp = Some((region, scale, img)),
                Err(e) => eprintln!("그림 굽기: {e}"),
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
