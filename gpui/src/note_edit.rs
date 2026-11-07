// 노트 편집 — 지금 앱 NotesView.tsx 의 편집 모드. 왼쪽 원문(고정폭 13px), 오른쪽 미리보기(350ms 늦춰 다시 그린다 —
// mermaid 펜스 안에서 글자마다 다시 그리지 않게). ⌘S 저장, 취소는 묻지 않고 버린다(지금 앱과 같다).
// 저장하려는 순간 디스크가 연 때보다 새로우면(Obsidian, vim, git 이 먼저 고침) 조용히 덮지 않고 묻는다.
// 저장 안 한 변경이 있는데 다른 노트를 열면 '계속 편집 / 버리고 옮기기'를 묻는다.

use std::ops::Range;
use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
use gpui_kit::component::input::{InputEditsEvent, InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
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
    /// 원문 칸에 고른 구간이 있나 — '선택 수정' 단추가 켜진다(바뀔 때만 다시 그린다)
    pub has_sel: bool,
    /// 미리보기를 늦춰 그리는 작업 — 새 글자가 오면 버리고 다시 건다
    debounce: Option<Task<()>>,
    /// 그림을 넣다 실패했다(.error-note) — 다음에 넣을 때 지운다
    pub image_error: Option<String>,
    /// Finder 에서 끌어 온 파일이 원문 칸 위에 있다(그 경로들) — 테두리가 점선이 되고, 놓으면 넣는다
    pub drop: Option<ExternalPaths>,
}

// 토글 단축키 — 지금 앱 mdDetails.ts 의 toggleShortcut / toggleTitleEnter. 자리는 바이트(입력칸과 같다).
const SUMMARY_OPEN: &str = "<details>\n<summary>";
const SUMMARY_CLOSE: &str = "</summary>\n\n\n\n</details>";

fn line_start(v: &str, caret: usize) -> usize {
    v[..caret].rfind('\n').map_or(0, |i| i + 1)
}

/// 줄 맨 앞의 `>>` 뒤 스페이스 → 토글 뼈대. 그 줄에 이미 쓴 글은 제목이 된다.
/// (바꿀 구간, 넣을 글, 커서 = 제목 자리). 아니면 None(평소 스페이스)
pub(crate) fn toggle_shortcut(v: &str, caret: usize) -> Option<(Range<usize>, String, usize)> {
    let start = line_start(v, caret);
    if &v[start..caret] != ">>" {
        return None;
    }
    let end = v[caret..].find('\n').map_or(v.len(), |i| caret + i);
    let text = format!("{SUMMARY_OPEN}{}{SUMMARY_CLOSE}", &v[caret..end]);
    Some((start..end, text, start + SUMMARY_OPEN.len()))
}

/// 토글 제목 줄에서 Enter — 제목에 줄바꿈을 넣는 대신 본문 자리로 커서를 옮긴다.
/// 뼈대 모양(제목 줄 뒤에 빈 줄 둘)일 때만 그 둘째 빈 줄 머리. 아니면 None(평소 Enter)
pub(crate) fn toggle_title_enter(v: &str, caret: usize) -> Option<usize> {
    let start = line_start(v, caret);
    let line_end = caret + v[caret..].find('\n')?;
    // /^<summary>.*<\/summary>$/ — JS 의 `.` 은 줄 끝 문자를 넘지 않는다
    let mid = v[start..line_end]
        .strip_prefix("<summary>")?
        .strip_suffix("</summary>")?;
    if mid.contains(['\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    v[line_end..].starts_with("\n\n\n").then_some(line_end + 2)
}

actions!(note_edit, [SrcEnter, SrcPaste]);

/// 원문 칸의 Enter 와 ⌘V 를 입력칸보다 먼저 받는다 — 같은 깊이면 나중에 묶은 바인딩이 이긴다.
/// gpui_kit::init 뒤에 부른다. 토글 제목 줄이 아니면 Enter 를, 그림이 아니면 ⌘V 를 입력칸으로 넘긴다
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", SrcEnter, Some("NoteSrc > Input")),
        KeyBinding::new("cmd-v", SrcPaste, Some("NoteSrc > Input")),
    ]);
}

/// 넣을 그림 하나 — 클립보드의 그림 바이트, 또는 Finder 의 파일
enum ImageJob {
    Bytes(Vec<u8>, &'static str),
    File(String),
}

pub enum NotePrompt {
    /// 저장 안 한 변경이 있는데 다른 노트를 열려 했다
    Unsaved { target: String },
    /// 저장 안 한 변경이 있는데 작업 폴더를 바꾸려 했다(None = 기본 보관함)
    UnsavedRoot { root: Option<String> },
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
        // `>>` 뒤 스페이스 — 키를 가로채지 않고 들어온 글자를 본다. 한글 입력기가 켜져 있으면
        // macOS 가 스페이스를 키 이벤트보다 입력기로 먼저 보내서, 키 리스너는 그걸 못 본다
        cx.subscribe_in(
            &state,
            window,
            |_, input, ev: &InputEditsEvent, window, cx| {
                let [(e, _)] = ev.edits() else {
                    return;
                };
                if e.old_end_byte != e.start_byte || e.new_end_byte != e.start_byte + 1 {
                    return;
                }
                let (value, sel) = {
                    let s = input.read(cx);
                    (s.value(), s.selected_range())
                };
                if sel != (e.new_end_byte..e.new_end_byte)
                    || value.as_bytes().get(e.start_byte) != Some(&b' ')
                {
                    return;
                }
                let mut before = value.to_string();
                before.remove(e.start_byte);
                let Some((r, text, caret)) = toggle_shortcut(&before, e.start_byte) else {
                    return;
                };
                input.update(cx, |s, cx| {
                    s.set_selected_range(r.start..r.end + 1, cx);
                    s.replace(text, window, cx);
                    s.set_selected_range(caret..caret, cx);
                });
            },
        )
        .detach();
        // 고른 구간이 생기거나 사라질 때만 판을 다시 그린다(커서가 움직일 때마다가 아니라)
        cx.observe(&state, |this, state, cx| {
            let has = !state.read(cx).selected_range().is_empty();
            if let Some(e) = this.notes.edit.as_mut()
                && e.has_sel != has
            {
                e.has_sel = has;
                cx.notify();
            }
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        let text = hex(Tokens::get(cx).text);
        let (preview, _) = crate::notes_view::build_blocks(&body, &text, cx);
        self.notes.edit = Some(NoteEdit {
            state,
            preview,
            dirty: false,
            has_sel: false,
            debounce: None,
            image_error: None,
            drop: None,
        });
    }

    /// 원문 칸의 Enter — 토글 제목 줄이면 본문 자리로, 아니면 입력칸에 넘긴다(줄바꿈)
    fn src_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.notes.edit.as_ref().map(|e| e.state.clone()) else {
            cx.propagate();
            return;
        };
        let at = state.update(cx, |s, cx| {
            let sel = s.selected_range();
            if !sel.is_empty() || s.marked_text_range(window, cx).is_some() {
                return None;
            }
            toggle_title_enter(&s.value(), sel.start)
        });
        match at {
            Some(at) => state.update(cx, |s, cx| s.set_selected_range(at..at, cx)),
            None => cx.propagate(),
        }
    }

    /// ⌘V — 클립보드에 그림(스크린샷)이나 Finder 에서 복사한 그림 파일이 있으면 노트 폴더의 `_assets/` 에 쓰고
    /// 커서 자리에 링크를 끼운다. 글자 붙여넣기는 입력칸에 넘긴다
    fn src_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let jobs: Vec<ImageJob> = cx
            .read_from_clipboard()
            .map(|c| {
                c.entries()
                    .iter()
                    .flat_map(|e| match e {
                        ClipboardEntry::Image(img) => {
                            vec![ImageJob::Bytes(img.bytes.clone(), img.format.mime_type())]
                        }
                        ClipboardEntry::ExternalPaths(p) => image_files(p),
                        ClipboardEntry::String(_) => vec![],
                    })
                    .collect()
            })
            .unwrap_or_default();
        if jobs.is_empty() || self.notes.selected.is_none() {
            cx.propagate();
            return;
        }
        self.add_images(jobs, window, cx);
    }

    /// Finder 에서 끄는 동안 — 원문 칸 위에 있나. GPUI 의 drag_over/on_drop 은 hover 로 판단하는데, 마지막
    /// 입력이 키보드면 hover 가 꺼지고 파일 끌기는 그걸 마우스로 되돌리지 않는다(치다가 끌어 오면 놓기가
    /// 무시된다). 그래서 끌기 이동은 칸의 자리로, 놓기는 창의 mouseup 으로 직접 판단한다
    fn src_drag_move(&mut self, e: &DragMoveEvent<ExternalPaths>, cx: &mut Context<Self>) {
        let over = e
            .bounds
            .contains(&e.event.position)
            .then(|| e.drag(cx).clone());
        if let Some(ed) = self.notes.edit.as_mut()
            && ed.drop.is_some() != over.is_some()
        {
            ed.drop = over;
            cx.notify();
        }
    }

    /// 창의 mouseup — 원문 칸 위에서 끌던 파일을 놓았다. 그림 파일만 받는다
    fn src_drop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paths) = self.notes.edit.as_mut().and_then(|e| e.drop.take()) else {
            return;
        };
        let jobs = image_files(&paths);
        if jobs.is_empty() {
            if let Some(e) = self.notes.edit.as_mut() {
                e.image_error = Some(t("notes.image.unsupported"));
            }
            cx.notify();
            return;
        }
        self.add_images(jobs, window, cx);
    }

    /// 차례로 저장해 성공한 것만 끼운다. 하나가 실패해도 나머지는 넣는다
    fn add_images(&mut self, jobs: Vec<ImageJob>, window: &mut Window, cx: &mut Context<Self>) {
        use amber_gpui::note_assets as na;
        let (Some(rel), Some(state)) = (
            self.notes.selected.clone(),
            self.notes.edit.as_ref().map(|e| e.state.clone()),
        ) else {
            return;
        };
        let dir = vault::note_dir_path(&vault::notes(&self.db), &rel);
        let now = self.db.now();
        let mut links = Vec::new();
        let mut failure = None;
        for job in jobs {
            let saved = match job {
                ImageJob::Bytes(data, mime) => {
                    na::save_note_image(&self.db, &dir, &data, mime, now)
                }
                ImageJob::File(path) => na::import_image_file(&self.db, &dir, &path, now),
            };
            match saved {
                Ok(name) => links.push(na::image_markdown(&name)),
                Err(e) => failure = Some(e),
            }
        }
        if !links.is_empty() {
            state.update(cx, |s, cx| {
                s.focus(window, cx);
                let sel = s.selected_range();
                let text = na::image_insert_text(&s.value(), sel.start, sel.end, &links);
                s.replace(text, window, cx);
            });
        }
        if let Some(e) = self.notes.edit.as_mut() {
            e.image_error = failure.map(|msg| t_with("notes.image.saveFailed", &[("msg", &msg)]));
        }
        cx.notify();
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let text = hex(Tokens::get(cx).text);
        if let Some(e) = self.notes.edit.as_mut() {
            let draft = e.state.read(cx).value().to_string();
            e.preview = crate::notes_view::build_blocks(&draft, &text, cx).0;
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
                    .id("note-src")
                    .tag("note-src")
                    .key_context("NoteSrc")
                    .on_action(
                        cx.listener(|this, _: &SrcEnter, window, cx| this.src_enter(window, cx)),
                    )
                    .on_action(
                        cx.listener(|this, _: &SrcPaste, window, cx| this.src_paste(window, cx)),
                    )
                    .on_drag_move(
                        cx.listener(|this, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                            this.src_drag_move(e, cx)
                        }),
                    )
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    // Finder 에서 그림을 끌어 올렸을 때 — 놓을 수 있는 자리라는 표시(.note-textarea.is-drop)
                    .when(edit.drop.is_some(), |d| {
                        d.border_color(k.text_2).border_dashed()
                    })
                    .bg(k.surface)
                    .py(px(9.))
                    .px(px(11.))
                    .font_family(MONO)
                    .text_size(px(13.))
                    .line_height(relative(1.6))
                    // 입력칸은 제 글자 크기(text_sm 14px, 줄 1.25rem)를 스스로 깐다 — 칸에도 같은 값을 줘야 13px, 1.6 이 된다
                    .child(
                        Textarea::new(&edit.state)
                            .appearance(false)
                            .h(relative(1.))
                            .text_size(px(13.))
                            .line_height(relative(1.6)),
                    )
                    .child({
                        let this = cx.entity().downgrade();
                        // 편집 중 찾기의 칠(칸 뒤에 그려 이번 그림의 자리를 쓴다)
                        let find = self
                            .notes
                            .page_find
                            .as_ref()
                            .filter(|f| f.in_src && !f.src.is_empty())
                            .map(|f| (edit.state.clone(), f.src.clone(), f.idx, k.dark));
                        canvas(
                            |_, _, _| {},
                            move |_, _, window, cx| {
                                if let Some((state, hits, cur, dark)) = &find {
                                    crate::note_page_find::paint_src_find(
                                        state, hits, *cur, *dark, window, cx,
                                    );
                                }
                                window.on_mouse_event(
                                    move |_: &MouseUpEvent, phase, window, cx| {
                                        if phase == DispatchPhase::Bubble {
                                            let _ = this.update(cx, |s, cx| s.src_drop(window, cx));
                                        }
                                    },
                                );
                            },
                        )
                        .absolute()
                        .size_0()
                    }),
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
            // 초안에서 AI 작성 — 지금 초안이 [현재 본문]이 된다
            .child(if self.ai_connected() {
                button("note-ai", k, false, false)
                    .child(crate::assets::icon("sparkles", 14.).text_color(k.text_2))
                    .child(t("notes.aiWrite"))
                    .on_click(cx.listener(|this, _, _, cx| this.open_ai_modal(cx)))
            } else {
                off("note-ai", "sparkles", t("notes.aiWrite"))
            })
            // 고른 구간만 고쳐 쓴다 — 출력이 짧아 초 단위로 끝난다. 구간은 누르는 순간의 원문 선택(바이트 = 소스 좌표)
            .child(
                if self.ai_connected() && self.notes.edit.as_ref().is_some_and(|e| e.has_sel) {
                    button("note-span-ai", k, false, false)
                        .child(crate::assets::icon("scissors", 14.).text_color(k.text_2))
                        .child(t("notes.spanAi.selBtn"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let sel = this
                                .notes
                                .edit
                                .as_ref()
                                .map(|e| e.state.read(cx).selected_range());
                            this.open_span_modal(crate::note_span_ai::SpanMode::Selection, sel, cx);
                        }))
                } else {
                    off("note-span-ai", "scissors", t("notes.spanAi.selBtn"))
                },
            )
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
                    440.,
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
            NotePrompt::UnsavedRoot { root } => {
                let root = root.clone();
                dialog::shell(
                    k,
                    440.,
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
                                this.apply_root(crate::vault_pane::Which::Notes, root.clone());
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
                    440.,
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

/// 그림 파일만 고른다(확장자로) — 아니면 빈 목록
fn image_files(p: &ExternalPaths) -> Vec<ImageJob> {
    p.paths()
        .iter()
        .filter_map(|path| path.to_str())
        .filter(|path| amber_gpui::note_assets::mime_for_name(path).is_some())
        .map(|path| ImageJob::File(path.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{toggle_shortcut, toggle_title_enter};

    /// 편집 한 번을 적용한 결과 — 글과 커서 자리
    fn apply(v: &str, caret: usize) -> Option<(String, usize)> {
        let (r, text, at) = toggle_shortcut(v, caret)?;
        Some((format!("{}{text}{}", &v[..r.start], &v[r.end..]), at))
    }

    #[test]
    fn shortcut_at_line_start_builds_the_skeleton_with_the_caret_in_the_title() {
        let v = "앞 문단\n>>";
        let (next, at) = apply(v, v.len()).unwrap();
        assert_eq!(
            next,
            "앞 문단\n<details>\n<summary></summary>\n\n\n\n</details>"
        );
        assert!(next[at..].starts_with("</summary>"));
    }

    #[test]
    fn text_already_on_the_line_becomes_the_title_and_the_next_line_stays() {
        let (next, _) = apply(">>계좌 간 송금\n다음 줄", 2).unwrap();
        assert_eq!(
            next,
            "<details>\n<summary>계좌 간 송금</summary>\n\n\n\n</details>\n다음 줄"
        );
    }

    #[test]
    fn shortcut_then_title_then_enter_then_body_reads_as_a_toggle() {
        let (next, at) = apply(">>", 2).unwrap();
        let titled = format!("{}제목{}", &next[..at], &next[at..]);
        let enter = toggle_title_enter(&titled, at + "제목".len()).unwrap();
        let body = format!("{}본문{}", &titled[..enter], &titled[enter..]);
        assert_eq!(
            body,
            "<details>\n<summary>제목</summary>\n\n본문\n\n</details>"
        );
    }

    #[test]
    fn a_single_gt_a_mid_line_pair_or_a_prefixed_pair_is_left_alone() {
        assert!(toggle_shortcut(">", 1).is_none());
        assert!(toggle_shortcut("a >>", 4).is_none());
        assert!(toggle_shortcut("x>>", 3).is_none());
        assert!(toggle_shortcut(">>>", 3).is_none());
    }

    #[test]
    fn title_enter_jumps_one_blank_line_into_the_body() {
        let v = "<details>\n<summary>제목</summary>\n\n\n\n</details>";
        let at = toggle_title_enter(v, v.find("</summary>").unwrap()).unwrap();
        assert_eq!(&v[..at], "<details>\n<summary>제목</summary>\n\n");
    }

    #[test]
    fn title_enter_leaves_other_lines_and_filled_toggles_alone() {
        assert!(toggle_title_enter("그냥 글\n\n\n", "그냥".len()).is_none());
        let filled = "<details>\n<summary>제목</summary>\n\n본문\n\n</details>";
        assert!(toggle_title_enter(filled, filled.find("</summary>").unwrap()).is_none());
    }
}
