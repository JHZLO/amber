// 노트 트리 끌어서 옮기기 — 지금 앱 lib/useTreeDnd.ts. 행을 끌면 원본은 자리에 흐리게 남고, 복제본이 커서를 세로로 따라온다.
// 커서 밑이 폴더 행이면 그 폴더, 파일 행이면 그 상위 폴더, 트리 여백이면 루트가 놓을 곳이다.
// 같은 폴더(제자리), 폴더를 자기 자신이나 하위로는 놓을 수 없다. 놓으면 곁딸린 것(댓글, 그림)과 함께 옮긴다.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use amber_gpui::vault;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::theme::Tokens;
use crate::vault_pane::Which;

pub type RowBounds = Rc<RefCell<HashMap<String, (Bounds<Pixels>, bool)>>>;
pub type ListBounds = Rc<Cell<Bounds<Pixels>>>;

/// 들고 있는 것 — GPUI 의 끌기 값
#[derive(Clone)]
pub struct TreeGrab {
    /// 어느 탭의 트리인가 — 놓을 때 그 보관함 안에서 옮긴다
    pub which: crate::vault_pane::Which,
    pub path: String,
    pub is_dir: bool,
    pub name: String,
}

pub struct TreeDrag {
    pub grab: TreeGrab,
    start_y: Pixels,
    /// 잡을 때 원본 행의 자리 — 복제본이 그 자리에서 세로로만 움직인다
    row: Bounds<Pixels>,
    pad_l: Pixels,
    dy: Pixels,
    /// 놓을 폴더('' = 루트). 놓을 수 없는 자리면 None
    pub target: Option<String>,
}

pub struct Nothing;

impl Render for Nothing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// 이동 불가면 None — 같은 폴더(제자리), 폴더를 자기 자신이나 하위로
fn valid_target(from: &TreeGrab, dir: Option<String>) -> Option<String> {
    let dir = dir?;
    if dir == vault::parent_of(&from.path) {
        return None;
    }
    if from.is_dir && (dir == from.path || dir.starts_with(&format!("{}/", from.path))) {
        return None;
    }
    Some(dir)
}

impl Spike {
    pub(crate) fn begin_tree_drag(&mut self, grab: TreeGrab, depth_pad: Pixels) {
        let ui = self.ui_mut(grab.which);
        let row = ui
            .tree_rows
            .borrow()
            .get(&grab.path)
            .map(|(b, _)| *b)
            .unwrap_or_default();
        let start_y = ui.tree_press.take().map_or(row.center().y, |p| p.y);
        ui.tree_drag = Some(TreeDrag {
            grab,
            start_y,
            row,
            pad_l: depth_pad,
            dy: px(0.),
            target: None,
        });
    }

    /// 커서 밑 — 행이면 폴더/상위 폴더, 트리 여백이면 루트, 트리 밖이면 None
    pub(crate) fn tree_drag_move(&mut self, w: Which, pos: Point<Pixels>) -> bool {
        let ui = self.ui_mut(w);
        let under = {
            let rows = ui.tree_rows.borrow();
            match rows.iter().find(|(_, (b, _))| b.contains(&pos)) {
                Some((path, (_, is_dir))) => Some(if *is_dir {
                    path.clone()
                } else {
                    vault::parent_of(path).to_string()
                }),
                None => ui.tree_area.get().contains(&pos).then(String::new),
            }
        };
        let Some(d) = ui.tree_drag.as_mut() else {
            return false;
        };
        d.dy = pos.y - d.start_y;
        d.target = valid_target(&d.grab, under);
        true
    }

    /// 놓았다 — 놓을 수 있는 자리면 옮기고, 옮겨 간 자리를 펼쳐 보여 준다
    pub(crate) fn tree_drop(&mut self, w: Which) {
        let Some(d) = self.ui_mut(w).tree_drag.take() else {
            return;
        };
        let Some(target) = d.target else {
            return;
        };
        let from = d.grab.path;
        // 노트는 곁딸린 것(댓글, 그림)과 함께 옮긴다
        let moved = match w {
            Which::Notes => vault::move_note_entry(&self.db, &from, &target),
            Which::Diagrams => crate::vault_pane::tree_of(&self.db, w).move_entry(&from, &target),
        };
        match moved {
            Ok(new) => {
                self.remap_tree_paths(w, &from, &new, d.grab.is_dir);
                self.ui_mut(w)
                    .expanded
                    .extend(vault::ancestor_paths(vault::parent_of(&new)));
            }
            Err(e) => match w {
                Which::Notes => self.notes.read_error = Some(amber_gpui::i18n::t(&e)),
                Which::Diagrams => self.diagrams.read_error = Some(amber_gpui::i18n::t(&e)),
            },
        }
        self.reload_tree(w);
    }

    /// 끌기가 남아 있는 탭(창 밖에서 놓았으면 render 가 마저 놓는다)
    pub(crate) fn dragging_tree(&self) -> Option<Which> {
        [Which::Notes, Which::Diagrams]
            .into_iter()
            .find(|w| self.ui(*w).tree_drag.is_some())
    }

    /// 떠 있는 복제본(.tree-drag-overlay) — 창 좌표로 뿌리 판에 그린다
    pub(crate) fn tree_drag_ghost(&self, k: &Tokens) -> Option<Div> {
        let d = self.ui(self.dragging_tree()?).tree_drag.as_ref()?;
        Some(
            div()
                .absolute()
                .left(d.row.origin.x)
                .top(d.row.origin.y + d.dy)
                .w(d.row.size.width)
                .h(d.row.size.height)
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(d.pad_l)
                .pr(px(10.))
                .rounded(px(6.))
                .bg(k.surface)
                .shadow(vec![
                    BoxShadow {
                        color: hsla(0., 0., 0., 0.18),
                        offset: point(px(0.), px(8.)),
                        blur_radius: px(22.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                    BoxShadow {
                        color: hsla(0., 0., 0., 0.12),
                        offset: point(px(0.), px(2.)),
                        blur_radius: px(6.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                ])
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.text)
                .child(
                    icon(if d.grab.is_dir { "folder" } else { "file" }, 14.)
                        .flex_shrink_0()
                        .text_color(k.text_2),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(d.grab.name.clone()),
                ),
        )
    }
}
