// 드롭다운 판 — 지금 앱 components/ui/select.tsx 와 styles.css 의 .select-menu, .select-item, .select-check.
// 트리거 아래 6px 에 뜨는 유리 판이고 너비는 트리거보다 좁아지지 않는다. 항목은 모두 체크 칸(14px)을
// 들고 있어 고른 것만 오른쪽으로 밀리지 않는다. 바깥을 누르면 닫힌다(Radix Select 와 같다).

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

pub struct MenuItem {
    pub label: String,
    pub checked: bool,
}

/// anchor = 트리거의 창 좌표. 트리거를 다시 누른 것은 바깥이 아니다(트리거가 스스로 닫는다)
pub fn select_menu(
    id: &'static str,
    anchor: Bounds<Pixels>,
    items: Vec<MenuItem>,
    k: &Tokens,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> Deferred {
    let on_pick = Rc::new(on_pick);
    let fill = |pair: (Hsla, Hsla)| {
        linear_gradient(
            150.,
            linear_color_stop(pair.0, 0.),
            linear_color_stop(pair.1, 1.),
        )
    };
    let mut shadow = k.glass_shadow(false)[..1].to_vec();
    shadow.extend(k.shadow());
    let panel = div()
        .id(id)
        .tag(id)
        .relative()
        .min_w(anchor.size.width)
        .p(px(4.))
        .flex()
        .flex_col()
        .rounded(px(6.))
        .border_1()
        .border_color(k.glass_edge)
        // 판 유리: 바탕을 86% 남기고 그 위에 면 유리
        .bg(k.surface.opacity(0.86))
        .shadow(shadow)
        .on_mouse_down_out(move |e: &MouseDownEvent, window, cx| {
            if !anchor.contains(&e.position) {
                on_close(window, cx);
            }
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(6.))
                .bg(fill(k.glass_fill)),
        )
        .children(items.into_iter().enumerate().map(|(i, it)| {
            let (hover_bg, text) = (fill(k.glass_fill_hover), k.text);
            let pick = on_pick.clone();
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .tag(format!("{id}-{i}"))
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(10.))
                .pr(px(30.))
                .py(px(7.))
                .rounded(px(6.))
                .text_size(px(13.))
                .whitespace_nowrap()
                .cursor_pointer()
                .font_weight(if it.checked {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(if it.checked { k.text } else { k.text_2 })
                .hover(move |s| s.bg(hover_bg).text_color(text))
                .child(div().w(px(14.)).flex_shrink_0().when(it.checked, |d| {
                    d.child(icon("check", 12.).text_color(k.text))
                }))
                .child(it.label)
                .on_click(move |_, window, cx| pick(i, window, cx))
        }));
    deferred(
        anchored()
            .position(point(anchor.left(), anchor.bottom() + px(6.)))
            .snap_to_window_with_margin(px(10.))
            .child(panel),
    )
    .with_priority(1)
}
