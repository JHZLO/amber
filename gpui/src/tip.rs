// 말풍선 — 지금 앱 ui.tsx 의 Tooltip(styles.css .tip): 떠 있는 유리, 11px 중간 굵기, 한 줄.
// GPUI 의 `.tooltip(tip("..."))` 로 건다(요소에 id 가 있어야 한다).

use gpui_kit::*;

use crate::theme::Tokens;

pub struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let k = Tokens::get(cx);
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(k.glass_edge)
            .relative()
            .bg(k.surface.opacity(0.96))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(6.))
                    .bg(crate::note_ai_view::glass(k.glass_fill)),
            )
            .shadow(k.glass_shadow(false))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(k.text)
            .whitespace_nowrap()
            .child(self.0.clone())
    }
}

pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}
