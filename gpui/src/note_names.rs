// 새 파일, 새 폴더, 이름 바꾸기 — 지금 앱 NotesView.tsx / DiagramsView.tsx 의 이름 모달과 트리 행 hover 동작.
// 만들기는 'CS/네트워크' 처럼 여러 단 경로를 받고(중간 폴더를 만든다) 놓을 자리를 고른다. 이름 바꾸기는 이름 하나만.
// 검증 문구는 vaultTree.ts 와 같다(빈 이름, `/ \ :`, 점으로 시작, 80자). 만든 파일은 바로 편집으로 연다.

use std::cell::Cell;
use std::rc::Rc;

use amber_gpui::i18n::t;
use amber_gpui::vault::{self, Node};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::{Confirm, Tag, button, primary_button};
use crate::menu::{MenuItem, select_menu};
use crate::theme::Tokens;
use crate::vault_pane::{L, Which, tree_of};
use crate::{Spike, dialog};

#[derive(Clone)]
pub enum NameKind {
    NewFile,
    NewFolder,
    Rename { path: String, is_dir: bool },
}

pub struct NameModal {
    kind: NameKind,
    /// 만들 자리('' = 루트)
    dir: String,
    input: Entity<InputState>,
    error: Option<String>,
    dir_open: bool,
    dir_anchor: Rc<Cell<Bounds<Pixels>>>,
}

impl NameModal {
    /// 닫기 전에 묻기(close_dirty)가 세는 입력칸
    pub(crate) fn input_id(&self) -> EntityId {
        self.input.entity_id()
    }
}

/// 자리 표기 — 루트는 '/', 나머지는 '/CS/깊이'
fn encode_dir(d: &str) -> String {
    format!("/{d}")
}

/// lib 의 오류는 사전 키(common.file.dupName)거나 날 메시지다 — 키면 옮기고 아니면 그대로(t 가 모르는 키는 그대로 돌려준다)
fn err_text(e: &str) -> String {
    t(e)
}

impl Spike {
    pub(crate) fn open_name_modal(
        &mut self,
        w: Which,
        kind: NameKind,
        dir: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (value, ph) = match &kind {
            NameKind::Rename { path, is_dir } => {
                let name = path.rsplit('/').next().unwrap_or(path);
                let name = if *is_dir {
                    name.to_string()
                } else {
                    w.stem(name)
                };
                (name, "")
            }
            NameKind::NewFile => (String::new(), w.l(L::PhFile)),
            NameKind::NewFolder => (String::new(), w.l(L::PhFolder)),
        };
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            if !ph.is_empty() {
                s = s.placeholder(t(ph));
            }
            s.set_value(value, window, cx);
            s
        });
        cx.subscribe_in(
            &input,
            window,
            move |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit_name_modal(w, window, cx);
                    cx.notify();
                }
            },
        )
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        let dir = dir.unwrap_or_else(|| self.ui(w).active_dir.clone());
        self.ui_mut(w).names = Some(NameModal {
            kind,
            dir,
            input,
            error: None,
            dir_open: false,
            dir_anchor: Default::default(),
        });
    }

    pub(crate) fn submit_name_modal(
        &mut self,
        w: Which,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.ui(w).names.as_ref() else {
            return;
        };
        let name = m.input.read(cx).value().trim().to_string();
        let kind = m.kind.clone();
        let dir = m.dir.clone();
        // 만들기는 여러 단 경로, 이름 바꾸기는 이름 하나
        let reason = match kind {
            NameKind::Rename { .. } => vault::invalid_name_reason(&name),
            _ => vault::invalid_path_reason(&name),
        };
        if let Some(r) = reason {
            if let Some(m) = self.ui_mut(w).names.as_mut() {
                m.error = Some(r);
            }
            return;
        }
        let r = match &kind {
            NameKind::Rename { path, is_dir } => {
                // 노트는 곁딸린 것(댓글, 개념 출처)까지 함께 옮긴다
                let renamed = match w {
                    Which::Notes => vault::rename_note_entry(&self.db, path, &name, *is_dir),
                    Which::Diagrams => tree_of(&self.db, w).rename_entry(path, &name, *is_dir),
                };
                renamed.map(|new| {
                    self.remap_tree_paths(w, path, &new, *is_dir);
                    None
                })
            }
            NameKind::NewFile => tree_of(&self.db, w).create_file(&dir, &name).map(Some),
            NameKind::NewFolder => tree_of(&self.db, w).create_folder(&dir, &name).map(|rel| {
                let ui = self.ui_mut(w);
                ui.expanded.extend(vault::ancestor_paths(&rel));
                ui.active_dir = rel;
                None
            }),
        };
        match r {
            Err(e) => {
                if let Some(m) = self.ui_mut(w).names.as_mut() {
                    m.error = Some(err_text(&e));
                }
            }
            Ok(created) => {
                self.ui_mut(w).names = None;
                self.reload_tree(w);
                // 새 파일은 바로 편집으로 연다 — 고치던 초안이 있으면 만들기만(초안 보호)
                if let Some(rel) = created {
                    let ui = self.ui_mut(w);
                    ui.expanded
                        .extend(vault::ancestor_paths(vault::parent_of(&rel)));
                    match w {
                        Which::Notes if !self.notes.edit.as_ref().is_some_and(|e| e.dirty) => {
                            self.notes.edit = None;
                            self.open_note(&rel, cx);
                            self.start_note_edit(window, cx);
                        }
                        Which::Diagrams if !self.diagram_dirty(cx) => {
                            self.open_diagram(&rel, cx);
                            self.start_diagram_edit(window, cx);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// 이름이 바뀐(옮겨진) 것 아래의 펼침, 고른 파일, 자리를 새 경로로 옮긴다
    pub(crate) fn remap_tree_paths(&mut self, w: Which, old: &str, new: &str, is_dir: bool) {
        let ui = self.ui_mut(w);
        if is_dir {
            ui.expanded = ui
                .expanded
                .iter()
                .map(|p| vault::remap_path(p, old, new))
                .collect();
            ui.active_dir = vault::remap_path(&ui.active_dir, old, new);
        }
        if let Some(sel) = ui.selected.as_mut() {
            *sel = vault::remap_path(sel, old, new);
        }
        // AI 실행은 경로가 키다 — 안 옮기면 결과가 고아가 된다
        if w == Which::Notes {
            self.notes.ai.remap(old, new, is_dir);
        }
    }

    /// 이름 모달 — 만들기는 자리(드롭다운) + 이름 + 경로 안내, 바꾸기는 이름만
    pub(crate) fn name_modal_view(
        &self,
        w: Which,
        k: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let m = self.ui(w).names.as_ref()?;
        let creating = !matches!(m.kind, NameKind::Rename { .. });
        let (title, label) = match m.kind {
            NameKind::NewFile => (w.l(L::NewFile), w.l(L::FieldFile)),
            NameKind::NewFolder => (w.l(L::NewFolder), w.l(L::FieldFolder)),
            NameKind::Rename { is_dir, .. } => (
                w.l(L::Rename),
                if is_dir {
                    w.l(L::FieldFolder)
                } else {
                    w.l(L::FieldFile)
                },
            ),
        };
        let field_label = |key: &str| {
            div()
                .mb(px(5.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.text_2)
                .child(t(key))
        };
        let anchor = m.dir_anchor.clone();
        let focused = m.input.read(cx).focus_handle(cx).is_focused(window);
        let body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .when(creating, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .child(field_label(w.l(L::Location)))
                        .child(
                            // .select.block — 폭을 채우는 트리거, 끝의 꺾쇠
                            div()
                                .id("name-dir")
                                .tag("name-dir")
                                .relative()
                                .h(px(34.))
                                .pl(px(12.))
                                .pr(px(10.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.glass_edge)
                                .bg(linear_gradient(
                                    150.,
                                    linear_color_stop(k.glass_fill.0, 0.),
                                    linear_color_stop(k.glass_fill.1, 1.),
                                ))
                                .shadow(k.glass_shadow(false))
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(k.text)
                                .cursor_pointer()
                                .child(encode_dir(&m.dir))
                                .child(icon("chevron-down", 12.).text_color(k.text_3))
                                .child(
                                    canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                                        .absolute()
                                        .inset_0(),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(m) = this.ui_mut(w).names.as_mut() {
                                        m.dir_open = !m.dir_open;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(field_label(label))
                    .child(
                        // .input — 강한 테, 포커스면 한 단 진하게
                        div()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(if focused { k.text_2 } else { k.border_strong })
                            .bg(k.surface)
                            .px(px(11.))
                            .py(px(4.))
                            .text_size(px(14.))
                            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| {
                                if e.keystroke.key == "escape" {
                                    let key = match w {
                                        Which::Notes => "names-notes",
                                        Which::Diagrams => "names-diagrams",
                                    };
                                    this.esc_close(key, window, cx, move |s, _, cx| {
                                        s.ui_mut(w).names = None;
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                }
                            }))
                            .child(Input::new(&m.input).appearance(false).px_0()),
                    )
                    .when(creating, |d| {
                        d.child(
                            div()
                                .mt(px(6.))
                                .text_size(px(12.))
                                .text_color(k.text_3)
                                .child(t(w.l(L::PathHint))),
                        )
                    }),
            )
            .when_some(m.error.clone(), |d, e| {
                // .error-note — 결과 알림(좌측 바 + 옅은 바탕)
                d.child(
                    div()
                        .px(px(12.))
                        .py(px(10.))
                        .rounded(px(6.))
                        .border_l(px(3.))
                        .border_color(k.danger)
                        .bg(k.danger_soft)
                        .text_size(px(13.))
                        .text_color(k.danger)
                        .child(e),
                )
            });
        let close = cx.listener(move |this, _, _, cx| {
            this.ui_mut(w).names = None;
            cx.notify();
        });
        let shell = dialog::shell(
            k,
            640.,
            t(title),
            body.into_any_element(),
            vec![
                button("name-cancel", k, false, false)
                    .child(t("common.cancel"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ui_mut(w).names = None;
                        cx.notify();
                    })),
                primary_button("name-submit", k, false)
                    .child(t(if creating {
                        w.l(L::Create)
                    } else {
                        w.l(L::RenameConfirm)
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.submit_name_modal(w, window, cx);
                        cx.notify();
                    })),
            ],
            close,
        );
        // 자리 고르기 판 — 지금 트리의 폴더 전부(루트 포함)
        let menu = m.dir_open.then(|| {
            let dirs = vault::flatten_dirs(self.ui(w).tree.as_deref().unwrap_or_default());
            let items = dirs
                .iter()
                .map(|d| MenuItem {
                    label: encode_dir(d),
                    checked: *d == m.dir,
                    ..Default::default()
                })
                .collect();
            let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
            select_menu(
                "name-dir-menu",
                m.dir_anchor.get(),
                items,
                k,
                move |i, _, cx| {
                    let _ = pick.update(cx, |this, cx| {
                        if let (Some(m), Some(d)) = (this.ui_mut(w).names.as_mut(), dirs.get(i)) {
                            m.dir = d.clone();
                            m.dir_open = false;
                        }
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |this, cx| {
                        if let Some(m) = this.ui_mut(w).names.as_mut() {
                            m.dir_open = false;
                        }
                        cx.notify();
                    });
                },
            )
        });
        Some(shell.children(menu))
    }

    /// 트리 행 위에 겹쳐 페이드인하는 동작(.row-actions) — 라벨을 밀지 않는다.
    /// 폴더는 '여기에 새 노트, 새 폴더'가 앞에, 모두 이름 바꾸기와 지우기
    pub(crate) fn tree_row_actions(
        &self,
        w: Which,
        n: &Node,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let act = |id: String, icon_name: &'static str, danger: bool| {
            row_act(w.tag(&id), icon_name, danger, k)
        };
        let (path, is_dir, name) = (n.path.clone(), n.is_dir, n.name.clone());
        row_actions(k)
            .when(is_dir, |d| {
                let (p1, p2) = (path.clone(), path.clone());
                d.child(
                    act(format!("row-new-note-{path}"), "file-plus", false).on_click(cx.listener(
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_name_modal(
                                w,
                                NameKind::NewFile,
                                Some(p1.clone()),
                                window,
                                cx,
                            );
                            cx.notify();
                        },
                    )),
                )
                .child(
                    act(format!("row-new-folder-{path}"), "folder-plus", false).on_click(
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_name_modal(
                                w,
                                NameKind::NewFolder,
                                Some(p2.clone()),
                                window,
                                cx,
                            );
                            cx.notify();
                        }),
                    ),
                )
            })
            .child({
                let p = path.clone();
                act(format!("row-rename-{path}"), "pencil", false).on_click(cx.listener(
                    move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_name_modal(
                            w,
                            NameKind::Rename {
                                path: p.clone(),
                                is_dir,
                            },
                            None,
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                ))
            })
            .child(
                act(format!("row-delete-{path}"), "trash", true).on_click(cx.listener(
                    move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.confirm = Some(match w {
                            Which::Notes => Confirm::Note {
                                rel: path.clone(),
                                name: name.clone(),
                                is_dir,
                            },
                            Which::Diagrams => Confirm::Diagram {
                                rel: path.clone(),
                                name: name.clone(),
                                is_dir,
                            },
                        });
                        cx.notify();
                    },
                )),
            )
    }
}

/// 트리 행 끝의 단추 줄(.row-actions) — 손이 행("tree-row" 묶음)에 올라가야 보인다. 보관함 트리와 ERD 트리가 같이 쓴다
pub(crate) fn row_actions(k: &Tokens) -> Div {
    div()
        .absolute()
        .right(px(4.))
        .top_0()
        .bottom_0()
        .flex()
        .items_center()
        .pl(px(18.))
        // 긴 라벨 위에 뜰 때 글자가 비치지 않게 hover 바탕색으로 덮는다
        .bg(linear_gradient(
            90.,
            linear_color_stop(k.surface_2.opacity(0.), 0.),
            linear_color_stop(k.surface_2, 0.3),
        ))
        .opacity(0.)
        .group_hover("tree-row", |s| s.opacity(1.))
        // 행 안 버튼을 누른 것은 끌기가 아니다
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// 행 단추 하나 — 24px, 아이콘 13px. danger 면 손이 올라갈 때 빨강
pub(crate) fn row_act(id: String, icon_name: &str, danger: bool, k: &Tokens) -> Stateful<Div> {
    let (soft, red, hover_bg, text) = (
        k.danger_soft,
        k.danger,
        linear_gradient(
            150.,
            linear_color_stop(k.glass_fill_hover.0, 0.),
            linear_color_stop(k.glass_fill_hover.1, 1.),
        ),
        k.text,
    );
    let group: SharedString = format!("{id}-g").into();
    div()
        .id(SharedString::from(id.clone()))
        .tag(id)
        .group(group.clone())
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .cursor_pointer()
        .when(danger, |d| d.hover(move |s| s.bg(soft)))
        .when(!danger, |d| d.hover(move |s| s.bg(hover_bg)))
        .child(
            icon(icon_name, 13.)
                .text_color(k.text_3)
                .group_hover(group, move |s| {
                    s.text_color(if danger { red } else { text })
                }),
        )
}
