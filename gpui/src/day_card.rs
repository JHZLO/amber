// 할 일 탭의 하루 카드 — 지금 앱 TodoView.tsx 의 .todo-card(머리 띠, 밀린 할 일, 빠른 추가, 목록, 완료 수)를 옮긴 것.
// 묶음 머리글(자식이 있는 최상위 줄)은 체크박스 없이 작은 대문자 + 가는 선이다 — 컨테이너는 체크하는 물건이 아니다.
// 다른 날로 넘긴 줄(고스트)은 흐리게, 도착 날짜를 '› 11/6' 로 단다. 체크는 된다(거기서 끝내면 도착 날짜에서도 끝난다).

use amber_gpui::dates;
use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::tree::{self, Node};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::theme::Tokens;

/// 중첩 단계마다 더 들어가는 폭
const INDENT: f32 = 24.;
const OVERDUE_LIMIT: usize = 20;

fn glass(k: &Tokens) -> Background {
    linear_gradient(
        150.,
        linear_color_stop(k.glass_fill.0, 0.),
        linear_color_stop(k.glass_fill.1, 1.),
    )
}

/// .btn.btn-sm — 유리 버튼. ghost 면 평소엔 테두리, 바탕이 없다(카드 머리 띠의 버튼)
pub fn button(id: impl Into<ElementId>, k: &Tokens, ghost: bool, disabled: bool) -> Stateful<Div> {
    let (fill, edge, shadow) = (glass(k), k.glass_edge, k.glass_shadow(false));
    div()
        .id(id)
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
                linear_color_stop(k.glass_fill.0.opacity(2.), 0.),
                linear_color_stop(k.glass_fill.1.opacity(2.), 1.),
            );
            let text = k.text;
            d.cursor_pointer()
                .hover(move |s| s.bg(fill_hover).shadow(hover).text_color(text))
        })
}

/// 둥근 체크 — 끝낸 일은 강조 유리 필에 체크 표시(.checkbox)
fn checkbox(id: impl Into<ElementId>, k: &Tokens, checked: bool, disabled: bool) -> Stateful<Div> {
    let accent = linear_gradient(
        150.,
        linear_color_stop(k.glass_fill_accent.0, 0.),
        linear_color_stop(k.glass_fill_accent.1, 1.),
    );
    div()
        .id(id)
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

        let nav = |id: &'static str, name: &str, delta: i64, cx: &mut Context<Self>| {
            div()
                .id(id)
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(k.text_mix(0.06)))
                .child(icon(name, 16.).text_color(k.text_2))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let d = dates::shift_day(&this.date, delta);
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
                    .child(dates::format_day_long(&self.date)),
            )
            .child({
                // 휴가 — 꺼짐은 표준 트리거, 켜짐은 호박 글자. 고르기(목록)는 다음 단계에서
                let vac = self.cal.vacations.get(&self.date);
                button("vac", k, true, false)
                    .when_some(vac, |d, kind| {
                        d.text_color(k.vacation)
                            .child(t(&format!("todos.vac.kind.{kind}")))
                    })
                    .when(vac.is_none(), |d| d.child(t("todos.vac.set")))
                    .child(icon("chevron-down", 13.).text_color(if vac.is_some() {
                        k.vacation
                    } else {
                        k.text_3
                    }))
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
            .child(
                button("drawer", k, true, false)
                    .ml(px(8.))
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
            );

        let quick = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .mx(px(-14.))
            .pl(px(16.))
            .pr(px(18.))
            .py(px(12.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(icon("plus", 15.).text_color(k.text_3))
            .child(
                div()
                    .flex_1()
                    .text_size(px(14.))
                    .child(Input::new(&self.input).appearance(false)),
            );

        let progress = tree::day_progress(&self.todos);
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
            .child(head);
        if self.date <= today && !self.overdue.is_empty() {
            card = card.child(self.overdue_strip(k, is_today, cx));
        }
        card.child(quick)
            .child(
                div()
                    .id("todo-listing")
                    .flex()
                    .flex_col()
                    .mx(px(-16.))
                    .px(px(16.))
                    .pt(px(10.))
                    .overflow_y_scroll()
                    .when(self.todos.is_empty(), |d| {
                        d.child(
                            div()
                                .py(px(16.))
                                .px(px(10.))
                                .text_size(px(12.))
                                .text_color(k.text_3)
                                .child(t("todos.empty.day")),
                        )
                    })
                    .children({
                        let roots = tree::visible_roots(&self.todos);
                        let mut prev_group = false;
                        roots
                            .iter()
                            .enumerate()
                            .map(|(i, r)| {
                                let group = self.todos.iter().any(|c| c.parent() == Some(r.id()));
                                // 묶음 앞뒤는 22px 띄운다(.todo-unit.is-group + .todo-unit)
                                let gap = i > 0 && (prev_group || group);
                                prev_group = group;
                                self.unit(r, 0, k, cx).when(gap, |d| d.mt(px(22.)))
                            })
                            .collect::<Vec<_>>()
                    }),
            )
            .when(progress.1 > 0, |d| {
                d.child(
                    div()
                        .mt(px(14.))
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(t_with(
                            "todos.meta.done",
                            &[
                                ("total", &progress.1.to_string()),
                                ("done", &progress.0.to_string()),
                            ],
                        )),
                )
            })
    }

    /// 항목 + 그 서브트리. 깊이 > 0 은 24px 들여쓰고, 셋째 단부터는 왼쪽 가는 선이 묶는다
    fn unit(&self, node: &Row, depth: usize, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let kids: Vec<&Row> = self
            .todos
            .iter()
            .filter(|c| c.parent() == Some(node.id()))
            .collect();
        let group = depth == 0 && !kids.is_empty();
        div()
            .flex()
            .flex_col()
            .when(depth > 0, |d| d.ml(px(INDENT)))
            .when(depth >= 2, |d| {
                d.border_l_1().border_color(k.border).pl(px(10.))
            })
            .child(self.row(node, group, depth >= 2, false, k, cx))
            .children(kids.into_iter().map(|c| self.unit(c, depth + 1, k, cx)))
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
            self.todos
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

        if group {
            return div()
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
                .children(progress);
        }

        let row_id = SharedString::from(format!("row-{id}-{overdue}"));
        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .px(px(8.))
            .py(px(10.))
            .rounded(px(8.))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
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
                    ),
            )
            .children(progress)
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
            })
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
