// 할 일 탭의 하루 카드 — 지금 앱 TodoView.tsx 의 .todo-card(머리 띠, 밀린 할 일, 빠른 추가, 목록, 완료 수)를 옮긴 것.
// 묶음 머리글(자식이 있는 최상위 줄)은 체크박스 없이 작은 대문자 + 가는 선이다 — 컨테이너는 체크하는 물건이 아니다.
// 다른 날로 넘긴 줄(고스트)은 흐리게, 도착 날짜를 '› 11/6' 로 단다. 체크는 된다(거기서 끝내면 도착 날짜에서도 끝난다).

use amber_gpui::dates;
use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::tree::{self, Node};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::calendar::Unit;
use crate::menu::{MenuItem, select_menu};
use crate::row_drag::DragRow;
use crate::theme::Tokens;

/// 화면 조작 시험대(ui_tests)가 이 이름으로 요소를 찾는다 — 시험 빌드가 아니면 아무 일도 안 한다
pub trait Tag {
    fn tag(self, name: impl Into<SharedString>) -> Self;
}

impl Tag for Stateful<Div> {
    fn tag(self, name: impl Into<SharedString>) -> Self {
        let name: SharedString = name.into();
        self.debug_selector(move || name.to_string())
    }
}

impl Tag for Div {
    fn tag(self, name: impl Into<SharedString>) -> Self {
        let name: SharedString = name.into();
        self.debug_selector(move || name.to_string())
    }
}

/// 지우기 전에 묻는 것 — 라이브 항목(하위 수를 말한다) 또는 이 날짜에 남은 이월 기록 한 줄
pub(crate) enum Confirm {
    Delete {
        id: i64,
        parent: Option<i64>,
        name: String,
        count: usize,
    },
    Record {
        id: i64,
        name: String,
    },
    /// 서랍(언젠가)의 카드 — 날짜가 없어 조상 재계산이 필요 없다
    Parked {
        id: i64,
        name: String,
    },
    /// 노트 탭의 노트 또는 폴더 — 휴지통으로 간다(곁딸린 댓글, 그림도 함께)
    Note {
        rel: String,
        name: String,
        is_dir: bool,
    },
    /// 다이어그램 탭의 파일 또는 폴더 — 휴지통으로 간다
    Diagram {
        rel: String,
        name: String,
        is_dir: bool,
    },
    /// 노트의 질문 스레드 — 사이드카에서 지운다(이어 물은 것까지)
    Comment {
        id: String,
        name: String,
    },
    /// 타임테이블 블록
    Block {
        id: i64,
        name: String,
    },
}

fn vac_of(s: &Spike) -> Option<String> {
    s.cal.vacations.get(&s.date).cloned()
}

/// 중첩 단계마다 더 들어가는 폭
const INDENT: f32 = 24.;

/// 할 일 입력칸 중 포커스를 쥔 것 — 지금 앱 .input:focus(테두리), .todo-card .todo-quick:focus-within(아래 선, 아이콘)
#[derive(Default, Clone, Copy, PartialEq)]
pub(crate) enum TodoFocus {
    #[default]
    None,
    Quick,
    Child,
    Edit,
}

/// 입력칸 상자(.input .todo-edit, 한 줄 30px) — 판 위에서 어디에 적는지 보이게 테두리와 면을 둔다.
/// 안의 칸은 small(24px, 여러 줄은 안쪽 여백 2/8 을 스스로 갖는다)
fn edit_box(focused: bool, k: &Tokens) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .rounded(px(6.))
        .border_1()
        .border_color(if focused { k.text_2 } else { k.border_strong })
        .bg(k.surface)
        .py(px(2.))
        .text_size(px(13.5))
}
const OVERDUE_LIMIT: usize = 20;

fn glass(k: &Tokens) -> Background {
    linear_gradient(
        150.,
        linear_color_stop(k.glass_fill.0, 0.),
        linear_color_stop(k.glass_fill.1, 1.),
    )
}

/// .btn.btn-sm — 유리 버튼. ghost 면 평소엔 테두리, 바탕이 없다(카드 머리 띠의 버튼)
pub fn button(
    id: impl Into<SharedString>,
    k: &Tokens,
    ghost: bool,
    disabled: bool,
) -> Stateful<Div> {
    let (fill, edge, shadow) = (glass(k), k.glass_edge, k.glass_shadow(false));
    let id: SharedString = id.into();
    div()
        .id(id.clone())
        .tag(id)
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .when(ghost, |d| {
            d.border_color(transparent_black()).text_color(k.text_2)
        })
        .when(!ghost, |d| {
            d.border_color(edge)
                .bg(fill.clone())
                .shadow(shadow.clone())
                .text_color(k.text)
        })
        .when(disabled, |d| d.text_color(k.text_3).opacity(0.6))
        .when(!disabled, |d| {
            let hover = k.glass_shadow(true);
            let fill_hover = linear_gradient(
                150.,
                linear_color_stop(k.glass_fill_hover.0, 0.),
                linear_color_stop(k.glass_fill_hover.1, 1.),
            );
            let text = k.text;
            d.cursor_pointer()
                .hover(move |s| s.bg(fill_hover).shadow(hover).text_color(text))
        })
}

/// .btn-primary — 주 동작(저장 등). 채움 대신 유리를 두껍게 한다(강조 유리 + 강조 테), 글자는 굵게
pub fn primary_button(id: impl Into<SharedString>, k: &Tokens, disabled: bool) -> Stateful<Div> {
    let id: SharedString = id.into();
    let fill = |pair: (Hsla, Hsla)| {
        linear_gradient(
            150.,
            linear_color_stop(pair.0, 0.),
            linear_color_stop(pair.1, 1.),
        )
    };
    let hover = fill(k.glass_fill_accent_hover);
    div()
        .id(id.clone())
        .tag(id)
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(k.glass_edge_accent)
        .bg(fill(k.glass_fill_accent))
        .shadow(k.glass_shadow_hover())
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap()
        .text_color(k.text)
        .when(disabled, |d| d.opacity(0.6))
        .when(!disabled, |d| {
            d.cursor_pointer().hover(move |s| s.bg(hover))
        })
}

/// .btn-danger — 지우기 확인의 '삭제'. 평소엔 빨간 글자, 손이 닿으면 빨간 테와 옅은 바탕
pub fn danger_button(id: impl Into<SharedString>, k: &Tokens) -> Stateful<Div> {
    let id: SharedString = id.into();
    let (danger, soft) = (k.danger, k.danger_soft);
    div()
        .id(id.clone())
        .tag(id)
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(k.glass_edge)
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(danger)
        .cursor_pointer()
        .hover(move |s| s.bg(soft).border_color(danger))
}

/// 둥근 체크 — 끝낸 일은 강조 유리 필에 체크 표시(.checkbox). ui.tsx 의 Checkbox 도 같은 모양
pub(crate) fn checkbox(
    id: impl Into<SharedString>,
    k: &Tokens,
    checked: bool,
    disabled: bool,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let accent = linear_gradient(
        150.,
        linear_color_stop(k.glass_fill_accent.0, 0.),
        linear_color_stop(k.glass_fill_accent.1, 1.),
    );
    div()
        .id(id.clone())
        .tag(id)
        .size(px(20.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border(px(1.5))
        .border_color(k.border_strong)
        .when(checked, |d| d.bg(accent).border_color(k.glass_edge_accent))
        .when(checked, |d| d.child(icon("check", 13.).text_color(k.text)))
        .when(disabled, |d| d.opacity(0.4))
        .when(!disabled, |d| {
            let hover = k.text_2;
            d.cursor_pointer().hover(move |s| s.border_color(hover))
        })
}

impl Spike {
    pub(crate) fn day_card(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let is_today = self.date == today;
        let week = self.cal.unit == Unit::Week;

        let nav = |id: &'static str, name: &str, delta: i64, cx: &mut Context<Self>| {
            div()
                .id(id)
                .tag(id)
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(k.text_mix(0.06)))
                .child(icon(name, 16.).text_color(k.text_2))
                .on_click(cx.listener(move |this, _, _, cx| {
                    // 주 단위면 한 주씩
                    let step = if this.cal.unit == Unit::Week { 7 } else { 1 };
                    let d = dates::shift_day(&this.date, delta * step);
                    this.go_date(d);
                    cx.notify();
                }))
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .min_h(px(48.))
            .mt(px(-4.))
            .mx(px(-14.))
            .pt(px(8.))
            .pr(px(10.))
            .pb(px(4.))
            .pl(px(18.))
            .child(
                div()
                    .mr(px(6.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text)
                    .child(if week {
                        let ws = dates::week_start_of(&self.date);
                        let range = format!(
                            "{} – {}",
                            dates::format_day_short(&ws),
                            dates::format_day_short(&dates::shift_day(&ws, 6))
                        );
                        t_with("todos.week.title", &[("range", &range)])
                    } else {
                        dates::format_day_long(&self.date)
                    }),
            )
            // 휴가는 하루에 거는 표식이라 주 단위에서는 숨긴다
            .when(!week, |d| {
                d.child({
                    // 휴가 — 꺼짐은 표준 트리거, 켜짐은 호박 글자. 누르면 곧장 쓰이지 않고 종류 목록이 열린다
                    let vac = self.cal.vacations.get(&self.date).cloned();
                    let anchor = self.vac_anchor.clone();
                    button("vac", k, true, false)
                        .when_some(vac.clone(), |d, kind| {
                            d.text_color(k.vacation)
                                .child(t(&format!("todos.vac.kind.{kind}")))
                        })
                        .when(vac.is_none(), |d| d.child(t("todos.vac.set")))
                        .child(icon("chevron-down", 13.).text_color(if vac.is_some() {
                            k.vacation
                        } else {
                            k.text_3
                        }))
                        .child(
                            canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                                .absolute()
                                .inset_0(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.vac_open = !this.vac_open;
                            cx.notify();
                        }))
                })
                .when(self.vac_open, |d| {
                    d.child(self.vac_menu(vac_of(self), k, cx))
                })
            })
            .child(div().flex_1())
            .child(
                button("today", k, true, is_today)
                    .child(t("todos.today"))
                    .when(!is_today, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            this.go_date(dates::today_str(chrono::Local::now().timestamp_millis()));
                            cx.notify();
                        }))
                    }),
            )
            .child(nav("prev-day", "chevron-left", -1, cx))
            .child(nav("next-day", "chevron-right", 1, cx))
            // 서랍은 일 단위에서만 — 주 목록은 '이번 주'라는 기한이 있어 '날짜 없음'과 섞이지 않는다
            .when(!week, |d| {
                d.child(
                    button("drawer", k, !self.parked_open, false)
                        .ml(px(8.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_parked_open(!this.parked_open);
                            cx.notify();
                        }))
                        .child(icon("panel", 14.).text_color(k.text_2))
                        .child(t("todos.drawer.title"))
                        .when(self.parked_n > 0, |d| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(k.text_3)
                                    .child(self.parked_n.to_string()),
                            )
                        }),
                )
            });

        let quick_focus = self.todo_focus == TodoFocus::Quick;
        let quick = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .mx(px(-14.))
            .pl(px(16.))
            .pr(px(18.))
            .py(px(12.))
            .border_b_1()
            .border_color(if quick_focus {
                k.glass_edge_hover
            } else {
                k.glass_edge
            })
            .child(icon("plus", 15.).text_color(if quick_focus { k.text } else { k.text_3 }))
            .child(
                div()
                    .flex_1()
                    .text_size(px(14.))
                    .child(Input::new(&self.input).appearance(false)),
            );

        let progress = tree::day_progress(self.rows());
        let mut card = div()
            .flex()
            .flex_col()
            .pt(px(4.))
            .px(px(14.))
            .pb(px(14.))
            .border_1()
            .border_color(k.glass_edge)
            .rounded(px(12.))
            .bg(glass(k))
            .shadow(k.glass_shadow(false))
            .child(head.flex_shrink_0());
        if !week && self.date <= today && !self.overdue.is_empty() {
            card = card.child(self.overdue_strip(k, is_today, cx).flex_shrink_0());
        }
        card.child(quick.flex_shrink_0())
            .child(
                div()
                    .id("todo-listing")
                    .flex()
                    .flex_col()
                    .flex_shrink(1.)
                    .min_h_0()
                    .mx(px(-16.))
                    .px(px(16.))
                    .pt(px(10.))
                    .overflow_y_scroll()
                    .track_scroll(&self.reports.panel.stack.list)
                    .on_drag_move::<DragRow>(cx.listener(
                        |this, e: &DragMoveEvent<DragRow>, _, cx| {
                            let reduce = cx.reduce_motion();
                            if this.row_drag_move(e.event.position, reduce) {
                                cx.notify();
                            }
                        },
                    ))
                    .child({
                        let lb = self.list_bounds.clone();
                        canvas(move |b, _, _| lb.set(b), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                    })
                    .when(self.rows().is_empty(), |d| {
                        d.child(
                            div()
                                .py(px(16.))
                                .px(px(10.))
                                .text_size(px(12.))
                                .text_color(k.text_3)
                                .child(t(if week {
                                    "todos.week.empty"
                                } else {
                                    "todos.empty.day"
                                })),
                        )
                    })
                    .children({
                        // 끄는 줄은 하위째 목록에서 빠진다
                        let roots: Vec<Row> = tree::visible_roots(self.rows())
                            .into_iter()
                            .filter(|r| !self.dragging(r.id()))
                            .collect();
                        let mut prev_group = false;
                        roots
                            .iter()
                            .enumerate()
                            .map(|(i, r)| {
                                let group = self.rows().iter().any(|c| c.parent() == Some(r.id()));
                                // 묶음 앞뒤는 22px 띄운다(.todo-unit.is-group + .todo-unit)
                                let gap = i > 0 && (prev_group || group);
                                prev_group = group;
                                self.unit(r, 0, k, cx).when(gap, |d| d.mt(px(22.)))
                            })
                            .collect::<Vec<_>>()
                    })
                    // 끄는 동안 — 빠진 줄 자리만큼 아래를 남긴다(밀려 내려간 마지막 줄이 잘리지 않게)
                    .when_some(self.row_drag.as_ref().map(|d| d.gap), |d, g| {
                        d.child(div().flex_shrink_0().h(g))
                    }),
            )
            .child(if week {
                // 주 목록은 묶음 규칙 없이 전부 센다. 늘 '이번 주에 할 것' 안내가 붙는다
                let rows = self.rows();
                let mut meta = String::new();
                if !rows.is_empty() {
                    let done = rows.iter().filter(|r| r.is("done", 1)).count();
                    meta = t_with(
                        "todos.week.done",
                        &[
                            ("total", &rows.len().to_string()),
                            ("done", &done.to_string()),
                        ],
                    );
                    // 지금 앱과 같은 구분자(U+00B7)
                    meta.push_str(" \u{b7} ");
                }
                meta.push_str(&t("todos.week.hint"));
                div()
                    .mt(px(14.))
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(meta)
            } else {
                div().when(progress.1 > 0, |d| {
                    d.mt(px(14.))
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(t_with(
                            "todos.meta.done",
                            &[
                                ("total", &progress.1.to_string()),
                                ("done", &progress.0.to_string()),
                            ],
                        ))
                })
            })
    }

    /// 휴가 종류 목록 — 해제는 이미 잡혀 있을 때만(빈 상태에서 '해제'는 고를 게 없다)
    fn vac_menu(&self, cur: Option<String>, k: &Tokens, cx: &mut Context<Self>) -> Deferred {
        let kinds = amber_gpui::vacations::KINDS;
        let mut items: Vec<MenuItem> = kinds
            .iter()
            .map(|kind| MenuItem {
                label: t(&format!("todos.vac.kind.{kind}")),
                checked: cur.as_deref() == Some(*kind),
                ..Default::default()
            })
            .collect();
        if cur.is_some() {
            items.push(MenuItem {
                label: t("todos.vac.clear"),
                ..Default::default()
            });
        }
        let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
        select_menu(
            "vac-menu",
            self.vac_anchor.get(),
            items,
            k,
            move |i, _, cx| {
                let _ = pick.update(cx, |this, cx| {
                    this.vac_open = false;
                    let r = match kinds.get(i) {
                        Some(kind) => {
                            amber_gpui::vacations::set_vacation(&this.db, &this.date, kind)
                        }
                        None => amber_gpui::vacations::remove_vacation(&this.db, &this.date),
                    };
                    if let Err(e) = r {
                        eprintln!("vacation: {e}");
                    }
                    this.reload_calendar();
                    cx.notify();
                });
            },
            move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    this.vac_open = false;
                    cx.notify();
                });
            },
        )
    }

    /// 지금 단위의 목록 — 일이면 그 날, 주면 그 주의 '주 할 일'
    pub(crate) fn rows(&self) -> &[Row] {
        if self.cal.unit == Unit::Week {
            &self.week_todos
        } else {
            &self.todos
        }
    }

    /// 새 항목이 들어갈 자리 — (날짜, scope). 주 할 일은 그 주의 시작일에 단다
    pub(crate) fn add_target(&self) -> (String, &'static str) {
        if self.cal.unit == Unit::Week {
            (dates::week_start_of(&self.date), "week")
        } else {
            (self.date.clone(), "day")
        }
    }

    /// 항목 + 그 서브트리. 깊이 > 0 은 24px 들여쓰고, 셋째 단부터는 왼쪽 가는 선이 묶는다
    fn unit(&self, node: &Row, depth: usize, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let kids: Vec<&Row> = self
            .rows()
            .iter()
            .filter(|c| c.parent() == Some(node.id()))
            .collect();
        let group = depth == 0 && !kids.is_empty();
        let kids: Vec<&Row> = kids
            .into_iter()
            .filter(|c| !self.dragging(c.id()))
            .collect();
        div()
            .flex()
            .flex_col()
            .when(depth > 0, |d| d.ml(px(INDENT)))
            .when(depth >= 2, |d| {
                d.border_l_1().border_color(k.border).pl(px(10.))
            })
            .child(self.row(node, group, depth >= 2, false, k, cx))
            .children(kids.into_iter().map(|c| self.unit(c, depth + 1, k, cx)))
            .when_some(
                self.adding_child
                    .as_ref()
                    .filter(|(pid, _)| *pid == node.id()),
                |d, (_, input)| {
                    d.child(
                        // .todo-row.todo-subadd 의 여백(4px 8px) 안에 선다
                        edit_box(self.todo_focus == TodoFocus::Child, k)
                            .ml(px(INDENT + 8.))
                            .mr(px(8.))
                            .my(px(4.))
                            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                                if e.keystroke.key == "escape" {
                                    this.adding_child = None;
                                    cx.notify();
                                }
                            }))
                            // 포커스 없는 곳을 눌러도 닫히며 저장된다(GPUI 는 그때 blur 가 안 난다)
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                if let Some((_, input)) = this.adding_child.clone() {
                                    let text = input.read(cx).value().trim().to_string();
                                    this.add_child(&text);
                                }
                                this.adding_child = None;
                                cx.notify();
                            }))
                            .child(Input::new(input).appearance(false).small().px(px(8.))),
                    )
                },
            )
    }

    fn row(
        &self,
        todo: &Row,
        group: bool,
        deep: bool,
        overdue: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = todo.id();
        let parent = todo.parent();
        let done = todo.is("done", 1);
        let carried = !overdue && todo.is("carried", 1);
        let gone = carried && todo.is("gone", 1);
        let content = todo
            .field("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let kids: Vec<&Row> = if overdue {
            Vec::new()
        } else {
            self.rows()
                .iter()
                .filter(|c| c.parent() == Some(id) && !c.is("gone", 1))
                .collect()
        };
        let kids_done = kids.iter().filter(|c| c.is("done", 1)).count();
        let toggle = cx.listener(move |this, _, _, cx| {
            this.toggle(id, parent, !done);
            cx.notify();
        });
        let progress = (!kids.is_empty()).then(|| {
            div()
                .flex_shrink_0()
                .ml(px(6.))
                .text_size(px(12.))
                .text_color(k.text_3)
                .child(format!("{kids_done}/{}", kids.len()))
        });

        let actions = self.row_actions(todo, overdue, carried, gone, k, cx);
        let read_only = overdue || carried;
        if group {
            let header = div()
                .group("todo-row")
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(8.))
                .py(px(4.))
                .mb(px(2.))
                .child(
                    div()
                        .flex_shrink(1.)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text_3)
                        .child(content.to_uppercase()),
                )
                .child(div().flex_1().min_w(px(12.)).h(px(1.)).bg(k.border))
                .children(progress)
                .children(actions);
            return self.drag_frame(header, id, !read_only, k, cx);
        }

        let row_id = SharedString::from(format!("row-{id}-{overdue}"));
        let editing = self
            .editing
            .as_ref()
            .filter(|(eid, _)| *eid == id && !read_only)
            .map(|(_, s)| s.clone());
        let text_click = {
            let content = content.clone();
            cx.listener(move |this, _, window, cx| {
                if read_only {
                    if !gone {
                        this.toggle(id, parent, !done);
                    }
                } else {
                    this.start_edit(id, &content, window, cx);
                }
                cx.notify();
            })
        };
        let el = div()
            .group("todo-row")
            .flex()
            .items_start()
            .gap(px(12.))
            .px(px(8.))
            .py(px(10.))
            .rounded(px(8.))
            // 끄는 동안엔 hover 배경을 끈다 — 들고 있는 줄만 보이게
            .when(self.row_drag.is_none(), |d| d.hover(|s| s.bg(glass(k))))
            .child(
                div()
                    .id(row_id.clone())
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .flex_1()
                    .min_w_0()
                    .child(
                        checkbox(
                            SharedString::from(format!("chk-{id}-{overdue}")),
                            k,
                            done,
                            gone,
                        )
                        .when(!gone, |d| d.on_click(toggle)),
                    )
                    .when_some(editing, |d, state| {
                        d.child(
                            edit_box(self.todo_focus == TodoFocus::Edit, k)
                                .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                                    if e.keystroke.key == "escape" {
                                        this.editing = None;
                                        cx.notify();
                                    }
                                }))
                                // 바깥 클릭 저장 — 포커스 없는 곳을 누르면 GPUI 는 blur 를 내지 않는다
                                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                    this.save_edit(cx);
                                    cx.notify();
                                }))
                                .child(
                                    Textarea::new(&state)
                                        .appearance(false)
                                        .small()
                                        .text_size(px(13.5))
                                        .line_height(relative(1.5)),
                                ),
                        )
                    })
                    .when(
                        self.editing
                            .as_ref()
                            .is_none_or(|(eid, _)| *eid != id || read_only),
                        |d| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("txt-{id}-{overdue}")))
                                    .tag(format!("txt-{id}-{overdue}"))
                                    .flex_1()
                                    .min_w_0()
                                    .cursor(if read_only {
                                        CursorStyle::PointingHand
                                    } else {
                                        CursorStyle::IBeam
                                    })
                                    .on_click(text_click)
                                    .text_size(px(if deep { 13. } else { 14. }))
                                    .line_height(relative(1.5))
                                    .text_color(if done || carried {
                                        k.text_3
                                    } else if deep {
                                        k.text_2
                                    } else {
                                        k.text
                                    })
                                    .when(done, |d| d.line_through())
                                    .child(content),
                            )
                        },
                    ),
            )
            .children(progress)
            .children(actions)
            .when(carried && !gone, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .ml(px(6.))
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(icon("chevron-right", 12.).text_color(k.text_3))
                        .child(dates::format_day_short(
                            todo.field("due_date")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default(),
                        )),
                )
            })
            .when(overdue, |d| {
                let due = todo
                    .field("due_date")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .flex_shrink_0()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(k.text_3)
                                .child(dates::format_day_short(&due)),
                        )
                        .child(
                            button(SharedString::from(format!("bring-{id}")), k, false, false)
                                .child(t(
                                    if self.date
                                        == dates::today_str(chrono::Local::now().timestamp_millis())
                                    {
                                        "todos.overdue.moveOne"
                                    } else {
                                        "todos.overdue.moveOneDay"
                                    },
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.move_here(&[id]);
                                    cx.notify();
                                })),
                        ),
                )
            });
        // 밀린 줄은 다른 날의 것이라 이 목록에서 끌지 않는다
        if overdue {
            el
        } else {
            self.drag_frame(el, id, !read_only, k, cx)
        }
    }

    /// 끄는 중인 줄(과 그 하위)인가
    pub(crate) fn dragging(&self, id: i64) -> bool {
        self.row_drag.as_ref().is_some_and(|d| d.id == id)
    }

    /// 끌기의 바탕 — 잰 자리(틈을 뺀 위치), 놓일 자리의 틈, hover 때 왼쪽 여백에 뜨는 손잡이.
    /// 넘긴 줄(고스트)과 지워진 기록은 손잡이가 없지만 자리는 잰다(그 사이로도 놓을 수 있다)
    fn drag_frame(&self, el: Div, id: i64, grip: bool, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let shift = self
            .row_drag
            .as_ref()
            .map_or(px(0.), |d| d.shift(id, std::time::Instant::now()));
        let map = self.row_bounds.clone();
        let this = cx.entity().downgrade();
        el.relative()
            // 그려진 자리만 내려간다(transform) — 자리는 그대로라 잰 자리에서 밀린 만큼 뺀다
            .top(shift)
            .child(
                canvas(
                    move |mut b, _, _| {
                        b.origin.y -= shift;
                        map.borrow_mut().insert(id, b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .when(grip, |d| {
                d.child(
                    // .todo-grip: 왼쪽 여백(-16px)에 세로 가운데, hover 때만 보인다(자리를 밀지 않는다)
                    div()
                        .absolute()
                        .left(px(-16.))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .id(SharedString::from(format!("grip-{id}")))
                                .tag(format!("grip-{id}"))
                                .flex()
                                .cursor(CursorStyle::OpenHand)
                                .opacity(0.)
                                .group_hover("todo-row", |s| s.opacity(1.))
                                .hover(|s| s.opacity(1.))
                                .child(icon("grip", 14.).text_color(k.text_3))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, e: &MouseDownEvent, _, _| {
                                        this.grip_down = Some(e.position);
                                    }),
                                )
                                .on_drag(DragRow { id }, move |d, offset, _, cx| {
                                    let ghost = this
                                        .update(cx, |s, cx| {
                                            let g = s.begin_row_drag(d.id, offset);
                                            cx.notify();
                                            g
                                        })
                                        .unwrap_or_default();
                                    cx.new(|_| ghost)
                                }),
                        ),
                )
            })
    }

    /// 줄 끝 hover 동작 — 글자를 눌러서는 할 수 없는 일에만 아이콘을 둔다(이름 바꾸기는 글자를 누르면 된다).
    /// 기록만 남은 줄은 그 기록 지우기, 넘긴 줄(고스트)은 하위 추가만, 밀린 줄은 없음
    fn row_actions(
        &self,
        todo: &Row,
        overdue: bool,
        carried: bool,
        gone: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if overdue {
            return None;
        }
        let (id, parent) = (todo.id(), todo.parent());
        let name = todo
            .field("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let act = |key: &str, icon_name: &str, danger: bool| {
            let (danger_c, soft, hover_bg, text) = (k.danger, k.danger_soft, glass(k), k.text);
            div()
                .id(SharedString::from(format!("{key}-{id}")))
                .tag(format!("{key}-{id}"))
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .group("row-act")
                .when(danger, |d| d.hover(move |s| s.bg(soft)))
                .when(!danger, |d| d.hover(move |s| s.bg(hover_bg)))
                .child(
                    icon(icon_name, 13.)
                        .text_color(k.text_3)
                        .group_hover("row-act", move |s| {
                            s.text_color(if danger { danger_c } else { text })
                        }),
                )
        };
        let row = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.))
            .opacity(0.)
            .group_hover("todo-row", |s| s.opacity(1.));
        Some(if gone {
            row.child(
                act("rec", "trash", true).on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some(Confirm::Record {
                        id,
                        name: name.clone(),
                    });
                    cx.notify();
                })),
            )
        } else {
            let add =
                act("add", "plus", false).on_click(cx.listener(move |this, _, window, cx| {
                    this.start_child(id, window, cx);
                    cx.notify();
                }));
            if carried {
                row.child(add)
            } else {
                row.child(add)
                    .child(act("park", "arrow-right", false).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.park(id, parent);
                            cx.notify();
                        },
                    )))
                    .child(act("del", "trash", true).on_click(cx.listener(
                        move |this, _, _, cx| {
                            // 하위가 없어도 묻는다 — 지우는 동작에 예외를 두면 손이 먼저 나간다
                            let live: Vec<Row> = this
                                .rows()
                                .iter()
                                .filter(|r| !r.is("gone", 1))
                                .cloned()
                                .collect();
                            let count = tree::descendant_count(&live, id);
                            this.confirm = Some(Confirm::Delete {
                                id,
                                parent,
                                name: name.clone(),
                                count,
                            });
                            cx.notify();
                        },
                    )))
            }
        })
    }

    /// 글자를 누르면 그 자리에서 고친다 — Enter 저장, Shift+Enter 줄바꿈, Esc 취소, 바깥을 누르면 저장
    pub(crate) fn start_edit(
        &mut self,
        id: i64,
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(1, 12)
                .submit_on_enter(true);
            s.set_value(content.to_string(), window, cx);
            s
        });
        cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { shift: false, .. } | InputEvent::Blur => {
                this.save_edit(cx);
                cx.notify();
            }
            _ => {}
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        self.editing = Some((id, state));
    }

    /// 빈 값과 바뀌지 않은 값은 취소다
    fn save_edit(&mut self, cx: &mut Context<Self>) {
        let Some((id, state)) = self.editing.take() else {
            return;
        };
        let text = state.read(cx).value().to_string();
        let content = text.trim();
        let before = self
            .todos
            .iter()
            .find(|r| r.id() == id)
            .and_then(|r| r.field("content"))
            .and_then(|v| v.as_str());
        if content.is_empty() || Some(content) == before {
            return;
        }
        if let Err(e) = amber_gpui::todos::update_todo_content(&self.db, id, content) {
            eprintln!("updateTodoContent: {e}");
        }
        self.reload();
    }

    pub(crate) fn start_child(&mut self, parent: i64, window: &mut Window, cx: &mut Context<Self>) {
        let state =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("todos.child.placeholder")));
        cx.subscribe_in(
            &state,
            window,
            |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).value().trim().to_string();
                    if this.add_child(&text) {
                        input.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                    cx.notify();
                }
                // 바깥을 누르면 적던 하위 항목을 버리지 않고 저장하며 닫는다
                InputEvent::Blur => {
                    let text = input.read(cx).value().trim().to_string();
                    this.add_child(&text);
                    this.adding_child = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        self.adding_child = Some((parent, state));
    }

    /// 하위 항목을 만들고 조상의 완료를 다시 센다(끝난 묶음에 새 일이 생기면 다시 열린다)
    fn add_child(&mut self, text: &str) -> bool {
        let Some((parent, _)) = self.adding_child.as_ref() else {
            return false;
        };
        let parent = *parent;
        if text.is_empty() {
            return false;
        }
        let (due, scope) = self.add_target();
        let r = amber_gpui::todos::create_todo(&self.db, text, &due, Some(parent), scope)
            .and_then(|_| amber_gpui::todos::recompute_chain_from(&self.db, Some(parent)));
        if let Err(e) = r {
            eprintln!("addChild: {e}");
        }
        self.reload();
        self.reload_calendar();
        true
    }

    /// 언젠가(서랍)로 — 날짜를 뗀다. 빠진 자리를 두고 부모 완료를 다시 센다
    fn park(&mut self, id: i64, parent: Option<i64>) {
        let r = amber_gpui::todos::park_subtree(&self.db, id).and_then(|_| match parent {
            Some(p) => amber_gpui::todos::recompute_chain_from(&self.db, Some(p)),
            None => Ok(()),
        });
        if let Err(e) = r {
            eprintln!("park: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    /// 확인 창에서 '삭제'를 눌렀다
    pub(crate) fn confirmed(&mut self, cx: &mut Context<Self>) {
        let r = match self.confirm.take() {
            Some(Confirm::Delete { id, parent, .. }) => {
                amber_gpui::todos::delete_todo(&self.db, id)
                    .and_then(|_| amber_gpui::todos::recompute_chain_from(&self.db, parent))
            }
            Some(Confirm::Record { id, .. }) => {
                amber_gpui::todos::remove_carry(&self.db, id, &self.date)
            }
            Some(Confirm::Parked { id, .. }) => amber_gpui::todos::delete_todo(&self.db, id),
            Some(Confirm::Note { rel, .. }) => self.delete_note(&rel).map_err(|e| {
                eprintln!("deleteNote: {e}");
                rusqlite::Error::InvalidQuery
            }),
            Some(Confirm::Diagram { rel, .. }) => self.delete_diagram(&rel).map_err(|e| {
                eprintln!("deleteDiagram: {e}");
                rusqlite::Error::InvalidQuery
            }),
            Some(Confirm::Block { id, .. }) => {
                let r = amber_gpui::blocks::delete_block(&self.db, id);
                self.reload_blocks();
                r
            }
            Some(Confirm::Comment { id, .. }) => {
                self.delete_thread(&id, cx);
                Ok(())
            }
            None => Ok(()),
        };
        if let Err(e) = r {
            eprintln!("delete: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    /// 밀린 할 일 — 오늘 계획의 재료라 날짜 바로 아래. 접힌 채 시작하고 머리를 누르면 펼친다
    fn overdue_strip(&self, k: &Tokens, is_today: bool, cx: &mut Context<Self>) -> Div {
        let ids: Vec<i64> = self.overdue.iter().map(Node::id).collect();
        let rows = tree::flatten_subset(&self.overdue);
        div()
            .flex()
            .flex_col()
            .mx(px(-14.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                div()
                    .id("overdue-head")
                    .tag("overdue-head")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(18.))
                    .pr(px(14.))
                    .py(px(9.))
                    .cursor_pointer()
                    .child(
                        icon(
                            if self.overdue_open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            14.,
                        )
                        .text_color(k.text_3),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text)
                            .child(t("todos.overdue.title")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(self.overdue.len().to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        button("bring-all", k, false, false)
                            .child(t(if is_today {
                                "todos.overdue.moveAll"
                            } else {
                                "todos.overdue.moveAllDay"
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.move_here(&ids);
                                cx.notify();
                            })),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overdue_open = !this.overdue_open;
                        cx.notify();
                    })),
            )
            .when(self.overdue_open, |d| {
                d.child(
                    div()
                        .px(px(6.))
                        .pb(px(6.))
                        .flex()
                        .flex_col()
                        .children(rows.iter().take(OVERDUE_LIMIT).filter_map(|r| {
                            let todo = self.overdue.iter().find(|o| o.id() == r.id)?;
                            Some(
                                div()
                                    .ml(px(r.depth as f32 * INDENT))
                                    .child(self.row(todo, false, false, true, k, cx)),
                            )
                        }))
                        .when(self.overdue.len() > OVERDUE_LIMIT, |d| {
                            d.child(
                                div()
                                    .px(px(10.))
                                    .py(px(6.))
                                    .text_size(px(12.))
                                    .text_color(k.text_3)
                                    .child(t_with(
                                        "todos.overdue.more",
                                        &[("n", &(self.overdue.len() - OVERDUE_LIMIT).to_string())],
                                    )),
                            )
                        }),
                )
            })
    }
}
