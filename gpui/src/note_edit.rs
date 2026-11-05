// 노트 편집 — 지금 앱 NotesView.tsx 의 편집 모드. 왼쪽 원문(고정폭 13px), 오른쪽 미리보기(350ms 늦춰 다시 그린다 —
// mermaid 펜스 안에서 글자마다 다시 그리지 않게). ⌘S 저장, 취소는 묻지 않고 버린다(지금 앱과 같다).
// 저장하려는 순간 디스크가 연 때보다 새로우면(Obsidian, vim, git 이 먼저 고침) 조용히 덮지 않고 묻는다.
// 저장 안 한 변경이 있는데 다른 노트를 열면 '계속 편집 / 버리고 옮기기'를 묻는다.

use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::*;

use crate::day_card::{Tag, button, danger_button};
use crate::markdown::MONO;
use crate::notes_view::Block;
use crate::theme::Tokens;
use crate::{Spike, dialog, hex};

pub struct NoteEdit {
    pub state: Entity<TextareaState>,
    pub preview: Vec<Block>,
    pub dirty: bool,
    /// 미리보기를 늦춰 그리는 작업 — 새 글자가 오면 버리고 다시 건다
    debounce: Option<Task<()>>,
}

pub enum NotePrompt {
    /// 저장 안 한 변경이 있는데 다른 노트를 열려 했다
    Unsaved { target: String },
    /// 저장하려는데 디스크가 더 새롭다(그 시각)
    Conflict { disk: i64 },
}

impl Spike {
    pub(crate) fn start_note_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.notes.body.clone();
        let state = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx);
            s.set_value(body.clone(), window, cx);
            s
        });
        cx.subscribe_in(&state, window, |this, input, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let dirty = input.read(cx).value().as_ref() != this.notes.body;
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(350))
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        s.refresh_preview(cx);
                        cx.notify();
                    });
                });
                if let Some(e) = this.notes.edit.as_mut() {
                    e.dirty = dirty;
                    e.debounce = Some(task);
                }
                cx.notify();
            }
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        let text = hex(Tokens::get(cx).text);
        let (preview, _) = crate::notes_view::build_blocks(&body, &text);
        self.notes.edit = Some(NoteEdit {
            state,
            preview,
            dirty: false,
            debounce: None,
        });
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let text = hex(Tokens::get(cx).text);
        if let Some(e) = self.notes.edit.as_mut() {
            let draft = e.state.read(cx).value().to_string();
            e.preview = crate::notes_view::build_blocks(&draft, &text).0;
        }
    }

    /// 초안 — 편집 중이 아니면 None
    pub(crate) fn note_draft(&self, cx: &App) -> Option<String> {
        self.notes
            .edit
            .as_ref()
            .map(|e| e.state.read(cx).value().to_string())
    }

    /// ⌘S. force = 디스크가 더 새로운 걸 알고도 덮는다(충돌 창의 '그래도 덮기')
    pub(crate) fn save_note(&mut self, force: bool, cx: &mut Context<Self>) {
        let (Some(rel), Some(draft)) = (self.notes.selected.clone(), self.note_draft(cx)) else {
            return;
        };
        let tree = vault::notes(&self.db);
        if !force
            && let (Some(cur), Some(opened)) = (tree.mtime(&rel), self.notes.mtime)
            && cur > opened
        {
            self.notes.prompt = Some(NotePrompt::Conflict { disk: cur });
            return;
        }
        if let Err(e) = tree.write_file(&rel, &draft) {
            self.notes.read_error = Some(e);
            return;
        }
        self.notes.edit = None;
        self.open_note(&rel, cx);
    }

    /// 트리에서 다른 노트를 누름 — 저장 안 한 변경이 있으면 먼저 묻는다
    pub(crate) fn request_open_note(&mut self, rel: &str, cx: &mut Context<Self>) {
        if self.notes.edit.as_ref().is_some_and(|e| e.dirty) {
            self.notes.prompt = Some(NotePrompt::Unsaved {
                target: rel.to_string(),
            });
            return;
        }
        self.notes.edit = None;
        self.open_note(rel, cx);
    }

    /// 편집 판 — 원문 | 미리보기(.note-edit-split, 두 칸 1:1, 틈 14px). 둘 다 제 안에서 굴린다
    pub(crate) fn note_edit_split(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let Some(edit) = self.notes.edit.as_ref() else {
            return div();
        };
        let on_link = self.on_link(cx);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .gap(px(14.))
            .child(
                // .textarea.note-textarea — 고정폭 13px, 줄 높이 1.6, 강한 테
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .py(px(9.))
                    .px(px(11.))
                    .font_family(MONO)
                    .text_size(px(13.))
                    .line_height(relative(1.6))
                    .child(Textarea::new(&edit.state).appearance(false).h(relative(1.))),
            )
            .child(
                // .note-preview — 원문 칸과 세로 여백(9px)이 같아야 두 칸의 첫 줄이 맞는다
                div()
                    .id("note-preview")
                    .tag("note-preview")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(k.border)
                    .bg(k.surface)
                    .py(px(9.))
                    .px(px(18.))
                    .flex()
                    .flex_col()
                    .children(
                        edit.preview.iter().enumerate().map(|(i, b)| {
                            self.note_block(b, 1_000_000 + i as u64, &on_link, k, cx)
                        }),
                    ),
            )
    }

    /// 편집 중의 동작 줄 왼쪽 — 저장(⌘S), 취소, AI 작성, 선택 수정
    pub(crate) fn note_edit_actions(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let off = |id: &'static str, icon_name: &'static str, label: String| {
            button(id, k, false, true)
                .child(crate::assets::icon(icon_name, 14.).text_color(k.text_3))
                .child(label)
        };
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                crate::day_card::primary_button("note-save", k, false)
                    .child(t("notes.saveCmd"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_note(false, cx);
                        cx.notify();
                    })),
            )
            .child(
                button("note-cancel", k, false, false)
                    .child(t("common.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notes.edit = None;
                        cx.notify();
                    })),
            )
            // AI 작성, 선택 수정은 노트 AI 를 붙일 때(D2g)
            .child(off("note-ai", "sparkles", t("notes.aiWrite")))
            .child(off("note-span-ai", "scissors", t("notes.spanAi.selBtn")))
    }

    /// 저장 안 한 변경 / 디스크 충돌 확인 창
    pub(crate) fn note_prompt(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let prompt = self.notes.prompt.as_ref()?;
        let close = cx.listener(|this, _, _, cx| {
            this.notes.prompt = None;
            cx.notify();
        });
        Some(match prompt {
            NotePrompt::Unsaved { target } => {
                let target = target.clone();
                dialog::shell(
                    k,
                    t("common.unsaved.title"),
                    div().child(t("common.unsaved.body")).into_any_element(),
                    vec![
                        button("unsaved-keep", k, false, false)
                            .child(t("common.unsaved.keep"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notes.prompt = None;
                                cx.notify();
                            })),
                        danger_button("unsaved-discard", k)
                            .child(t("common.unsaved.discard"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.notes.prompt = None;
                                this.notes.edit = None;
                                this.open_note(&target, cx);
                                cx.notify();
                            })),
                    ],
                    close,
                )
            }
            NotePrompt::Conflict { disk } => {
                let now = chrono::Local::now().timestamp_millis();
                // 지금 앱과 같은 구분자(U+00B7)
                let body = format!(
                    "{} \u{b7} {}.",
                    t("notes.conflict.body"),
                    t_with(
                        "notes.conflict.diskTime",
                        &[("time", &crate::notes_view::time_ago(*disk, now))]
                    )
                );
                dialog::shell(
                    k,
                    t("notes.conflict.title"),
                    div()
                        .flex()
                        .flex_col()
                        .child(body)
                        .child(t("notes.conflict.warn"))
                        .into_any_element(),
                    vec![
                        button("conflict-keep", k, false, false)
                            .child(t("notes.keepEditing"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notes.prompt = None;
                                cx.notify();
                            })),
                        danger_button("conflict-reload", k)
                            .child(t("notes.conflict.reload"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notes.prompt = None;
                                this.notes.edit = None;
                                if let Some(rel) = this.notes.selected.clone() {
                                    this.open_note(&rel, cx);
                                }
                                cx.notify();
                            })),
                        danger_button("conflict-overwrite", k)
                            .child(t("notes.conflict.overwrite"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notes.prompt = None;
                                this.save_note(true, cx);
                                cx.notify();
                            })),
                    ],
                    close,
                )
            }
        })
    }
}
