// 노트 탭과 다이어그램 탭이 함께 쓰는 폴더 트리 — 지금 앱도 두 탭이 같은 트리 UX(같은 CSS, useTreeDnd, TreeFind,
// RootPicker)를 쓴다. 다른 것은 보관함(파일 종류, 작업 폴더 키), 문구, 파일을 열고 옮기는 방법뿐이라 Which 로 가른다.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use amber_gpui::i18n::t;
use amber_gpui::vault::{self, Node, Tree, TreeConfig};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, primary_button};
use crate::pane::PaneGrab;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Which {
    Notes,
    Diagrams,
}

/// 두 탭의 트리 화면 상태
#[derive(Default)]
pub struct TreeUi {
    /// None = 아직 안 읽음
    pub tree: Option<Vec<Node>>,
    pub error: Option<String>,
    pub expanded: HashSet<String>,
    /// 새 파일/폴더가 들어갈 폴더 — 마지막으로 누른 폴더(또는 연 파일의 폴더)
    pub active_dir: String,
    pub selected: Option<String>,
    /// 새 파일, 새 폴더, 이름 바꾸기 모달
    pub names: Option<crate::note_names::NameModal>,
    /// 트리 끌기: 끄는 중인 것, 행마다(경로 → 자리, 폴더인가)와 트리 판의 자리, 누른 자리
    pub tree_drag: Option<crate::tree_drag::TreeDrag>,
    pub tree_rows: crate::tree_drag::RowBounds,
    pub tree_area: crate::tree_drag::ListBounds,
    pub tree_press: Option<Point<Pixels>>,
    /// 트리에서 찾기(⌘F)
    pub find: Option<crate::note_find::TreeFindState>,
    /// 작업 폴더 고르기 판이 열렸는가, 트리거 자리
    pub root_menu: bool,
    pub root_anchor: Rc<Cell<Bounds<Pixels>>>,
    pub width: f32,
}

/// 탭마다 다른 문구
pub enum L {
    NewFile,
    NewFolder,
    Rename,
    FieldFile,
    FieldFolder,
    Location,
    PathHint,
    Create,
    RenameConfirm,
    PhFile,
    PhFolder,
}

impl Which {
    pub fn cfg(self) -> &'static TreeConfig {
        match self {
            Which::Notes => &vault::NOTES,
            Which::Diagrams => &vault::DIAGRAMS,
        }
    }

    /// 시험대가 찾는 이름 — 노트는 원래 이름 그대로, 다이어그램은 'd' 를 붙인다
    pub fn tag(self, base: &str) -> String {
        match self {
            Which::Notes => base.to_string(),
            Which::Diagrams => format!("d{base}"),
        }
    }

    pub fn root_key(self) -> &'static str {
        self.cfg().root_pref.unwrap_or_default()
    }

    pub fn recent_key(self) -> &'static str {
        match self {
            Which::Notes => "amber.recent-roots.notes",
            Which::Diagrams => "amber.recent-roots.diagrams",
        }
    }

    /// 판 폭(usePaneResize 의 기본값, 탭마다 키가 다르다)
    pub fn pane(self) -> PaneGrab {
        PaneGrab {
            key: match self {
                Which::Notes => "amber.notes.list-width",
                Which::Diagrams => "amber.diagrams.list-width",
            },
            min: 240.,
            max: 640.,
        }
    }

    /// 이름 칸에 보일 이름 — 파일이면 확장자를 뗀다
    pub fn stem(self, name: &str) -> String {
        let lower = name.to_lowercase();
        self.cfg()
            .exts
            .iter()
            .find(|e| lower.ends_with(*e))
            .map_or(name, |e| &name[..name.len() - e.len()])
            .to_string()
    }

    pub fn l(self, l: L) -> &'static str {
        match (self, l) {
            (Which::Notes, L::NewFile) => "notes.newNote",
            (Which::Notes, L::NewFolder) => "notes.newFolder",
            (Which::Notes, L::Rename) => "notes.rename",
            (Which::Notes, L::FieldFile) => "notes.field.noteName",
            (Which::Notes, L::FieldFolder) => "notes.field.folderName",
            (Which::Notes, L::Location) => "notes.field.location",
            (Which::Notes, L::PathHint) => "notes.hint.path",
            (Which::Notes, L::Create) => "notes.create",
            (Which::Notes, L::RenameConfirm) => "notes.renameConfirm",
            (Which::Notes, L::PhFile) => "notes.ph.noteName",
            (Which::Notes, L::PhFolder) => "notes.ph.folderName",
            (Which::Diagrams, L::NewFile) => "diagrams.newFile",
            (Which::Diagrams, L::NewFolder) => "diagrams.newFolder",
            (Which::Diagrams, L::Rename) => "diagrams.rename",
            (Which::Diagrams, L::FieldFile) => "diagrams.modal.fileName",
            (Which::Diagrams, L::FieldFolder) => "diagrams.modal.folderName",
            (Which::Diagrams, L::Location) => "diagrams.modal.location",
            (Which::Diagrams, L::PathHint) => "diagrams.modal.pathHint",
            (Which::Diagrams, L::Create) => "diagrams.modal.create",
            (Which::Diagrams, L::RenameConfirm) => "diagrams.modal.renameConfirm",
            (Which::Diagrams, L::PhFile) => "diagrams.modal.filePh",
            (Which::Diagrams, L::PhFolder) => "diagrams.modal.folderPh",
        }
    }
}

/// 작업 폴더에 따른 지금 보관함
pub fn tree_of(db: &amber_gpui::db::Db, w: Which) -> Tree<'_> {
    Tree::new(db, w.cfg())
}

impl Spike {
    pub(crate) fn ui(&self, w: Which) -> &TreeUi {
        match w {
            Which::Notes => &self.notes.ui,
            Which::Diagrams => &self.diagrams.ui,
        }
    }

    pub(crate) fn ui_mut(&mut self, w: Which) -> &mut TreeUi {
        match w {
            Which::Notes => &mut self.notes.ui,
            Which::Diagrams => &mut self.diagrams.ui,
        }
    }

    fn tree_focus_of(&self, w: Which) -> &FocusHandle {
        match w {
            Which::Notes => &self.tree_focus,
            Which::Diagrams => &self.dtree_focus,
        }
    }

    /// 탭에 들어올 때마다 — Finder, git, 다른 편집기가 바꾼 것을 반영한다
    pub(crate) fn reload_tree(&mut self, w: Which) {
        let r = tree_of(&self.db, w).list_tree();
        let ui = self.ui_mut(w);
        match r {
            Ok(tree) => {
                ui.tree = Some(tree);
                ui.error = None;
            }
            Err(e) => ui.error = Some(e),
        }
    }

    /// 보관함 이름 — 기본 보관함은 언어를 따르고, 고른 작업 폴더는 폴더 이름 그대로
    pub(crate) fn root_name(&self, w: Which) -> String {
        match amber_gpui::prefs::get(&self.db, w.root_key()).filter(|r| !r.is_empty()) {
            Some(r) => r
                .split('/')
                .filter(|s| !s.is_empty())
                .next_back()
                .unwrap_or(&r)
                .to_string(),
            None => t("settings.root.default"),
        }
    }

    pub(crate) fn toggle_dir(&mut self, w: Which, path: &str) {
        let ui = self.ui_mut(w);
        if !ui.expanded.remove(path) {
            ui.expanded.insert(path.to_string());
        }
        ui.active_dir = path.to_string();
    }

    /// 트리에서 파일을 눌렀다 — 저장 안 한 편집이 있으면 각 탭이 먼저 묻는다
    pub(crate) fn request_open(&mut self, w: Which, rel: &str, cx: &mut Context<Self>) {
        match w {
            Which::Notes => self.request_open_note(rel, cx),
            Which::Diagrams => self.request_open_diagram(rel, cx),
        }
    }

    /// 왼쪽 트리 카드 — 머리, 찾기 줄, 트리, 오른쪽 끝의 폭 손잡이
    pub(crate) fn tree_pane(
        &self,
        w: Which,
        width: f32,
        body_w: f32,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        // 찾기 줄의 몇 개는 트리를 거른 결과로 센다
        let ui = self.ui(w);
        let count = match (ui.find.as_ref(), ui.tree.as_ref()) {
            (Some(f), Some(tree)) => amber_gpui::tree_find::filter_tree(tree, &f.query)
                .matches
                .len(),
            _ => 0,
        };
        let focus = self.tree_focus_of(w).clone();
        div()
            .relative()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .pt(px(8.))
            .pb(px(8.))
            .pl(px(8.))
            .pr(px(4.))
            .child(
                // 트리 판 — 누르면 포커스를 가져가 그 뒤의 ⌘F 가 트리 찾기로 온다(본문의 ⌘F 와 안 겹치게)
                div()
                    .track_focus(&focus)
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_1()
                    .border_color(k.glass_edge)
                    .rounded(px(12.))
                    .bg(crate::note_ai_view::glass(k.glass_fill))
                    .shadow(k.glass_shadow(false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            let f = this.tree_focus_of(w).clone();
                            if !f.contains_focused(window, cx) {
                                f.focus(window, cx);
                            }
                        }),
                    )
                    // 바깥을 누르면 트리에서 포커스를 놓는다 — 그래야 본문에서의 ⌘F 가 그 탭의 찾기로 간다
                    .on_mouse_down_out(cx.listener(move |this, _, window, cx| {
                        if this.tree_focus_of(w).is_focused(window) {
                            this.focus.focus(window, cx);
                        }
                    }))
                    .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| {
                        let m = &e.keystroke.modifiers;
                        if m.platform && !m.alt && e.keystroke.key == "f" {
                            this.start_tree_find(w, window, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(self.tree_head(w, k, cx))
                    .children(self.tree_find_bar(w, count, k, cx))
                    .child(self.tree_list(w, k, cx)),
            )
            .child(self.pane_resizer(w.pane(), width, body_w, k, cx))
    }

    /// .notes-tree-head — 보관함 고르기 | 새 파일, 새 폴더, 찾기, 다시 읽기
    fn tree_head(&self, w: Which, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let icon_btn = |id: String, name: &str, size: f32| {
            let (bg, text) = (k.surface_2, k.text);
            let id = SharedString::from(id);
            div()
                .id(id.clone())
                .tag(id.clone())
                .size(px(24.))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .group(id.clone())
                .hover(move |s| s.bg(bg))
                .child(
                    icon(name, size)
                        .text_color(k.text_2)
                        .group_hover(id, move |s| s.text_color(text)),
                )
        };
        let anchor = self.ui(w).root_anchor.clone();
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.))
            .pt(px(9.))
            .pb(px(9.))
            .pr(px(10.))
            .pl(px(16.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                // .root-picker — 누르면 기본 보관함, 최근 폴더, 폴더 열기
                div()
                    .id(SharedString::from(w.tag("root-picker")))
                    .tag(w.tag("root-picker"))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .max_w(px(170.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text)
                    .cursor_pointer()
                    .hover(|s| s.bg(k.surface_2))
                    .child(icon("folder-open", 14.).text_color(k.text_3))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.root_name(w)),
                    )
                    .child(icon("chevron-down", 10.).text_color(k.text_3))
                    .child(
                        canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let ui = this.ui_mut(w);
                        ui.root_menu = !ui.root_menu;
                        cx.notify();
                    })),
            )
            .children(self.root_menu(w, k, cx))
            .child(div().flex_1())
            .child(
                icon_btn(w.tag("notes-new-note"), "file-plus", 15.).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.open_name_modal(
                            w,
                            crate::note_names::NameKind::NewFile,
                            None,
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_btn(w.tag("notes-new-folder"), "folder-plus", 15.).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.open_name_modal(
                            w,
                            crate::note_names::NameKind::NewFolder,
                            None,
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_btn(w.tag("notes-find"), "search", 14.).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.start_tree_find(w, window, cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_btn(w.tag("notes-refresh"), "refresh", 14.).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.reload_tree(w);
                        cx.notify();
                    },
                )),
            )
    }

    fn tree_list(&self, w: Which, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let ui = self.ui(w);
        // 이번 그림의 행 자리만 남긴다 — 접혀 사라진 행이 놓을 곳으로 잡히지 않게
        ui.tree_rows.borrow_mut().clear();
        let area = ui.tree_area.clone();
        let drop_root = ui
            .tree_drag
            .as_ref()
            .is_some_and(|d| d.target.as_deref() == Some(""));
        let list = div()
            .id(SharedString::from(w.tag("notes-tree")))
            .tag(w.tag("notes-tree"))
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(6.))
            .px(px(8.))
            .pb(px(24.))
            // 루트(트리 여백)로 놓을 때 — 판 안쪽 2px 링
            .when(drop_root, |d| {
                d.rounded(px(8.)).shadow(vec![Tokens::ring(k.text_3, 2.)])
            })
            .on_drag_move::<crate::tree_drag::TreeGrab>(cx.listener(
                move |this, e: &DragMoveEvent<crate::tree_drag::TreeGrab>, _, cx| {
                    if this.tree_drag_move(w, e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .child(
                canvas(move |b, _, _| area.set(b), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            );
        if let Some(e) = &ui.error {
            return list.child(
                div()
                    .m(px(4.))
                    .text_size(px(12.5))
                    .text_color(k.danger)
                    .child(e.clone()),
            );
        }
        match &ui.tree {
            None => list,
            Some(tree) if tree.is_empty() => list.child(self.tree_empty(w, k, cx)),
            Some(tree) => {
                // 찾는 중이면 맞은 것과 그 조상만, 조상은 임시로 펼친다
                let found = ui
                    .find
                    .as_ref()
                    .map(|f| amber_gpui::tree_find::filter_tree(tree, &f.query));
                let hit = ui.find.as_ref().and_then(|f| {
                    let cur = found.as_ref()?.matches.get(f.idx)?.0.clone();
                    Some((f.query.clone(), cur))
                });
                let nodes = found
                    .as_ref()
                    .map_or(tree.as_slice(), |f| f.nodes.as_slice());
                let auto: Vec<String> =
                    found.as_ref().map(|f| f.expand.clone()).unwrap_or_default();
                if found.is_some() && nodes.is_empty() {
                    return list.child(
                        div()
                            .px(px(10.))
                            .py(px(14.))
                            .text_size(px(12.5))
                            .text_color(k.text_3)
                            .child(t("common.find.treeEmpty")),
                    );
                }
                let mut rows = Vec::new();
                self.tree_rows(w, nodes, 0, &auto, hit.as_ref(), &mut rows, k, cx);
                list.children(rows)
            }
        }
    }

    /// 빈 보관함 — 노트는 안내 두 줄, 다이어그램은 안내 + 만들기 단추(.tree-empty)
    fn tree_empty(&self, w: Which, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let (lead, sub) = match w {
            Which::Notes => ("notes.tree.empty.lead", "notes.tree.empty.sub"),
            Which::Diagrams => ("diagrams.empty.tree1", "diagrams.empty.tree2"),
        };
        div()
            .py(px(44.))
            .px(px(12.))
            .flex()
            .flex_col()
            .items_center()
            .text_size(px(13.))
            .line_height(relative(1.7))
            .text_color(k.text_3)
            .child(t(lead))
            .child(t(sub))
            .when(w == Which::Diagrams, |d| {
                d.child(
                    primary_button("dtree-empty-create", k, false)
                        .mt(px(12.))
                        .child(icon("file-plus", 14.).text_color(k.text))
                        .child(t("diagrams.empty.create"))
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
            })
    }

    /// 펼친 폴더만 자식을 그린다(1,000개 규모에서 접힌 것까지 그리면 수천 개가 살아 있다)
    #[allow(clippy::too_many_arguments)]
    fn tree_rows(
        &self,
        w: Which,
        nodes: &[Node],
        depth: usize,
        auto: &[String],
        hit: Option<&(String, String)>,
        out: &mut Vec<AnyElement>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) {
        for n in nodes {
            let open =
                n.is_dir && (self.ui(w).expanded.contains(&n.path) || auto.contains(&n.path));
            out.push(
                self.tree_row(w, n, depth, open, hit, k, cx)
                    .into_any_element(),
            );
            if open && let Some(kids) = &n.children {
                self.tree_rows(w, kids, depth + 1, auto, hit, out, k, cx);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_row(
        &self,
        w: Which,
        n: &Node,
        depth: usize,
        open: bool,
        hit: Option<&(String, String)>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let ui = self.ui(w);
        // 찾기의 지금 결과 — 행 배경은 그대로 두고(연 것과 헷갈리지 않게) 가는 테와 노란 글자만
        let find_current = hit.is_some_and(|(_, cur)| *cur == n.path);
        let label = crate::note_find::label(&n.name, hit.map(|(q, _)| q.as_str()), find_current, k);
        let selected = !n.is_dir && ui.selected.as_deref() == Some(n.path.as_str());
        let active = n.is_dir && ui.active_dir == n.path;
        let fill = |pair: (Hsla, Hsla)| {
            linear_gradient(
                150.,
                linear_color_stop(pair.0, 0.),
                linear_color_stop(pair.1, 1.),
            )
        };
        // 왼쪽 바(§3 '너 여기 있다') — inset 2px 0 0
        let bar = |color| BoxShadow {
            color,
            offset: point(px(2.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        };
        let path = n.path.clone();
        let is_dir = n.is_dir;
        let tag = w.tag(&format!("tree-{}", n.path));
        let actions = self.tree_row_actions(w, n, k, cx);
        // AI 실행 점 — 노트만(쓰는 중 / 준비됨 / 실패, 정확한 말은 배너가 갖는다)
        let ai_dot = match w {
            Which::Notes => self.ai_tree_dot(&n.path, k, cx.reduce_motion()),
            Which::Diagrams => None,
        };
        let drag = ui.tree_drag.as_ref();
        let lifted = drag.is_some_and(|d| d.grab.path == n.path);
        let target = is_dir && drag.is_some_and(|d| d.target.as_deref() == Some(n.path.as_str()));
        let rows = ui.tree_rows.clone();
        let (rec_path, pad) = (n.path.clone(), px(8. + depth as f32 * 14.));
        let this = cx.entity().downgrade();
        let grab = crate::tree_drag::TreeGrab {
            which: w,
            path: n.path.clone(),
            is_dir,
            name: n.name.clone(),
        };
        div()
            .id(SharedString::from(tag.clone()))
            .tag(tag)
            .group("tree-row")
            .relative()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(8. + depth as f32 * 14.))
            .pr(px(6.))
            .py(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .text_size(px(13.5))
            .text_color(if is_dir || selected { k.text } else { k.text_2 })
            .when(is_dir || selected, |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(!selected && !active && drag.is_none(), |d| {
                let bg = fill(k.glass_fill);
                d.hover(move |s| s.bg(bg))
            })
            // 들린 원본은 자리에 흐리게 남는다
            .when(lifted, |d| d.opacity(0.4))
            .when(active, |d| {
                let mut shadow = k.glass_shadow(false);
                shadow.push(bar(k.border_strong));
                d.bg(fill(k.glass_fill))
                    .border_color(k.glass_edge)
                    .shadow(shadow)
            })
            .when(selected, |d| {
                let mut shadow = k.glass_shadow(true)[..1].to_vec();
                shadow.extend(k.glass_shadow(false)[1..].iter().cloned());
                shadow.push(bar(k.text));
                d.bg(fill(k.glass_fill_accent))
                    .border_color(k.glass_edge_accent)
                    .shadow(shadow)
            })
            // 놓을 폴더 — 채움 + 굵은 링(받아들이는 표시)
            .when(target, |d| {
                d.bg(k.surface_3)
                    .shadow(vec![Tokens::ring(k.text_3, 2.)])
                    .text_color(k.text)
            })
            .child(div().w(px(13.)).flex_shrink_0().when(is_dir, |d| {
                d.child(
                    icon(
                        if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        13.,
                    )
                    .text_color(k.text_3),
                )
            }))
            .child(
                icon(
                    if !is_dir {
                        "file"
                    } else if open || target {
                        "folder-open"
                    } else {
                        "folder"
                    },
                    14.,
                )
                .flex_shrink_0()
                .text_color(if is_dir || selected {
                    k.text_2
                } else {
                    k.text_3
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .when(find_current && !selected, |d| {
                d.shadow(vec![Tokens::ring(k.border_strong, 1.)])
            })
            .children(ai_dot)
            .child(actions)
            .child(
                canvas(
                    move |b, _, _| {
                        rows.borrow_mut().insert(rec_path, (b, is_dir));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                    this.ui_mut(w).tree_press = Some(e.position);
                }),
            )
            .on_drag(
                grab,
                move |g: &crate::tree_drag::TreeGrab, _, _, cx: &mut App| {
                    let g = g.clone();
                    let _ = this.update(cx, |s, cx| {
                        s.begin_tree_drag(g, pad);
                        cx.notify();
                    });
                    cx.new(|_| crate::tree_drag::Nothing)
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if is_dir {
                    this.toggle_dir(w, &path);
                } else if this.ui(w).selected.as_deref() != Some(path.as_str()) {
                    this.request_open(w, &path, cx);
                }
                cx.notify();
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::Which;

    #[test]
    fn stems_and_tags() {
        assert_eq!(Which::Notes.stem("a.md"), "a");
        assert_eq!(Which::Diagrams.stem("erd.MMD"), "erd");
        assert_eq!(Which::Diagrams.stem("x.mermaid"), "x");
        assert_eq!(Which::Diagrams.stem("plain"), "plain");
        assert_eq!(Which::Notes.tag("tree-a"), "tree-a");
        assert_eq!(Which::Diagrams.tag("tree-a"), "dtree-a");
    }
}
