// 다이어그램 탭 — 지금 앱 DiagramsView.tsx. 왼쪽 트리(노트 탭과 같은 vault_pane), 오른쪽은 고른 .mmd 의 렌더.
// 정본은 vault/diagrams/(또는 고른 작업 폴더)의 실제 파일이다. 창으로 돌아올 때마다 트리를 다시 읽는다(감시는 없다).

use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
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

#[derive(Default)]
pub struct DiagramsState {
    /// 트리 — Deref 로 self.diagrams.selected 처럼 쓴다
    pub ui: TreeUi,
    pub body: String,
    pub mtime: Option<i64>,
    pub read_error: Option<String>,
    pub prompt: Option<DiagramPrompt>,
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
    /// 고치던 초안이 있는가 — 편집(D3c)이 붙기 전에는 늘 없다
    pub(crate) fn diagram_dirty(&self, _cx: &App) -> bool {
        false
    }

    pub(crate) fn request_open_diagram(&mut self, rel: &str, cx: &mut Context<Self>) {
        if self.diagram_dirty(cx) {
            self.diagrams.prompt = Some(DiagramPrompt::Unsaved {
                target: rel.to_string(),
            });
            return;
        }
        self.open_diagram(rel, cx);
    }

    pub(crate) fn open_diagram(&mut self, rel: &str, _cx: &mut Context<Self>) {
        let tree = tree_of(&self.db, Which::Diagrams);
        let read = tree.read_file(rel);
        let mtime = tree.mtime(rel);
        let d = &mut self.diagrams;
        let dir = vault::parent_of(rel).to_string();
        d.expanded.extend(vault::ancestor_paths(&dir));
        d.active_dir = dir;
        // 다른 파일이면 캔버스를 새로(맞춤부터) — 판의 자리는 그대로 쓴다
        if d.selected.as_deref() != Some(rel) || d.canvas.chart.is_empty() {
            let viewport = d.canvas.viewport.clone();
            d.canvas = Default::default();
            d.canvas.viewport = viewport;
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

    /// 편집 시작 — D3c 에서 원문 | 미리보기를 붙인다
    pub(crate) fn start_diagram_edit(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    pub(crate) fn close_diagram(&mut self) {
        let d = &mut self.diagrams;
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
            .child(
                button("dgm-edit", k, false, true)
                    .child(icon("pencil", 14.).text_color(k.text_3))
                    .child(t("diagrams.edit")),
            )
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
        // 읽기 화면의 캔버스는 프레임 없이 모서리까지. 전체 화면이면 뿌리 판이 그린다
        let canvas = match &d.read_error {
            Some(e) => error_note(t_with("diagrams.readError", &[("msg", e)])).into_any_element(),
            None if d.canvas.fullscreen => div().flex_1().into_any_element(),
            None => self.diagram_canvas(k, cx).into_any_element(),
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
