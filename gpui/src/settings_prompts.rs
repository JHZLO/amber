// 설정 › 저장 프롬프트 — 지금 앱 SettingsModal.tsx 의 프롬프트 칸. 노트 AI 작성 모달이 칩으로 낸다.
// 목록(이름만 한 줄, 손을 올리면 지우기), 새로/편집은 모달 전체를 쓰는 집중 편집기(이름 + 지시문, 목록 없이),
// 지우기는 한 번 묻는다. 저장은 settings 의 note_ai_prompts(JSON 배열) — 바꾸는 즉시.

use amber_gpui::i18n::t;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::note_ai_view::{Prompt, field_label, hint, load_prompts};
use crate::settings_view::set_section;
use crate::theme::Tokens;
use crate::tip::tip;

const KEY: &str = "note_ai_prompts";

pub struct PromptEdit {
    pub id: String,
    pub is_new: bool,
    pub name: Entity<InputState>,
    pub text: Entity<TextareaState>,
}

/// JS 문자열의 앞 n 글자(UTF-16)
fn head(s: &str, n: usize) -> String {
    let u: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16_lossy(&u[..u.len().min(n)])
}

/// 목록과 확인 창에 보일 이름 — 이름이 비면 지시문 앞 24자
pub fn prompt_name(p: &Prompt) -> String {
    let l = amber_gpui::vault::js_trim(&p.label);
    if l.is_empty() {
        head(&p.text, 24)
    } else {
        l.to_string()
    }
}

pub fn save_prompts(db: &amber_gpui::db::Db, list: &[Prompt]) {
    let arr: Vec<serde_json::Value> = list
        .iter()
        .map(|p| serde_json::json!({ "id": p.id, "label": p.label, "text": p.text }))
        .collect();
    amber_gpui::prefs::set(db, KEY, &serde_json::Value::from(arr).to_string());
}

impl Spike {
    pub(crate) fn prompt_start(
        &mut self,
        p: Option<Prompt>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, is_new, label, text) = match p {
            Some(p) => (p.id, false, p.label, p.text),
            None => (
                amber_gpui::comments::new_comment_id(),
                true,
                String::new(),
                String::new(),
            ),
        };
        let name = cx.new(|cx| {
            let mut s =
                InputState::new(window, cx).placeholder(t("settings.prompt.namePlaceholder"));
            s.set_value(label, window, cx);
            s
        });
        let body = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(10, 10)
                .placeholder(t("settings.prompt.textPlaceholder"));
            s.set_value(text, window, cx);
            s
        });
        cx.observe(&body, |_, _, cx| cx.notify()).detach();
        name.update(cx, |s, cx| s.focus(window, cx));
        self.settings.prompt_edit = Some(PromptEdit {
            id,
            is_new,
            name,
            text: body,
        });
        cx.notify();
    }

    /// 편집기를 닫는다(한 단계 뒤로) — 사라진 입력칸이 포커스를 쥐지 않게 뿌리로
    pub(crate) fn prompt_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.prompt_edit = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn prompt_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.settings.prompt_edit.as_ref() else {
            return;
        };
        let text = amber_gpui::vault::js_trim(&e.text.read(cx).value()).to_string();
        if text.is_empty() {
            return;
        }
        let name = amber_gpui::vault::js_trim(&e.name.read(cx).value()).to_string();
        let label = if name.is_empty() {
            head(&text, 24)
        } else {
            name
        };
        let updated = Prompt {
            id: e.id.clone(),
            label,
            text,
        };
        let mut list = load_prompts(&self.db);
        if e.is_new {
            list.push(updated);
        } else {
            for p in list.iter_mut() {
                if p.id == updated.id {
                    *p = updated.clone();
                }
            }
        }
        save_prompts(&self.db, &list);
        self.prompt_cancel(window, cx);
    }

    pub(crate) fn prompts_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let list = load_prompts(&self.db);
        let desc = format!(
            "{}{}{}",
            t("settings.prompts.desc.pre"),
            t("settings.prompts.desc.bold"),
            t("settings.prompts.desc.post")
        );
        let action = button("set-prompt-new", k, false, false)
            .child(icon("plus", 13.).text_color(k.text_2))
            .child(t("settings.prompt.new"))
            .on_click(cx.listener(|this, _, window, cx| this.prompt_start(None, window, cx)));
        let sec = set_section(
            k,
            t("settings.prompts.title"),
            Some(desc),
            Some(action.into_any_element()),
        )
        .tag("set-prompts");
        if list.is_empty() {
            return sec.child(
                div()
                    .p(px(16.))
                    .border_1()
                    .border_dashed()
                    .border_color(k.border_strong)
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .text_color(k.text_3)
                    .flex()
                    .justify_center()
                    .child(t("settings.prompts.empty")),
            );
        }
        let n = list.len();
        sec.child(
            div()
                .flex()
                .flex_col()
                .border_1()
                .border_color(k.border)
                .rounded(px(8.))
                .overflow_hidden()
                .children(list.into_iter().enumerate().map(|(i, p)| {
                    let group = SharedString::from(format!("prompt-row-{i}"));
                    let name = prompt_name(&p);
                    let (edit, del) = (p.clone(), (p.id.clone(), name.clone()));
                    let danger = k.danger;
                    let soft = k.danger_soft;
                    div()
                        .id(group.clone())
                        .group(group.clone())
                        .flex()
                        .items_center()
                        .pr(px(6.))
                        .when(i + 1 < n, |d| d.border_b_1().border_color(k.border))
                        .hover(|s| s.bg(k.surface_2))
                        .child(
                            div()
                                .id(SharedString::from(format!("set-prompt-{i}")))
                                .tag(format!("set-prompt-{i}"))
                                .flex_1()
                                .min_w_0()
                                .py(px(10.))
                                .px(px(12.))
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .cursor_pointer()
                                .tooltip(tip(p.text.clone()))
                                .child(name)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.prompt_start(Some(edit.clone()), window, cx)
                                })),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("set-prompt-del-{i}")))
                                .tag(format!("set-prompt-del-{i}"))
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .opacity(0.)
                                .group_hover(group.clone(), |s| s.opacity(1.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(soft))
                                .tooltip(tip(t("common.delete")))
                                .child(icon("trash", 14.).text_color(danger))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.prompt_del = Some(del.clone());
                                    cx.notify();
                                })),
                        )
                })),
        )
    }

    /// 집중 편집기 — 이름 + 큰 지시문 하나, 목록 없이(설정 판을 대신한다)
    pub(crate) fn prompt_editor(&self, e: &PromptEdit, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let empty = amber_gpui::vault::js_trim(&e.text.read(cx).value()).is_empty();
        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .mb(px(16.))
                    .child(field_label(t("settings.prompt.nameLabel"), k))
                    .child(
                        div()
                            .max_w(px(440.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(k.border_strong)
                            .bg(k.surface)
                            .px(px(11.))
                            .py(px(4.))
                            .text_size(px(14.))
                            .tag("set-prompt-name")
                            .child(Input::new(&e.name).appearance(false).px_0()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(field_label(t("settings.prompt.textLabel"), k))
                    .child(
                        div()
                            .max_w(px(440.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(k.border_strong)
                            .bg(k.surface)
                            .px(px(3.))
                            .py(px(4.))
                            .tag("set-prompt-text")
                            .child(
                                Textarea::new(&e.text)
                                    .appearance(false)
                                    .small()
                                    .text_size(px(14.))
                                    .line_height(px(22.)),
                            ),
                    )
                    .child(hint(t("settings.prompt.nameHint"), k)),
            );
        crate::dialog::shell_fixed(
            k,
            640.,
            t(if e.is_new {
                "settings.prompt.new"
            } else {
                "settings.prompt.editTitle"
            }),
            body.into_any_element(),
            vec![
                button("set-prompt-cancel", k, false, false)
                    .child(t("common.cancel"))
                    .on_click(cx.listener(|this, _, window, cx| this.prompt_cancel(window, cx))),
            ],
            vec![
                primary_button("set-prompt-save", k, empty)
                    .child(t("common.save"))
                    .when(!empty, |b| {
                        b.on_click(cx.listener(|this, _, window, cx| this.prompt_save(window, cx)))
                    }),
            ],
            cx.listener(|this, _, window, cx| this.prompt_cancel(window, cx)),
        )
    }

    /// 지우기 확인 — 직접 쓴 글이고 되돌릴 길이 없다
    pub(crate) fn prompt_delete_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let (_, name) = self.settings.prompt_del.clone()?;
        Some(crate::dialog::confirm_delete(
            k,
            t("settings.prompts.deleteTitle"),
            amber_gpui::i18n::t_with("settings.prompts.deleteConfirm", &[("name", "{name}")]),
            name,
            cx.listener(|this, _, _, cx| {
                this.settings.prompt_del = None;
                cx.notify();
            }),
            cx.listener(|this, _, _, cx| {
                if let Some((id, _)) = this.settings.prompt_del.take() {
                    let list: Vec<Prompt> = load_prompts(&this.db)
                        .into_iter()
                        .filter(|p| p.id != id)
                        .collect();
                    save_prompts(&this.db, &list);
                }
                cx.notify();
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{head, prompt_name};
    use crate::note_ai_view::Prompt;

    #[test]
    fn a_nameless_prompt_shows_the_first_24_utf16_units() {
        let p = |label: &str, text: &str| Prompt {
            id: "x".into(),
            label: label.into(),
            text: text.into(),
        };
        assert_eq!(prompt_name(&p("  이름  ", "본문")), "이름");
        assert_eq!(prompt_name(&p(" ", &"가".repeat(30))), "가".repeat(24));
        assert_eq!(head("ab", 24), "ab");
    }
}
