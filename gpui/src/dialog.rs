// 삭제 확인 — 지금 앱 ui.tsx 의 ConfirmDelete(ConfirmShell). 지우는 동작은 예외 없이 한 번 더 묻는다(DESIGN §3).
// 무엇을 지우는지 이름을 굵게 말하고, 되돌릴 수 없다고 적는다. 바깥을 눌러도 닫히지 않는다(경고 창).

use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::{button, danger_button};
use crate::theme::Tokens;

/// 확인 창 틀(ConfirmShell) — 제목, 본문, 오른쪽 아래 버튼 줄. 바깥을 눌러도 닫히지 않는다(경고 창)
pub fn shell(
    k: &Tokens,
    max_w: f32,
    title: String,
    body: AnyElement,
    buttons: Vec<Stateful<Div>>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    shell_split(k, max_w, title, body, Vec::new(), buttons, on_close)
}

/// .modal.fixed-h — 단계가 바뀌어도 판 키가 그대로(min(620px, 86%)). 본문이 남은 높이를 채운다
pub fn shell_fixed(
    k: &Tokens,
    max_w: f32,
    title: String,
    body: AnyElement,
    left: Vec<Stateful<Div>>,
    buttons: Vec<Stateful<Div>>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    shell_with(
        k,
        max_w,
        Some(620.),
        false,
        title,
        body,
        left,
        buttons,
        on_close,
    )
}

/// .modal.settings — 넓게(980) 열고 키를 고정(min(700px, 88%)). 본문은 여백 없이 가로로(왼쪽 목록 | 판), 판이 제 안에서 구른다
pub fn shell_settings(
    k: &Tokens,
    title: String,
    body: AnyElement,
    buttons: Vec<Stateful<Div>>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    shell_with(
        k,
        980.,
        Some(700.),
        true,
        title,
        body,
        Vec::new(),
        buttons,
        on_close,
    )
}

/// 바닥 줄이 왼쪽 묶음 | 빈칸 | 오른쪽 묶음(.modal-foot 의 spacer). 본문은 넘치면 스크롤(.modal-body), 판은 창의 86% 까지
pub fn shell_split(
    k: &Tokens,
    max_w: f32,
    title: String,
    body: AnyElement,
    left: Vec<Stateful<Div>>,
    buttons: Vec<Stateful<Div>>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    shell_with(k, max_w, None, false, title, body, left, buttons, on_close)
}

#[allow(clippy::too_many_arguments)]
fn shell_with(
    k: &Tokens,
    max_w: f32,
    fixed_h: Option<f32>,
    bare: bool,
    title: String,
    body: AnyElement,
    left: Vec<Stateful<Div>>,
    buttons: Vec<Stateful<Div>>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    // 한 줄에 선 버튼은 키가 같다 — 줄이 이긴다(.modal-foot .btn: 32px, 옆 14px, 12.5px)
    let foot = |b: Stateful<Div>| b.h(px(32.)).px(px(14.)).text_size(px(12.5));
    let panel = linear_gradient(
        150.,
        linear_color_stop(k.glass_fill.0, 0.),
        linear_color_stop(k.glass_fill.1, 1.),
    );
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(rgba(0x0a0a0a66))
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.))
        .child(
            div()
                .relative()
                .w_full()
                .max_w(px(max_w))
                .when_some(fixed_h, |d, h| d.h(px(h)))
                .max_h(relative(if bare { 0.88 } else { 0.86 }))
                .flex()
                .flex_col()
                .rounded(px(12.))
                .border_1()
                .border_color(k.glass_edge)
                .bg(k.surface)
                .child(div().absolute().inset_0().rounded(px(12.)).bg(panel))
                .shadow(vec![
                    k.glass_shadow(false)[0].clone(),
                    BoxShadow {
                        color: k.seat.opacity(0.5),
                        offset: point(px(0.), px(10.)),
                        blur_radius: px(30.),
                        spread_radius: px(-8.),
                        inset: false,
                    },
                ])
                .child(
                    div()
                        .flex()
                        .items_center()
                        .px(px(20.))
                        .py(px(16.))
                        .border_b_1()
                        .border_color(k.border)
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(16.))
                                .font_weight(FontWeight::BOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .id("confirm-x")
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(k.text_mix(0.06)))
                                .child(icon("x", 16.).text_color(k.text_2))
                                .on_click(on_close),
                        ),
                )
                .child(if bare {
                    div()
                        .id("modal-body")
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .overflow_hidden()
                        .text_size(px(14.))
                        .line_height(relative(1.6))
                        .child(body)
                } else {
                    div()
                        .id("modal-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(20.))
                        .text_size(px(14.))
                        .line_height(relative(1.6))
                        .child(body)
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(20.))
                        .py(px(14.))
                        .border_t_1()
                        .border_color(k.border)
                        .children(left.into_iter().map(foot))
                        .child(div().flex_1())
                        .children(buttons.into_iter().map(foot)),
                ),
        )
}

/// body 는 `{name}` 자리를 가진 한 문장
pub fn confirm_delete(
    k: &Tokens,
    title: String,
    body: String,
    name: String,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    // 이름만 굵게 — 한 문장으로 흐르게 StyledText 의 강조 구간으로 그린다
    let (before, after) = body.split_once("{name}").unwrap_or((&body, ""));
    let sentence = format!("{before}{name}{after}");
    let bold = before.len()..before.len() + name.len();
    let on_cancel = std::rc::Rc::new(on_cancel);
    let close = on_cancel.clone();
    let panel = linear_gradient(
        150.,
        linear_color_stop(k.glass_fill.0, 0.),
        linear_color_stop(k.glass_fill.1, 1.),
    );
    div()
        .absolute()
        .inset_0()
        // 뒤의 화면으로 클릭과 hover 가 새지 않게 막는다
        .occlude()
        .bg(rgba(0x0a0a0a66))
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.))
        .child(
            div()
                .relative()
                .w_full()
                .max_w(px(440.))
                .flex()
                .flex_col()
                .rounded(px(12.))
                .border_1()
                .border_color(k.glass_edge)
                .bg(k.surface)
                .child(div().absolute().inset_0().rounded(px(12.)).bg(panel))
                .shadow(vec![
                    k.glass_shadow(false)[0].clone(),
                    BoxShadow {
                        color: k.seat.opacity(0.5),
                        offset: point(px(0.), px(10.)),
                        blur_radius: px(30.),
                        spread_radius: px(-8.),
                        inset: false,
                    },
                ])
                .child(
                    div()
                        .flex()
                        .items_center()
                        .px(px(20.))
                        .py(px(16.))
                        .border_b_1()
                        .border_color(k.border)
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(16.))
                                .font_weight(FontWeight::BOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .id("confirm-x")
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(k.text_mix(0.06)))
                                .child(icon("x", 16.).text_color(k.text_2))
                                .on_click(move |e, w, cx| close(e, w, cx)),
                        ),
                )
                .child(
                    div()
                        .p(px(20.))
                        .text_size(px(14.))
                        .line_height(relative(1.6))
                        .child(StyledText::new(sentence).with_highlights(vec![(
                            bold,
                            HighlightStyle {
                                font_weight: Some(FontWeight::BOLD),
                                ..Default::default()
                            },
                        )]))
                        .child(t("common.irreversible")),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .px(px(20.))
                        .py(px(14.))
                        .border_t_1()
                        .border_color(k.border)
                        .child(
                            button("confirm-cancel", k, false, false)
                                .child(t("common.cancel"))
                                .on_click(move |e, w, cx| on_cancel(e, w, cx)),
                        )
                        .child(
                            danger_button("confirm-delete", k)
                                .child(t("common.delete"))
                                .on_click(on_confirm),
                        ),
                ),
        )
}
