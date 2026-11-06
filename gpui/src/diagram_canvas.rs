// 다이어그램 캔버스 — 지금 앱 DiagramCanvas.tsx(svg-pan-zoom). 휠 = 커서 자리를 붙잡고 확대/축소, 끌기 = 이동,
// 더블클릭 = 확대(Shift = 축소), 맞춤/100%/±, 전체 화면, 배치 엔진(ELK/Dagre). 그림은 다크에서도 흰 판이다.
// 배율 표시는 처음 맞춘 크기가 100% 다(svg-pan-zoom 의 getZoom 과 같은 기준).
//
// 벡터를 어느 배율에서도 선명하게: 그림 전체는 판에 맞춘 해상도로 한 번 굽고(옮기고 키우는 동안 바로 보인다),
// 확대가 멈추면 보이는 곳(+여백)만 그 배율로 다시 구워 위에 얹는다. 굽기는 뒤에서 돈다.
// 문법이 깨지면 마지막으로 그린 그림을 그대로 두고 아래에 오류 줄만 띄운다.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use resvg::usvg;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 20.;
/// svg-pan-zoom 의 zoomScaleSensitivity — 단추 한 번 1.4배, 더블클릭 1.8배
const SENS: f32 = 0.4;
/// 전체 그림을 굽는 한 변의 상한(픽셀)
const FULL_MAX_PX: f32 = 4096.;
/// 선명하게 다시 구울 때 보이는 곳 둘레에 더 굽는 몫(판 한 변의 비율)
const MARGIN: f32 = 0.5;

/// 판에 맞추는 배율과 가운데 자리 — svg-pan-zoom 의 fit(contain) + center
pub fn fit(view: (f32, f32), size: (f32, f32)) -> (f32, (f32, f32)) {
    let s = (view.0 / size.0).min(view.1 / size.1);
    let s = if s.is_finite() && s > 0. { s } else { 1. };
    (s, ((view.0 - size.0 * s) / 2., (view.1 - size.1 * s) / 2.))
}

/// 판 안의 점 p 를 붙잡고 배율을 old → new 로 바꿀 때의 새 자리
pub fn zoom_at(pan: (f32, f32), old: f32, new: f32, p: (f32, f32)) -> (f32, f32) {
    let r = new / old;
    (p.0 - (p.0 - pan.0) * r, p.1 - (p.1 - pan.1) * r)
}

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

/// 끌어 옮기기 — GPUI 의 끌기 값
#[derive(Clone)]
pub struct CanvasGrab;

pub struct CanvasDrag;

impl Render for CanvasDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct Canvas {
    /// 지금 그린(또는 그리려다 실패한) 원문
    pub chart: String,
    pub tree: Option<Arc<usvg::Tree>>,
    pub error: Option<String>,
    /// 100% 배율 — 그림을 받은 때 판에 맞춘 배율
    pub base: f32,
    /// base 에 대한 배율(1 = 100%)
    pub zoom: f32,
    /// 그림 왼쪽 위의 판 안 자리
    pub pan: Point<Pixels>,
    /// 판의 자리(창 좌표)와 화면 배율 — 그린 뒤 잰다
    pub viewport: Rc<Cell<Bounds<Pixels>>>,
    pub dpr: Rc<Cell<f32>>,
    /// 다음 그림에서 맞춘다(새 그림, 전체 화면 전환)
    pub fit_pending: bool,
    /// 맞춘 때의 판 크기 — 판이 바뀌면 다시 맞춘다
    fitted_view: (f32, f32),
    /// 전체(낮은 해상도)와 보이는 곳(선명하게: 구간, 배율)
    pub full: Option<Arc<Image>>,
    full_scale: f32,
    pub sharp: Option<((f32, f32, f32, f32), f32, Arc<Image>)>,
    raster_gen: u64,
    raster_task: Option<Task<()>>,
    /// 누른 자리와 그때의 그림 자리 — 끌기가 시작되면 grab 으로 옮긴다(누른 뒤 끌기 전에 다시 그려져도 남게)
    pub press: Option<(Point<Pixels>, Point<Pixels>)>,
    /// 끄는 중(창 밖에서 놓으면 render 가 지운다)
    pub grab: Option<(Point<Pixels>, Point<Pixels>)>,
    pub hover: bool,
    pub fullscreen: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            chart: String::new(),
            tree: None,
            error: None,
            base: 0.,
            zoom: 1.,
            pan: Point::default(),
            viewport: Rc::default(),
            dpr: Rc::new(Cell::new(1.)),
            fit_pending: false,
            fitted_view: (0., 0.),
            full: None,
            full_scale: 0.,
            sharp: None,
            raster_gen: 0,
            raster_task: None,
            press: None,
            grab: None,
            hover: false,
            fullscreen: false,
        }
    }
}

impl Canvas {
    fn size(&self) -> Option<(f32, f32)> {
        self.tree
            .as_ref()
            .map(|t| (t.size().width(), t.size().height()))
    }

    fn scale(&self) -> f32 {
        self.base * self.zoom
    }

    fn view(&self) -> (f32, f32) {
        let b = self.viewport.get();
        (f32::from(b.size.width), f32::from(b.size.height))
    }

    fn pan_f(&self) -> (f32, f32) {
        (f32::from(self.pan.x), f32::from(self.pan.y))
    }

    /// 그림을 바꾼다 — 같은 원문이면 아무것도 안 한다. 실패하면 마지막 그림을 두고 오류만
    pub fn set_chart(&mut self, chart: &str) {
        if self.chart == chart && (self.tree.is_some() || self.error.is_some()) {
            return;
        }
        self.chart = chart.to_string();
        match crate::figure::mermaid_tree(chart) {
            Ok(tree) => {
                self.tree = Some(Arc::new(tree));
                self.error = None;
                self.full = None;
                self.sharp = None;
                self.fit_pending = true;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// 배율을 판 안의 점 p 를 붙잡고 바꾼다(최소 0.1, 최대 20)
    pub fn zoom_by(&mut self, factor: f32, p: (f32, f32)) {
        if self.tree.is_none() || self.base <= 0. {
            return;
        }
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (x, y) = zoom_at(self.pan_f(), self.scale(), self.base * new, p);
        self.zoom = new;
        self.pan = point(px(x), px(y));
    }

    /// 판 가운데를 붙잡고
    pub fn zoom_center(&mut self, factor: f32) {
        let v = self.view();
        self.zoom_by(factor, (v.0 / 2., v.1 / 2.));
    }

    /// 맞춤 — 지금 판 크기로 다시 맞추고 그것을 100% 로
    pub fn fit_now(&mut self) {
        let (Some(size), v) = (self.size(), self.view()) else {
            return;
        };
        if v.0 <= 0. || v.1 <= 0. {
            self.fit_pending = true;
            return;
        }
        let (s, (x, y)) = fit(v, size);
        self.base = s;
        self.zoom = 1.;
        self.pan = point(px(x), px(y));
        self.fit_pending = false;
        self.fitted_view = v;
        // 전체 그림의 해상도가 맞춘 배율에 달려 있다
        self.full = None;
        self.sharp = None;
    }

    /// 100% — 처음 맞춘 배율로 되돌리고 가운데로
    pub fn reset_100(&mut self) {
        let (Some(size), v) = (self.size(), self.view()) else {
            return;
        };
        self.zoom = 1.;
        let s = self.scale();
        self.pan = point(px((v.0 - size.0 * s) / 2.), px((v.1 - size.1 * s) / 2.));
    }

    pub fn pct(&self) -> u32 {
        (self.zoom * 100.).round().max(1.) as u32
    }
}

impl Spike {
    /// render 에서 — 판을 잰 뒤 맞추고(판이 바뀌었으면 다시), 굽기를 건다
    pub(crate) fn prepare_diagram_canvas(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.diagrams.canvas;
        let v = c.view();
        if c.tree.is_some() && v.0 > 0. && (c.fit_pending || (c.fitted_view != v && c.zoom == 1.)) {
            c.fit_now();
        }
        self.schedule_raster(cx);
    }

    /// 전체 그림이 없으면 굽고, 확대가 전체 해상도를 넘으면 보이는 곳을 선명하게 — 120ms 멈춘 뒤에
    fn schedule_raster(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.diagrams.canvas;
        let (Some(tree), Some(size)) = (c.tree.clone(), c.size()) else {
            return;
        };
        if c.base <= 0. || c.fit_pending {
            return;
        }
        let dpr = c.dpr.get().max(1.);
        let want = c.scale() * dpr;
        let need_full = c.full.is_none();
        // 전체 해상도로 충분하면 선명 층은 필요 없다
        let need_sharp = !need_full
            && want > c.full_scale * 1.05
            && match &c.sharp {
                None => true,
                Some((region, scale, _)) => {
                    let vis = visible_region(c.view(), c.pan_f(), c.scale(), size);
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
        c.raster_gen += 1;
        let generation = c.raster_gen;
        let full_scale = (c.base * dpr)
            .min(FULL_MAX_PX / size.0.max(size.1))
            .max(0.05);
        let region = visible_region(c.view(), c.pan_f(), c.scale(), size);
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
                let c = &mut s.diagrams.canvas;
                if c.raster_gen != generation {
                    return;
                }
                match out {
                    Ok((true, _, scale, img)) => {
                        c.full = Some(img);
                        c.full_scale = scale;
                    }
                    Ok((false, region, scale, img)) => c.sharp = Some((region, scale, img)),
                    Err(e) => eprintln!("다이어그램 굽기: {e}"),
                }
                cx.notify();
            });
        });
        self.diagrams.canvas.raster_task = Some(task);
    }

    /// 휠 — 커서 자리를 붙잡고 확대/축소(svg-pan-zoom 처럼 1.4 의 거듭제곱)
    fn canvas_wheel(&mut self, e: &ScrollEvent) {
        let dy = match e.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * 30.,
        };
        let c = &mut self.diagrams.canvas;
        let o = c.viewport.get().origin;
        let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
        c.zoom_by((1. + SENS).powf(dy / 90.), p);
    }

    pub(crate) fn canvas_pan_to(&mut self, pos: Point<Pixels>) -> bool {
        let c = &mut self.diagrams.canvas;
        let Some((start, pan)) = c.grab else {
            return false;
        };
        c.pan = pan + (pos - start);
        true
    }

    pub(crate) fn canvas_drop(&mut self) {
        self.diagrams.canvas.grab = None;
    }

    /// 캔버스에 손이 올라가 있을 때의 단축키(+ - 1 0), Esc 는 전체 화면부터 닫는다. 받았으면 true
    pub(crate) fn canvas_key(&mut self, key: &str) -> bool {
        let c = &mut self.diagrams.canvas;
        if key == "escape" && c.fullscreen {
            c.fullscreen = false;
            c.fit_pending = true;
            return true;
        }
        if !c.hover || c.tree.is_none() {
            return false;
        }
        match key {
            "+" | "=" => c.zoom_center(1. + SENS),
            "-" => c.zoom_center(1. / (1. + SENS)),
            "1" => c.reset_100(),
            "0" => c.fit_now(),
            _ => return false,
        }
        true
    }

    /// 캔버스(.dgm-canvas-wrap) — 그림 층 둘, 오른쪽 위 화면 조작, 왼쪽 아래 배치 엔진, 아래 오류 줄
    pub(crate) fn diagram_canvas(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let c = &self.diagrams.canvas;
        let (viewport, dpr) = (c.viewport.clone(), c.dpr.clone());
        let scale = c.scale();
        let layer = |img_: Arc<Image>, x: f32, y: f32, w: f32, h: f32| {
            img(img_)
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
        };
        let mut stage = div()
            .id("dgm-canvas")
            .tag("dgm-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .cursor(if c.grab.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .child(
                canvas(
                    move |b, window, _| {
                        let changed = viewport.get() != b;
                        viewport.set(b);
                        dpr.set(window.scale_factor());
                        // 처음 잰 판이면 다음 그림에서 맞춘다
                        if changed {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        if let (Some(tree), false) = (&c.tree, c.fit_pending) {
            let (w, h) = (tree.size().width(), tree.size().height());
            let (px0, py0) = (f32::from(c.pan.x), f32::from(c.pan.y));
            if let Some(full) = &c.full {
                stage = stage.child(layer(full.clone(), px0, py0, w * scale, h * scale));
            }
            if let Some(((rx, ry, rw, rh), _, sharp)) = &c.sharp {
                stage = stage.child(layer(
                    sharp.clone(),
                    px0 + rx * scale,
                    py0 + ry * scale,
                    rw * scale,
                    rh * scale,
                ));
            }
        } else if c.error.is_none() {
            stage = stage.child(
                div()
                    .p(px(30.))
                    .flex()
                    .justify_center()
                    .text_size(px(13.))
                    .text_color(k.text_3)
                    .child(t("diagrams.canvas.rendering")),
            );
        }
        let stage = stage
            .on_hover(cx.listener(|this, h: &bool, _, _| this.diagrams.canvas.hover = *h))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                this.canvas_wheel(e);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    let c = &mut this.diagrams.canvas;
                    c.press = Some((e.position, c.pan));
                }),
            )
            .on_drag(CanvasGrab, {
                let this = cx.entity().downgrade();
                move |_, _, _, cx: &mut App| {
                    let _ = this.update(cx, |s, _| {
                        let c = &mut s.diagrams.canvas;
                        c.grab = c.press.take();
                    });
                    cx.new(|_| CanvasDrag)
                }
            })
            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                // 더블클릭 = 커서 자리에서 확대(Shift = 축소)
                if e.click_count() == 2 {
                    let c = &mut this.diagrams.canvas;
                    let o = c.viewport.get().origin;
                    let p = e.position();
                    let f = 1. + SENS * 2.;
                    let f = if e.modifiers().shift { 1. / f } else { f };
                    c.zoom_by(f, (f32::from(p.x - o.x), f32::from(p.y - o.y)));
                    cx.notify();
                }
            }));

        // 흰 판 위라 테마 토큰이 아니라 밝은 색을 고정으로 쓴다(.dgm-float)
        let ink = rgb(0x52525b);
        let float_btn = |id: &'static str, active: bool| {
            div()
                .id(id)
                .tag(id)
                .min_w(px(28.))
                .h(px(28.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                // 고른 탭은 채움(§3) — hover 는 한 요소에 한 번만 걸 수 있다
                .when(active, |d| {
                    d.bg(rgb(0x18181b))
                        .text_color(rgb(0xffffff))
                        .hover(|s| s.bg(rgb(0x333333)))
                })
                .when(!active, |d| {
                    d.text_color(ink)
                        .hover(|s| s.bg(rgba(0x18181b0f)).text_color(rgb(0x18181b)))
                })
        };
        let float = || {
            div()
                .absolute()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(9.))
                .border_1()
                .border_color(rgba(0x18181b1a))
                .bg(rgba(0xffffffdb))
                .shadow(vec![BoxShadow {
                    color: rgba(0x18181b1a).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                // 판 위 단추를 누른 것은 끌기가 아니다
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let full_icon = if c.fullscreen { "x" } else { "expand" };
        let tools = float()
            .top(px(10.))
            .right(px(10.))
            .child(
                float_btn("dgm-fit", false)
                    .child(icon("fit-screen", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.fit_now();
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-zoom-out", false)
                    .child(icon("zoom-out", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.zoom_center(1. / (1. + SENS));
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-pct", false)
                    .min_w(px(46.))
                    .child(format!("{}%", c.pct()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.reset_100();
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-zoom-in", false)
                    .child(icon("zoom-in", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.zoom_center(1. + SENS);
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-fullscreen", false)
                    .child(icon(full_icon, 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let c = &mut this.diagrams.canvas;
                        c.fullscreen = !c.fullscreen;
                        c.fit_pending = true;
                        cx.notify();
                    })),
            );
        // 배치 엔진 — 고른 쪽만 채움. 앱 전체 설정이라 노트 안 그림도 이 엔진으로 그린다
        let cur = amber_gpui::mermaid::layout();
        let layouts = float()
            .bottom(px(10.))
            .left(px(10.))
            .children(["elk", "dagre"].map(|l| {
                let on = cur == l;
                float_btn(
                    if l == "elk" {
                        "dgm-layout-elk"
                    } else {
                        "dgm-layout-dagre"
                    },
                    on,
                )
                .px(px(10.))
                .whitespace_nowrap()
                .child(t(&format!("diagrams.layout.{l}")))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_diagram_layout(l);
                    cx.notify();
                }))
            }));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(0xffffff))
            .child(stage)
            .child(tools)
            .child(layouts)
            .when_some(c.error.clone(), |d, e| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(12.))
                        .py(px(8.))
                        .border_t_1()
                        .border_color(k.danger)
                        .bg(k.danger_soft)
                        .child(
                            div()
                                .id("dgm-error")
                                .tag("dgm-error")
                                .flex_1()
                                .min_w_0()
                                .max_h(px(76.))
                                .overflow_y_scroll()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(11.5))
                                .text_color(k.danger)
                                .child(e.clone()),
                        )
                        .child(
                            crate::day_card::button("dgm-error-copy", k, false, false)
                                .child(t("diagrams.copy"))
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(e.clone()));
                                })),
                        ),
                )
            })
    }

    /// 배치 엔진을 바꾼다 — 설정에 남기고 지금 그림을 다시 그린다(lib/diagramLayout.ts)
    pub(crate) fn set_diagram_layout(&mut self, name: &str) {
        amber_gpui::mermaid::set_layout(name);
        amber_gpui::prefs::set(&self.db, "amber.diagram.layout", name);
        let c = &mut self.diagrams.canvas;
        let chart = std::mem::take(&mut c.chart);
        c.tree = None;
        c.error = None;
        c.set_chart(&chart);
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, visible_region, zoom_at};

    #[test]
    fn fit_contains_and_centers() {
        let (s, (x, y)) = fit((800., 400.), (400., 400.));
        assert_eq!(s, 1.);
        assert_eq!((x, y), (200., 0.));
        let (s, _) = fit((200., 200.), (400., 100.));
        assert_eq!(s, 0.5);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor() {
        let (pan, old, new, p) = ((10., 20.), 1., 2., (110., 120.));
        let n = zoom_at(pan, old, new, p);
        // 커서 밑 그림 좌표가 그대로
        let before = ((p.0 - pan.0) / old, (p.1 - pan.1) / old);
        let after = ((p.0 - n.0) / new, (p.1 - n.1) / new);
        assert_eq!(before, after);
    }

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
