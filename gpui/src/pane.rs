// 두 판 사이 폭 조절 — 지금 앱 lib/usePaneResize.ts 와 styles.css 의 .pane-resizer.
// 왼쪽 판 오른쪽 끝에 10px 손잡이(경계에 반씩 걸친다). 쉴 때는 틈이 곧 손잡이고(커서가 알린다),
// 끄는 동안만 1px 선이 보인다. 잡은 지점을 존중한다: 커서가 움직인 만큼만 폭이 바뀐다.
// 폭은 '희망 폭'이고 그릴 땐 창에 맞춰 줄인다 — 오른쪽 판은 늘 340px 를 갖는다.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::day_card::Tag;
use crate::theme::Tokens;

/// 오른쪽(상세) 판이 늘 확보하는 폭
pub const DETAIL_MIN: f32 = 340.;

/// 들고 있는 것 — 어느 판의 폭인가(설정 키)
#[derive(Clone, Copy)]
pub struct PaneGrab {
    pub key: &'static str,
    pub min: f32,
    pub max: f32,
}

pub struct PaneDrag {
    pub grab: PaneGrab,
    start_x: Pixels,
    start_w: f32,
    /// 두 판을 합친 폭 — 오른쪽 판 몫을 남긴다
    body_w: f32,
}

/// 그릴 폭 — 희망 폭을 [min, 몸통 - 오른쪽 몫] 에 맞춘다
pub fn effective(want: f32, min: f32, body_w: f32) -> f32 {
    want.min(body_w - DETAIL_MIN).max(min)
}

pub struct Nothing;

impl Render for Nothing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Spike {
    /// 왼쪽 판 안 오른쪽 끝에 얹는 손잡이. width = 지금 그린 폭, body_w = 두 판을 합친 폭
    pub(crate) fn pane_resizer(
        &self,
        grab: PaneGrab,
        width: f32,
        body_w: f32,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let dragging = self
            .pane_drag
            .as_ref()
            .is_some_and(|d| d.grab.key == grab.key);
        let this = cx.entity().downgrade();
        let id = SharedString::from(format!("resizer-{}", grab.key));
        div()
            .id(id.clone())
            .tag(id)
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-5.))
            .w(px(10.))
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .when(dragging, |d| {
                d.child(div().w(px(1.)).h_full().bg(k.border_strong))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| this.pane_press = Some(e.position.x)),
            )
            .on_drag(grab, move |g: &PaneGrab, _, _, cx: &mut App| {
                let g = *g;
                let _ = this.update(cx, |s, cx| {
                    if let Some(x) = s.pane_press.take() {
                        s.pane_drag = Some(PaneDrag {
                            grab: g,
                            start_x: x,
                            start_w: width,
                            body_w,
                        });
                    }
                    cx.notify();
                });
                cx.new(|_| Nothing)
            })
    }

    /// 끄는 중 — 폭 하나뿐이라 매번 고쳐 그린다
    pub(crate) fn pane_move(&mut self, x: Pixels) -> bool {
        let Some(d) = self.pane_drag.as_ref() else {
            return false;
        };
        let hi = d.grab.max.min(d.body_w - DETAIL_MIN);
        let w = (d.start_w + f32::from(x - d.start_x))
            .min(hi)
            .max(d.grab.min);
        let changed = (w - self.cal_w).abs() > 0.1;
        self.cal_w = w;
        changed
    }

    /// 놓았다 — 이 판의 폭을 기억한다(판마다 따로)
    pub(crate) fn pane_drop(&mut self) {
        if let Some(d) = self.pane_drag.take() {
            amber_gpui::prefs::set(&self.db, d.grab.key, &self.cal_w.round().to_string());
        }
    }
}
