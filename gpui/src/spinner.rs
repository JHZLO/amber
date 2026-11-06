// 도는 고리 — 지금 앱 ui.tsx 의 Spinner(styles.css .spinner): 18px, 2px 테(border-strong), 위쪽 4분의 1 은
// 글자색, 0.7초에 한 바퀴. 움직임 줄이기면 멈춘 고리.

use std::f32::consts::{FRAC_PI_4, PI};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::*;

use crate::theme::Tokens;

const SIZE: f32 = 18.;
const WIDTH: f32 = 2.;
const TURN_MS: u128 = 700;

fn arc(center: Point<Pixels>, r: f32, from: f32, to: f32) -> Option<Path<Pixels>> {
    let at = |a: f32| point(center.x + px(r * a.cos()), center.y + px(r * a.sin()));
    let mut b = PathBuilder::stroke(px(WIDTH));
    b.move_to(at(from));
    // 반원보다 작게 나눠 그린다(한 번에 그리는 호는 180도 아래)
    let steps = ((to - from) / (PI / 2.)).ceil().max(1.) as usize;
    for i in 1..=steps {
        let a = from + (to - from) * i as f32 / steps as f32;
        b.arc_to(point(px(r), px(r)), px(0.), false, true, at(a));
    }
    b.build().ok()
}

pub fn spinner(k: &Tokens, still: bool) -> impl IntoElement {
    let (ring, ink) = (k.border_strong, k.text);
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let r = (SIZE - WIDTH) / 2.;
            let c = b.center();
            let turn = if still {
                0.
            } else {
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                (ms % TURN_MS) as f32 / TURN_MS as f32 * 2. * PI
            };
            if let Some(p) = arc(c, r, 0., 2. * PI) {
                window.paint_path(p, ring);
            }
            // 위쪽 4분의 1(-135° ~ -45°)
            let top = -PI / 2.;
            if let Some(p) = arc(c, r, top - FRAC_PI_4 + turn, top + FRAC_PI_4 + turn) {
                window.paint_path(p, ink);
            }
            if !still {
                window.request_animation_frame();
            }
        },
    )
    .size(px(SIZE))
    .flex_none()
}
