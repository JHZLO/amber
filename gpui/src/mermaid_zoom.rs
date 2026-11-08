// mermaid 그림 카드와 확대 뷰어 — 지금 앱 Mermaid.tsx(흰 카드, 손을 올리면 '확대' 배지, 누르면 확대, 깨지면 원문 코드)와
// MermaidZoom.tsx(가운데 큰 카드, 휠은 커서 자리를 붙잡고 1.1배씩, 끌어 이동, 맞춤 = 판의 90%, 20% ~ 120%).
// 120% 까지만 키우므로 열 때 그 배율로 한 번 구워 둔다 — 옮기고 키우는 동안 다시 굽지 않는다.
// 닫기는 X 와 Esc 뿐 — 확대한 그림을 끌다 판 밖에서 놓아도 닫히지 않게 바깥 누르기로는 닫지 않는다.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button};
use crate::figure::Figure;
use crate::theme::Tokens;

const MIN: f32 = 0.2;
const MAX: f32 = 1.2;
const STEP: f32 = 1.1;

/// 끌어 옮기기 — GPUI 의 끌기 값
#[derive(Clone)]
pub struct ZoomGrab;

pub struct ZoomDrag;

impl Render for ZoomDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct MmdZoom {
    img: Arc<RenderImage>,
    /// 그림의 자연 크기
    size: (f32, f32),
    pub scale: f32,
    /// 판 가운데에서 그림 가운데까지
    pub t: (f32, f32),
    fitted: bool,
    canvas: Rc<Cell<Bounds<Pixels>>>,
    press: Option<(Point<Pixels>, (f32, f32))>,
    grab: Option<(Point<Pixels>, (f32, f32))>,
}

impl MmdZoom {
    /// 맞춤 — 판의 90%, 가운데로
    pub fn fit(&mut self) {
        let b = self.canvas.get();
        let (cw, ch) = (f32::from(b.size.width), f32::from(b.size.height));
        if cw <= 0. || ch <= 0. {
            return;
        }
        self.t = (0., 0.);
        self.scale = ((cw / self.size.0).min(ch / self.size.1) * 0.9).clamp(MIN, MAX);
        self.fitted = true;
    }

    /// 단추 — 판 가운데를 붙잡고
    pub fn zoom_by(&mut self, k: f32) {
        let ns = (self.scale * k).clamp(MIN, MAX);
        let r = ns / self.scale;
        self.scale = ns;
        self.t = (self.t.0 * r, self.t.1 * r);
    }

    /// 휠 한 번 = 1.1배, 커서 자리를 붙잡고
    fn wheel(&mut self, e: &ScrollEvent) {
        let dy = match e.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y,
        };
        if dy == 0. {
            return;
        }
        let b = self.canvas.get();
        let mx = f32::from(e.position.x - b.origin.x) - f32::from(b.size.width) / 2.;
        let my = f32::from(e.position.y - b.origin.y) - f32::from(b.size.height) / 2.;
        let step = if dy > 0. { STEP } else { 1. / STEP };
        let ns = (self.scale * step).clamp(MIN, MAX);
        let k = ns / self.scale;
        self.t = (mx - (mx - self.t.0) * k, my - (my - self.t.1) * k);
        self.scale = ns;
    }

    pub fn pct(&self) -> u32 {
        (self.scale * 100.).round() as u32
    }
}

impl Spike {
    /// 그림 카드를 눌렀다 — 120% 에서도 선명하게 화면 배율까지 곱해 한 번 굽는다.
    /// 포커스는 뿌리로(지금 앱도 판으로 옮긴다) — 모달 입력칸이 쥐고 있으면 Esc 가 뷰어에 닿지 않는다
    pub(crate) fn open_mmd_zoom(&mut self, fig: &Figure, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
        let dpr = window.scale_factor().max(1.);
        let img = crate::figure::raster(&fig.tree, (0., 0., fig.width, fig.height), MAX * dpr)
            .unwrap_or_else(|_| fig.image.clone());
        self.mmd_zoom = Some(MmdZoom {
            img,
            size: (fig.width, fig.height),
            scale: 1.,
            t: (0., 0.),
            fitted: false,
            canvas: Rc::default(),
            press: None,
            grab: None,
        });
    }

    pub(crate) fn zoom_pan_to(&mut self, pos: Point<Pixels>) -> bool {
        let Some(z) = self.mmd_zoom.as_mut() else {
            return false;
        };
        let Some((start, t)) = z.grab else {
            return false;
        };
        z.t = (
            t.0 + f32::from(pos.x - start.x),
            t.1 + f32::from(pos.y - start.y),
        );
        true
    }

    pub(crate) fn zoom_drop(&mut self) {
        if let Some(z) = self.mmd_zoom.as_mut() {
            z.grab = None;
        }
    }

    /// 노트, 모달 안의 mermaid 그림(.mermaid-rendered) — 다크에서도 흰 카드. 누르면 확대 뷰어
    pub(crate) fn mermaid_card(
        &self,
        id: impl Into<ElementId>,
        fig: &Figure,
        max_w: f32,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id: ElementId = id.into();
        let group = SharedString::from(format!("mmd-card-{id}"));
        let w = fig.width.min(max_w);
        let fig2 = fig.clone();
        div()
            .id(id)
            .tag("mmd-card")
            .group(group.clone())
            .relative()
            .my(px(14.))
            .p(px(14.))
            .flex()
            .justify_center()
            .border_1()
            .border_color(k.border)
            .rounded(px(8.))
            .bg(rgb(0xffffff))
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(0xd4d4d8)).bg(rgb(0xfafafa)))
            .child(
                img(fig.image.clone())
                    .w(px(w))
                    .h(px(fig.height * w / fig.width)),
            )
            // 손을 올리면 오른쪽 위 '확대' — 밝은 카드 위라 테마와 무관하게 짙게
            .child(
                div()
                    .absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(4.))
                    .bg(rgb(0x18181b))
                    .text_color(rgb(0xffffff))
                    .text_size(px(11.))
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(0.92))
                    .child(icon("expand", 13.).text_color(rgb(0xffffff)))
                    .child(t("diagrams.mmd.expand")),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_mmd_zoom(&fig2, window, cx);
                cx.notify();
            }))
    }

    /// 그리지 못한 mermaid(.mermaid-fail) — 오류 한 줄과 원문을 코드 블록으로(내용은 보이게)
    pub(crate) fn mermaid_fail(
        &self,
        id: &str,
        msg: &str,
        src: &str,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let fence = "`".repeat(
            src.split('\n')
                .map(|l| l.trim_start().chars().take_while(|c| *c == '`').count())
                .max()
                .unwrap_or(0)
                .max(2)
                + 1,
        );
        div()
            .tag("mmd-fail")
            .my(px(14.))
            .child(
                div()
                    .mb(px(4.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(k.danger)
                    .child(t("diagrams.mmd.failHead")),
            )
            .when(!msg.is_empty(), |d| {
                d.child(
                    div()
                        .mb(px(6.))
                        .max_h(px(72.))
                        .overflow_hidden()
                        .font_family(crate::markdown::MONO)
                        .text_size(px(11.5))
                        .text_color(k.text_3)
                        .child(msg.to_string()),
                )
            })
            .child(crate::markdown::text(
                SharedString::from(format!("mmd-fail-{id}")),
                format!("{fence}\n{src}\n{fence}"),
                self.on_link(cx),
                k,
                None,
            ))
    }

    /// 확대 뷰어(.mmd-zoom-overlay) — 맨 위에 뜬다(모달 안 그림에서 열어도)
    pub(crate) fn mmd_zoom_view(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let z = self.mmd_zoom.as_mut()?;
        // 판을 잰 다음 그림에서 맞춘다
        if !z.fitted {
            z.fit();
        }
        let z = self.mmd_zoom.as_ref()?;
        let canvas_cell = z.canvas.clone();
        let b = z.canvas.get();
        let (cw, ch) = (f32::from(b.size.width), f32::from(b.size.height));
        let (w, h) = (z.size.0 * z.scale, z.size.1 * z.scale);
        let icon_btn = |id: &'static str, name: &str, size: f32| {
            button(id, k, false, false)
                .w(px(32.))
                .h(px(32.))
                .px_0()
                .justify_center()
                .child(icon(name, size).text_color(k.text_2))
        };
        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(k.border)
            .bg(k.surface)
            .child(
                div()
                    .tag("mmd-zoom-pct")
                    .min_w(px(44.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(k.text_2)
                    .child(format!("{}%", z.pct())),
            )
            .child(div().flex_1())
            .child(icon_btn("mmd-zoom-out", "minus", 16.).on_click(cx.listener(
                |this, _, _, cx| {
                    if let Some(z) = this.mmd_zoom.as_mut() {
                        z.zoom_by(1. / 1.2);
                    }
                    cx.notify();
                },
            )))
            .child(
                button("mmd-zoom-fit", k, false, false)
                    .child(t("diagrams.zoom.fit"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(z) = this.mmd_zoom.as_mut() {
                            z.fit();
                        }
                        cx.notify();
                    })),
            )
            .child(
                icon_btn("mmd-zoom-in", "plus", 16.).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(z) = this.mmd_zoom.as_mut() {
                        z.zoom_by(1.2);
                    }
                    cx.notify();
                })),
            )
            .child(
                icon_btn("mmd-zoom-close", "x", 17.).on_click(cx.listener(|this, _, _, cx| {
                    this.mmd_zoom = None;
                    cx.notify();
                })),
            );
        let stage = div()
            .id("mmd-zoom-canvas")
            .tag("mmd-zoom-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(0xffffff))
            .cursor(if z.grab.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .child(
                canvas(
                    move |bounds, window, _| {
                        if canvas_cell.get() != bounds {
                            canvas_cell.set(bounds);
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(z.fitted, |d| {
                d.child(
                    img(z.img.clone())
                        .absolute()
                        .left(px(cw / 2. + z.t.0 - w / 2.))
                        .top(px(ch / 2. + z.t.1 - h / 2.))
                        .w(px(w))
                        .h(px(h)),
                )
            })
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                if let Some(z) = this.mmd_zoom.as_mut() {
                    z.wheel(e);
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    if let Some(z) = this.mmd_zoom.as_mut() {
                        z.press = Some((e.position, z.t));
                    }
                }),
            )
            .on_drag(ZoomGrab, {
                let this = cx.entity().downgrade();
                move |_, _, _, cx: &mut App| {
                    let _ = this.update(cx, |s, _| {
                        if let Some(z) = s.mmd_zoom.as_mut() {
                            z.grab = z.press.take();
                        }
                    });
                    cx.new(|_| ZoomDrag)
                }
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(40.))
                .bg(rgba(0x18181b52))
                .child(
                    div()
                        .tag("mmd-zoom")
                        .w(px(1100.))
                        .max_w_full()
                        .h(px(760.))
                        .max_h_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .rounded(px(14.))
                        .border_1()
                        .border_color(k.border)
                        .bg(k.surface)
                        .shadow(k.shadow())
                        .child(toolbar)
                        .child(stage),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui_kit::{Bounds, RenderImage, ScrollDelta, ScrollEvent, point, px, size};

    use super::{MAX, MmdZoom};

    fn zoom(scale: f32) -> MmdZoom {
        MmdZoom {
            img: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])),
            size: (400., 200.),
            scale,
            t: (0., 0.),
            fitted: true,
            canvas: Rc::new(Cell::new(Bounds::new(
                point(px(100.), px(50.)),
                size(px(1000.), px(600.)),
            ))),
            press: None,
            grab: None,
        }
    }

    #[test]
    fn fit_is_ninety_percent_and_capped_at_120() {
        let mut z = zoom(1.);
        z.fit();
        assert_eq!(z.scale, MAX, "넓은 판에서도 120% 까지만");
        z.size = (4000., 1000.);
        z.fit();
        assert!((z.scale - 0.225).abs() < 1e-6, "1000/4000 * 0.9");
    }

    #[test]
    fn wheel_keeps_the_point_under_the_cursor() {
        let mut z = zoom(0.5);
        // 판 가운데에서 오른쪽 100, 아래 50
        let at = point(px(100. + 600.), px(50. + 350.));
        let before = ((100. - z.t.0) / z.scale, (50. - z.t.1) / z.scale);
        z.wheel(&ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(3.))),
            ..Default::default()
        });
        assert!((z.scale - 0.55).abs() < 1e-6);
        let after = ((100. - z.t.0) / z.scale, (50. - z.t.1) / z.scale);
        assert!((before.0 - after.0).abs() < 1e-4 && (before.1 - after.1).abs() < 1e-4);
        // 버튼은 가운데를 붙잡는다 — 옮긴 만큼 같은 비로
        z.t = (10., -20.);
        z.zoom_by(1. / 1.2);
        assert!((z.t.0 - 10. / 1.2).abs() < 1e-4 && (z.t.1 + 20. / 1.2).abs() < 1e-4);
    }
}
