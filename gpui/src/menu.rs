// 드롭다운 판 — 지금 앱 components/ui/select.tsx 와 styles.css 의 .select-menu, .select-item, .select-check.
// 트리거 아래 6px 에 뜨는 유리 판이고 너비는 트리거보다 좁아지지 않는다. 항목은 모두 체크 칸(14px)을
// 들고 있어 고른 것만 오른쪽으로 밀리지 않는다. 바깥을 누르면 닫힌다(Radix Select 와 같다).

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

#[derive(Default)]
pub struct MenuItem {
    pub label: String,
    pub checked: bool,
    /// 둘째 줄(작은 흐린 글자) — 작업 폴더 고르기의 경로 등
    pub sub: Option<String>,
    /// 이 항목 위에 가는 선(.root-menu-divider)
    pub divider_before: bool,
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
        // 판 유리: 웹은 바탕 86% + 뒤 흐림(blur 20px)이라 뒤의 글이 안 읽힌다. GPUI 엔 뒤 흐림이 없어
        // 바탕을 96% 로 올려 같은 효과(뒤가 비치되 읽히지는 않게)를 낸다
        .bg(k.surface.opacity(0.96))
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
        .children(items.into_iter().enumerate().flat_map(|(i, it)| {
            let (hover_bg, text) = (fill(k.glass_fill_hover), k.text);
            let pick = on_pick.clone();
            let divider = it.divider_before.then(|| {
                div()
                    .h(px(1.))
                    .mx(px(6.))
                    .my(px(4.))
                    .bg(k.border)
                    .into_any_element()
            });
            let row = div()
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
                .child(match it.sub {
                    None => div().child(it.label),
                    Some(sub) => div()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .min_w_0()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(it.label))
                        .child(
                            div()
                                .max_w(px(280.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(k.text_3)
                                .child(sub),
                        ),
                })
                .on_click(move |_, window, cx| pick(i, window, cx))
                .into_any_element();
            divider.into_iter().chain(std::iter::once(row))
        }));
    deferred(
        anchored()
            .position(point(anchor.left(), anchor.bottom() + px(6.)))
            .snap_to_window_with_margin(px(10.))
            .child(panel),
    )
    .with_priority(1)
}
