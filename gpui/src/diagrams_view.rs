// 다이어그램 탭 — 지금 앱 DiagramsView.tsx. 왼쪽 트리(노트 탭과 같은 vault_pane), 오른쪽은 고른 .mmd 의 렌더.
// 정본은 vault/diagrams/(또는 고른 작업 폴더)의 실제 파일이다. 창으로 돌아올 때마다 트리를 다시 읽는다(감시는 없다).

use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{button, danger_button, primary_button};
use crate::theme::Tokens;
use crate::vault_pane::{TreeUi, Which, tree_of};

/// 저장 안 한 변경이 있는데 다른 파일/작업 폴더로 가려 했다, 또는 저장하려는데 디스크가 더 새롭다
pub enum DiagramPrompt {
    Unsaved { target: String },
    UnsavedRoot { root: Option<String> },
    Conflict { disk: i64 },
}

pub struct DiagramEdit {
    pub state: Entity<TextareaState>,
    pub dirty: bool,
    debounce: Option<Task<()>>,
}

#[derive(Default)]
pub struct DiagramsState {
    /// 트리 — Deref 로 self.diagrams.selected 처럼 쓴다
    pub ui: TreeUi,
    pub body: String,
    pub mtime: Option<i64>,
    pub read_error: Option<String>,
    pub prompt: Option<DiagramPrompt>,
    /// 편집 중이면 원문 칸 — 미리보기는 캔버스가 350ms 늦춰 그린다
    pub edit: Option<DiagramEdit>,
    /// 캔버스(확대, 이동, 마지막 정상 렌더) — diagram_canvas.rs
    pub canvas: crate::diagram_canvas::Canvas,
}

impl std::ops::Deref for DiagramsState {
    type Target = TreeUi;
    fn deref(&self) -> &TreeUi {
        &self.ui
    }
}

impl std::ops::DerefMut for DiagramsState {
    fn deref_mut(&mut self) -> &mut TreeUi {
        &mut self.ui
    }
}

/// 'ERD/주문.mmd' → '주문'
fn diagram_name(rel: &str) -> String {
    Which::Diagrams.stem(rel.rsplit('/').next().unwrap_or(rel))
}

impl Spike {
    /// 고치던 초안이 있는가(저장 안 한 변경)
    pub(crate) fn diagram_dirty(&self, _cx: &App) -> bool {
        self.diagrams.edit.as_ref().is_some_and(|e| e.dirty)
    }

    pub(crate) fn diagram_draft(&self, cx: &App) -> Option<String> {
        self.diagrams
            .edit
            .as_ref()
            .map(|e| e.state.read(cx).value().to_string())
    }

    /// 트리에서 다른 파일을 누름 — 저장 안 한 변경이 있으면 먼저 묻는다
    pub(crate) fn request_open_diagram(&mut self, rel: &str, cx: &mut Context<Self>) {
        if self.diagram_dirty(cx) {
            self.diagrams.prompt = Some(DiagramPrompt::Unsaved {
                target: rel.to_string(),
            });
            return;
        }
        self.diagrams.edit = None;
        self.open_diagram(rel, cx);
    }

    /// 편집을 접는다 — 미리보기로 바뀐 캔버스를 본문으로 되돌린다
    pub(crate) fn cancel_diagram_edit(&mut self) {
        let d = &mut self.diagrams;
        d.edit = None;
        let body = d.body.clone();
        d.canvas.set_chart(&body);
        d.canvas.fit_pending = true;
        d.canvas.deselect();
    }

    /// ⌘S. force = 디스크가 더 새로운 걸 알고도 덮는다(충돌 창의 '그래도 덮어쓰기')
    pub(crate) fn save_diagram(&mut self, force: bool, cx: &mut Context<Self>) {
        let (Some(rel), Some(draft)) = (self.diagrams.selected.clone(), self.diagram_draft(cx))
        else {
            return;
        };
        if self.diagrams.read_error.is_some() {
            return;
        }
        let tree = tree_of(&self.db, Which::Diagrams);
        // 열 때 잡아 둔 시각보다 디스크가 새로우면 밖(Finder, vim, git)이 먼저 고친 것 — 조용히 덮지 않는다
        if !force
            && let (Some(cur), Some(opened)) = (tree.mtime(&rel), self.diagrams.mtime)
            && cur > opened
        {
            self.diagrams.prompt = Some(DiagramPrompt::Conflict { disk: cur });
            return;
        }
        if let Err(e) = tree.write_file(&rel, &draft) {
            self.diagrams.read_error = Some(e);
            return;
        }
        self.diagrams.edit = None;
        self.open_diagram(&rel, cx);
        self.diagrams.canvas.fit_pending = true;
        self.diagrams.canvas.deselect();
    }

    /// 노드 카드의 '라인 N' — 편집 중이 아니면 편집을 열고, 원문 칸에서 그 줄을 골라 둔다(DiagramsView.jumpToLine)
    pub(crate) fn jump_to_diagram_line(
        &mut self,
        line: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diagrams.edit.is_none() {
            self.start_diagram_edit(window, cx);
        }
        let Some(state) = self.diagrams.edit.as_ref().map(|e| e.state.clone()) else {
            return;
        };
        state.update(cx, |s, cx| {
            let text = s.value().to_string();
            let mut start = 0;
            for (i, l) in text.split('\n').enumerate() {
                if i as i64 + 1 == line {
                    s.set_selected_range(start..start + l.len(), cx);
                    break;
                }
                start += l.len() + 1;
            }
            s.focus(window, cx);
        });
    }

    pub(crate) fn open_diagram(&mut self, rel: &str, _cx: &mut Context<Self>) {
        let tree = tree_of(&self.db, Which::Diagrams);
        let read = tree.read_file(rel);
        let mtime = tree.mtime(rel);
        let d = &mut self.diagrams;
        let dir = vault::parent_of(rel).to_string();
        d.expanded.extend(vault::ancestor_paths(&dir));
        d.active_dir = dir;
        // 다른 파일이면 캔버스를 새로(맞춤부터) — 판의 자리와 열린 찾기 바는 그대로 쓴다(새 그림에서 다시 찾는다)
        if d.selected.as_deref() != Some(rel) || d.canvas.chart.is_empty() {
            let viewport = d.canvas.viewport.clone();
            let search = d.canvas.search.take();
            d.canvas = Default::default();
            d.canvas.viewport = viewport;
            d.canvas.search = search;
        }
        d.selected = Some(rel.to_string());
        match read {
            Ok(body) => {
                d.canvas.set_chart(&body);
                d.body = body;
                d.mtime = mtime;
                d.read_error = None;
            }
            // 못 읽은 파일을 빈 본문으로 열어 두면 그 빈 초안이 ⌘S 로 원본을 덮는다 — 본문은 비우고 오류만
            Err(e) => {
                d.body.clear();
                d.canvas = Default::default();
                d.mtime = None;
                d.read_error = Some(e);
            }
        }
    }

    /// 편집 시작 — 원문 | 캔버스. 타이핑은 350ms 늦춰 캔버스에 그린다(mermaid 다시 그리기가 비싸다)
    pub(crate) fn start_diagram_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.diagrams.selected.is_none() || self.diagrams.read_error.is_some() {
            return;
        }
        let body = self.diagrams.body.clone();
        let state = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx);
            s.set_value(body, window, cx);
            s
        });
        cx.subscribe_in(&state, window, |this, input, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let draft = input.read(cx).value().to_string();
                let dirty = draft != this.diagrams.body;
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(350))
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        s.diagrams.canvas.set_chart(&draft);
                        cx.notify();
                    });
                });
                if let Some(e) = this.diagrams.edit.as_mut() {
                    e.dirty = dirty;
                    e.debounce = Some(task);
                }
                cx.notify();
            }
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        // 지금 앱은 읽기/편집 캔버스가 따로라 바꿀 때마다 새로 맞추고 선택도 풀린다 — 판 폭이 반으로 바뀐다
        self.diagrams.canvas.fit_pending = true;
        self.diagrams.canvas.deselect();
        self.diagrams.edit = Some(DiagramEdit {
            state,
            dirty: false,
            debounce: None,
        });
    }

    pub(crate) fn close_diagram(&mut self) {
        let d = &mut self.diagrams;
        d.edit = None;
        d.selected = None;
        d.body.clear();
        d.canvas = Default::default();
        d.mtime = None;
        d.read_error = None;
    }

    /// 지우기 확인을 지난 뒤 — 고른 파일이 그 안에 있었으면 닫는다
    pub(crate) fn delete_diagram(&mut self, rel: &str) -> Result<(), String> {
        let r = tree_of(&self.db, Which::Diagrams).delete_entry(rel);
        let inside = |p: &str| p == rel || p.starts_with(&format!("{rel}/"));
        if self.diagrams.selected.as_deref().is_some_and(inside) {
            self.close_diagram();
        }
        if inside(&self.diagrams.active_dir) {
            self.diagrams.active_dir = vault::parent_of(rel).to_string();
        }
        self.reload_tree(Which::Diagrams);
        r
    }

    /// 전체 화면 캔버스 — 앱 전체를 덮는다(Esc, 단추로 닫는다)
    pub(crate) fn diagram_fullscreen(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let c = &self.diagrams.canvas;
        (c.fullscreen && self.diagrams.selected.is_some() && self.diagrams.read_error.is_none())
            .then(|| {
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .occlude()
                    .child(self.diagram_canvas(k, cx))
            })
    }

    pub(crate) fn diagrams_body(&self, body_w: f32, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let w = Which::Diagrams;
        let width = crate::pane::effective(self.diagrams.width, w.pane().min, body_w);
        div()
            .size_full()
            .flex()
            .child(self.tree_pane(w, width, body_w, k, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(px(8.))
                    .pb(px(8.))
                    .pr(px(8.))
                    .pl(px(4.))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .overflow_hidden()
                            .border_1()
                            .border_color(k.glass_edge)
                            .rounded(px(12.))
                            .bg(crate::note_ai_view::glass(k.glass_fill))
                            .shadow(k.glass_shadow(false))
                            .child(match self.diagrams.selected.clone() {
                                Some(rel) => self.diagram_detail(&rel, k, cx).into_any_element(),
                                None => self.diagrams_empty(k, cx).into_any_element(),
                            }),
                    ),
            )
    }

    /// 컴팩트 머리(.dgm-head: 경로 줄 + 제목 | 동작) — 캔버스에 자리를 최대로 준다
    fn diagram_detail(&self, rel: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let d = &self.diagrams;
        let name = diagram_name(rel);
        let crumb = std::iter::once(t("diagrams.title"))
            .chain(
                vault::parent_of(rel)
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            )
            .collect::<Vec<_>>()
            .join(" \u{203a} ");
        let ok = d.read_error.is_none();
        let head = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .px(px(14.))
            .py(px(9.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .mb(px(1.))
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(icon("folder", 12.).text_color(k.text_3))
                            .child(crumb),
                    )
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(k.text)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(name.clone()),
                    ),
            )
            .child(div().flex_1())
            // DDL → ERD(AI), 편집은 D3c/D3e 에서
            .child(
                button("dgm-ai", k, false, true)
                    .child(icon("sparkles", 14.).text_color(k.text_3))
                    .child("DDL \u{2192} ERD"),
            )
            .children(if d.edit.is_some() {
                vec![
                    primary_button("dgm-save", k, false)
                        .child(format!("{} (\u{2318}S)", t("common.save")))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_diagram(false, cx);
                            cx.notify();
                        })),
                    button("dgm-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_diagram_edit();
                            cx.notify();
                        })),
                ]
            } else {
                vec![
                    button("dgm-edit", k, false, !ok)
                        .child(icon("pencil", 14.).text_color(if ok { k.text_2 } else { k.text_3 }))
                        .child(t("diagrams.edit"))
                        .when(ok, |d| {
                            d.on_click(cx.listener(|this, _, window, cx| {
                                this.start_diagram_edit(window, cx);
                                cx.notify();
                            }))
                        }),
                ]
            })
            .child({
                let (rel, name) = (rel.to_string(), name.clone());
                danger_button("dgm-delete", k)
                    .child(icon("trash", 14.).text_color(k.danger))
                    .child(t("common.delete"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(crate::day_card::Confirm::Diagram {
                            rel: rel.clone(),
                            name: name.clone(),
                            is_dir: false,
                        });
                        cx.notify();
                    }))
            });
        let error_note = |msg: String| {
            crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                .mx(px(14.))
                .my(px(10.))
                .child(msg)
        };
        // 읽기 화면의 캔버스는 프레임 없이 모서리까지, 편집이면 원문 | 테두리 있는 캔버스(.note-edit-split).
        // 전체 화면이면 뿌리 판이 그린다
        let canvas = match (&d.read_error, &d.edit) {
            (Some(e), _) => {
                error_note(t_with("diagrams.readError", &[("msg", e)])).into_any_element()
            }
            (None, Some(edit)) => div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(10.))
                .px(px(14.))
                .py(px(10.))
                .child(
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
                        .font_family(crate::markdown::MONO)
                        .text_size(px(13.))
                        .line_height(relative(1.6))
                        .child(Textarea::new(&edit.state).appearance(false).h(relative(1.))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(k.border)
                        .overflow_hidden()
                        .when(!d.canvas.fullscreen, |x| {
                            x.child(self.diagram_canvas(k, cx))
                        }),
                )
                .into_any_element(),
            (None, None) if d.canvas.fullscreen => div().flex_1().into_any_element(),
            (None, None) => self.diagram_canvas(k, cx).into_any_element(),
        };
        let mut meta = format!("{}/{rel}", self.root_name(Which::Diagrams));
        if let Some(ms) = d.mtime.filter(|_| ok) {
            let now = chrono::Local::now().timestamp_millis();
            // 지금 앱과 같은 구분자(U+00B7)
            meta.push_str(" \u{b7} ");
            meta.push_str(&t_with(
                "diagrams.meta.modified",
                &[("ago", &crate::notes_view::time_ago(ms, now))],
            ));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(head)
            .child(canvas)
            // 아래 메타 = 슬림 바(.detail.dgm .detail-meta)
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(14.))
                    .pt(px(6.))
                    .pb(px(7.))
                    .border_t_1()
                    .border_color(k.glass_edge)
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(meta),
            )
    }

    /// 저장 안 한 변경(다른 파일, 작업 폴더) / 밖에서 먼저 고침(덮기, 다시 읽기)
    pub(crate) fn diagram_prompt(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let prompt = self.diagrams.prompt.as_ref()?;
        let close = || {
            cx.listener(|this, _, _, cx| {
                this.diagrams.prompt = None;
                cx.notify();
            })
        };
        let keep = || {
            button("unsaved-keep", k, false, false)
                .child(t("common.unsaved.keep"))
                .on_click(close())
        };
        let unsaved = |discard: Stateful<Div>| {
            crate::dialog::shell(
                k,
                440.,
                t("common.unsaved.title"),
                div().child(t("common.unsaved.body")).into_any_element(),
                vec![keep(), discard],
                close(),
            )
        };
        Some(match prompt {
            DiagramPrompt::Unsaved { target } => {
                let target = target.clone();
                unsaved(
                    danger_button("unsaved-discard", k)
                        .child(t("common.unsaved.discard"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.diagrams.prompt = None;
                            this.diagrams.edit = None;
                            this.open_diagram(&target, cx);
                            cx.notify();
                        })),
                )
            }
            DiagramPrompt::UnsavedRoot { root } => {
                let root = root.clone();
                unsaved(
                    danger_button("unsaved-discard", k)
                        .child(t("common.unsaved.discard"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.diagrams.prompt = None;
                            this.apply_root(Which::Diagrams, root.clone());
                            cx.notify();
                        })),
                )
            }
            DiagramPrompt::Conflict { disk } => {
                let now = chrono::Local::now().timestamp_millis();
                // 지금 앱과 같은 구분자(U+00B7)
                let body = format!(
                    "{} \u{b7} {}.",
                    t("diagrams.conflict.body1"),
                    t_with(
                        "diagrams.conflict.diskMtime",
                        &[("ago", &crate::notes_view::time_ago(*disk, now))]
                    )
                );
                crate::dialog::shell(
                    k,
                    440.,
                    t("diagrams.conflict.title"),
                    div()
                        .flex()
                        .flex_col()
                        .child(body)
                        .child(t("diagrams.conflict.body2"))
                        .into_any_element(),
                    vec![
                        keep(),
                        danger_button("conflict-reload", k)
                            .child(t("diagrams.conflict.reread"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.diagrams.prompt = None;
                                this.diagrams.edit = None;
                                if let Some(rel) = this.diagrams.selected.clone() {
                                    this.open_diagram(&rel, cx);
                                }
                                cx.notify();
                            })),
                        danger_button("conflict-overwrite", k)
                            .child(t("diagrams.conflict.overwrite"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.diagrams.prompt = None;
                                this.save_diagram(true, cx);
                                cx.notify();
                            })),
                    ],
                    close(),
                )
            }
        })
    }

    /// 고른 파일이 없을 때(.notes-empty) — 만들기 단추와 두 가지 팁
    fn diagrams_empty(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let tip = |icon_name: &str, text: String| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(12.))
                .text_color(k.text_3)
                .child(icon(icon_name, 12.).text_color(k.text_3))
                .child(text)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .child(
                div()
                    .size(px(68.))
                    .mb(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(18.))
                    .bg(k.surface_2)
                    .border_1()
                    .border_color(k.border)
                    .child(icon("workflow", 30.).text_color(k.text_3)),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .text_color(k.text)
                    .child(t("diagrams.title")),
            )
            .child(
                div()
                    .mt(px(6.))
                    .mb(px(18.))
                    .max_w(px(380.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_size(px(13.))
                    .line_height(relative(1.7))
                    .text_color(k.text_3)
                    .child(t("diagrams.empty.sub1"))
                    .child(t("diagrams.empty.sub2")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        primary_button("dgm-empty-new-file", k, false)
                            .child(icon("file-plus", 14.).text_color(k.text))
                            .child(t("diagrams.newFile"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_name_modal(
                                    Which::Diagrams,
                                    crate::note_names::NameKind::NewFile,
                                    None,
                                    window,
                                    cx,
                                );
                                cx.notify();
                            })),
                    )
                    .child(
                        button("dgm-empty-new-folder", k, false, false)
                            .child(icon("folder-plus", 14.).text_color(k.text_2))
                            .child(t("diagrams.newFolder"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_name_modal(
                                    Which::Diagrams,
                                    crate::note_names::NameKind::NewFolder,
                                    None,
                                    window,
                                    cx,
                                );
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .mt(px(18.))
                    .flex()
                    .gap(px(14.))
                    .child(tip("pencil", t("diagrams.empty.tipEdit")))
                    .child(tip("expand", t("diagrams.empty.tipZoom"))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::diagram_name;

    #[test]
    fn names_drop_the_extension() {
        assert_eq!(diagram_name("ERD/주문.mmd"), "주문");
        assert_eq!(diagram_name("flow.mermaid"), "flow");
    }
}
