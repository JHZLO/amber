// 할 일 탭 왼쪽 아래 타임테이블 카드 — 지금 앱 DayTimetable.tsx 와 styles.css 의 .day-tt, .tt-*.
// 일/주는 시간 그리드(1시간 44px), 월은 날짜별 목록. 블록 가로 위치는 timetable::layout_column 이 정한다
// (포함은 들여쓰고, 교차는 칸을 나눈다). 끌어 만들기, 옮기기, 늘이기는 다음 단계에서 붙인다.

use amber_gpui::dates;
use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::timetable::{self, BLOCK_GAP, Box as TtBox, GUTTER};
use amber_gpui::tree::Node;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::Spike;
use crate::theme::Tokens;

pub const HOUR_H: f32 = 44.;
const HOURS_W: f32 = 46.;
const GRID_PAD_R: f32 = 10.;

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Day,
    Week,
    Month,
}

impl View {
    pub fn id(self) -> &'static str {
        match self {
            View::Day => "day",
            View::Week => "week",
            View::Month => "month",
        }
    }
    pub fn from_id(id: &str) -> View {
        match id {
            "week" => View::Week,
            "month" => View::Month,
            _ => View::Day,
        }
    }
}

/// 이 뷰가 불러올 날짜 범위(TodoView 의 ttRange)
pub fn range(view: View, selected: &str) -> (String, String) {
    match view {
        View::Week => {
            let ws = dates::week_start_of(selected);
            let we = dates::shift_day(&ws, 6);
            (ws, we)
        }
        View::Month => {
            let m = dates::month_of(selected).expect("날짜");
            let grid = dates::month_grid_dates(m.year, m.month, 0);
            let inside: Vec<&String> = grid
                .iter()
                .filter(|d| dates::month_of(d) == Some(m))
                .collect();
            (inside[0].clone(), inside[inside.len() - 1].clone())
        }
        View::Day => (selected.to_string(), selected.to_string()),
    }
}

fn hhmm(m: i64) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

fn min_to_y(m: f64) -> f32 {
    (m / 60. * HOUR_H as f64) as f32
}

impl Spike {
    /// 할 일에 이어진 블록은 그 할 일의 글을 제목으로 쓴다. 지워진 할 일의 블록은 남은 title 로
    fn block_title(&self, b: &Row) -> String {
        let title = b
            .field("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        match b.field("todo_id").and_then(|v| v.as_i64()) {
            Some(tid) => self
                .todos
                .iter()
                .find(|r| r.id() == tid)
                .and_then(|r| {
                    r.field("content")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .filter(|s| !s.is_empty())
                .or(Some(title).filter(|s| !s.is_empty()))
                .unwrap_or_else(|| t("todos.tt.deletedTodo")),
            None => title,
        }
    }

    fn block_done(&self, b: &Row) -> bool {
        b.field("todo_id")
            .and_then(|v| v.as_i64())
            .is_some_and(|tid| self.todos.iter().any(|r| r.id() == tid && r.is("done", 1)))
    }

    pub(crate) fn tt_card(&self, k: &Tokens, inner_w: f32, cx: &mut Context<Self>) -> Div {
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let planned: i64 = self
            .tt_blocks
            .iter()
            .map(|b| {
                b.field("end_min").and_then(|v| v.as_i64()).unwrap_or(0)
                    - b.field("start_min").and_then(|v| v.as_i64()).unwrap_or(0)
            })
            .sum();
        let planned_label = (planned > 0).then(|| {
            let (h, m) = (planned / 60, planned % 60);
            let parts: Vec<String> = [
                (h > 0).then(|| t_with("todos.tt.hours", &[("h", &h.to_string())])),
                (m > 0).then(|| t_with("todos.tt.minutes", &[("m", &m.to_string())])),
            ]
            .into_iter()
            .flatten()
            .collect();
            t_with("todos.tt.planned", &[("time", &parts.join(" "))])
        });
        let seg = div()
            .flex()
            .gap(px(2.))
            .p(px(2.))
            .bg(k.surface_2)
            .border_1()
            .border_color(k.border)
            .rounded(px(7.))
            .children([View::Day, View::Week, View::Month].map(|v| {
                let active = self.tt_view == v;
                div()
                    .id(SharedString::from(format!("tt-{}", v.id())))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if active { k.text } else { k.text_2 })
                    .cursor_pointer()
                    .when(active, |d| {
                        d.bg(k.glass_raise)
                            .shadow(k.glass_shadow(false)[..1].to_vec())
                    })
                    .child(t(&format!("todos.tt.view.{}", v.id())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tt_view = v;
                        amber_gpui::prefs::set(&this.db, "amber.todo.tt-view", v.id());
                        this.tt_scrolled = None;
                        this.reload_blocks();
                        cx.notify();
                    }))
            }));
        let head = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(12.))
            .pr(px(10.))
            .py(px(6.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text_3)
                    .child(t("todos.tt.label")),
            )
            .child(div().flex_1())
            .children(
                planned_label.map(|l| div().text_size(px(11.5)).text_color(k.text_3).child(l)),
            )
            .child(seg);

        let card = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .mt(px(8.))
            .overflow_hidden()
            .border_1()
            .border_color(k.glass_edge)
            .rounded(px(12.))
            .bg(linear_gradient(
                150.,
                linear_color_stop(k.glass_fill.0, 0.),
                linear_color_stop(k.glass_fill.1, 1.),
            ))
            .shadow(k.glass_shadow(false))
            .child(head);

        if self.tt_view == View::Month {
            return card.child(self.tt_agenda(k, &today, cx));
        }

        let days: Vec<String> = match self.tt_view {
            View::Week => dates::week_days(&dates::week_start_of(&self.date)),
            _ => vec![self.date.clone()],
        };
        let col_w = 100. / days.len() as f64;
        let grid_w = (inner_w - HOURS_W - GRID_PAD_R - 1.) as f64;
        let now_min = timetable::now_minute(chrono::Local::now().timestamp_millis()) as f64;
        let today_col = days.iter().position(|d| *d == today);

        let week_head = (self.tt_view == View::Week).then(|| {
            div()
                .flex_shrink_0()
                .flex()
                .py(px(4.))
                .pr(px(GRID_PAD_R))
                .border_b_1()
                .border_color(k.border)
                .child(div().w(px(HOURS_W)).flex_shrink_0())
                .children(days.iter().map(|d| {
                    let (is_today, sel) = (*d == today, *d == self.date && *d != today);
                    let date = d.clone();
                    let day = dates::parse(d).map_or(0, |x| chrono::Datelike::day(&x));
                    div()
                        .id(SharedString::from(format!("tt-day-{d}")))
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(1.))
                        .pt(px(3.))
                        .pb(px(4.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(sel, |d| d.bg(k.surface_3))
                        .hover(|s| s.bg(k.surface_2))
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(k.text_3)
                                .child(dates::weekday_short(d)),
                        )
                        .child(
                            div()
                                .size(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_today { k.primary_fg } else { k.text_2 })
                                .when(is_today, |d| d.bg(k.primary_bg))
                                .child(day.to_string()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.go_date(date.clone());
                            cx.notify();
                        }))
                }))
        });

        // 칸마다 배치 — 칸 안 상자는 거터를 뺀 폭(layoutColumn 의 root)
        let mut placed = Vec::new();
        for (i, d) in days.iter().enumerate() {
            let col: Vec<Json> = self
                .tt_blocks
                .iter()
                .filter(|b| b.field("date").and_then(|v| v.as_str()) == Some(d.as_str()))
                .map(|b| Json::Object(b.clone()))
                .collect();
            let root = TtBox {
                pct_l: i as f64 * col_w,
                px_l: 0.,
                pct_w: col_w,
                px_w: -GUTTER,
            };
            placed.extend(
                timetable::layout_column(&col, root, grid_w)
                    .into_iter()
                    .map(|p| (i, p)),
            );
        }

        let lines = (0..24).map(|h| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(h as f32 * HOUR_H))
                .h(px(1.))
                .bg(k.glass_edge)
        });
        let vlines = (1..days.len()).map(|i| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative((i as f64 * col_w / 100.) as f32))
                .w(px(1.))
                .bg(k.border)
        });
        let blocks = placed.into_iter().map(|(col, p)| {
            let b: Row = p.b.as_object().cloned().unwrap_or_default();
            let start = b.field("start_min").and_then(|v| v.as_f64()).unwrap_or(0.);
            let end = b.field("end_min").and_then(|v| v.as_f64()).unwrap_or(0.);
            let title = self.block_title(&b);
            let done = self.block_done(&b);
            let running = days[col] == today && !done && now_min >= start && now_min < end;
            let h = min_to_y(end - start);
            let left = (grid_w * p.bx.pct_l / 100. + p.bx.px_l) as f32;
            let width = ((grid_w * p.bx.pct_w / 100. + p.bx.px_w - BLOCK_GAP) as f32).max(28.);
            let accent = linear_gradient(
                150.,
                linear_color_stop(k.glass_fill_accent.0, 0.),
                linear_color_stop(k.glass_fill_accent.1, 1.),
            );
            let untitled = title.is_empty();
            div()
                .absolute()
                .top(px(min_to_y(start)))
                .left(px(left))
                .w(px(width))
                .h(px((h - 2.).max(12.)))
                .px(px(8.))
                .py(px(3.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .overflow_hidden()
                .rounded(px(6.))
                .border_1()
                .border_color(if running {
                    k.glass_edge_accent
                } else {
                    k.glass_edge
                })
                .bg(k.surface)
                .shadow(if p.depth > 0 {
                    // 얹힌 블록은 바탕색 1px 테로 아래 블록과 갈라 보이게(.tt-block.cascade)
                    vec![
                        k.glass_shadow(false)[0].clone(),
                        Tokens::outer_ring(k.surface, 1.),
                    ]
                } else {
                    k.glass_shadow(false)
                })
                .text_size(px(12.))
                .line_height(relative(1.35))
                .text_color(if done { k.text_3 } else { k.text })
                .when(running, |d| d.font_weight(FontWeight::BOLD))
                .child(div().absolute().inset_0().bg(if running {
                    accent
                } else {
                    linear_gradient(
                        150.,
                        linear_color_stop(k.glass_fill.0, 0.),
                        linear_color_stop(k.glass_fill.1, 1.),
                    )
                }))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .when(untitled, |d| d.text_color(k.text_3))
                        .when(done, |d| d.line_through())
                        .child(if untitled {
                            t("todos.tt.untitled")
                        } else {
                            title
                        }),
                )
                .when(h >= 34., |d| {
                    d.child(
                        div()
                            .text_size(px(10.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(if running { k.text_2 } else { k.text_3 })
                            .child(format!("{} – {}", hhmm(start as i64), hhmm(end as i64))),
                    )
                })
        });
        let now = today_col.map(|c| {
            div()
                .absolute()
                .top(px(min_to_y(now_min)))
                .left(relative((c as f64 * col_w / 100.) as f32))
                .w(relative((col_w / 100.) as f32))
                .h(px(1.))
                .bg(k.glass_edge_accent)
                .child(
                    div()
                        .absolute()
                        .left(px(-3.5))
                        .top(px(-3.))
                        .size(px(7.))
                        .rounded_full()
                        .bg(k.text)
                        .shadow(vec![Tokens::outer_ring(k.text_mix(0.16), 2.)]),
                )
        });
        let hours = (1..24).map(|h| {
            div()
                .absolute()
                .right(px(8.))
                .top(px(h as f32 * HOUR_H - 7.))
                .text_size(px(10.5))
                .text_color(k.text_3)
                .child(format!("{h:02}:00"))
        });

        card.children(week_head).child(
            div()
                .id("tt-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.tt_scroll)
                .child(
                    div()
                        .relative()
                        .h(px(HOUR_H * 24. + 16.))
                        .child(
                            div()
                                .absolute()
                                .top(px(8.))
                                .left_0()
                                .w(px(HOURS_W))
                                .h(px(HOUR_H * 24.))
                                .children(hours),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(8.))
                                .left(px(HOURS_W))
                                .right(px(GRID_PAD_R))
                                .h(px(HOUR_H * 24.))
                                .border_l_1()
                                .border_color(k.glass_edge)
                                .children(lines)
                                .children(vlines)
                                .children(blocks)
                                .children(now),
                        ),
                ),
        )
    }

    /// 월 — 날짜별 묶음 목록
    fn tt_agenda(&self, k: &Tokens, today: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut agenda: Vec<(String, Vec<&Row>)> = Vec::new();
        for b in &self.tt_blocks {
            let d = b
                .field("date")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            match agenda.last_mut() {
                Some((day, list)) if *day == d => list.push(b),
                _ => agenda.push((d, vec![b])),
            }
        }
        div()
            .id("tt-agenda")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(6.))
            .pr(px(10.))
            .pb(px(12.))
            .pl(px(12.))
            .when(agenda.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(4.))
                        .py(px(14.))
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(t("todos.tt.monthEmpty")),
                )
            })
            .children(agenda.into_iter().map(|(d, list)| {
                let is_today = d == today;
                let date = d.clone();
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(SharedString::from(format!("ag-{d}")))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pt(px(10.))
                            .px(px(2.))
                            .pb(px(4.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(k.text_2)
                            .cursor_pointer()
                            .child(dates::format_day_long(&d))
                            .when(is_today, |x| {
                                x.child(
                                    div()
                                        .px(px(6.))
                                        .py(px(1.))
                                        .rounded_full()
                                        .bg(k.primary_bg)
                                        .text_color(k.primary_fg)
                                        .text_size(px(10.))
                                        .child(t("todos.today")),
                                )
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.go_date(date.clone());
                                cx.notify();
                            })),
                    )
                    .children(list.into_iter().map(|b| {
                        let s = b.field("start_min").and_then(|v| v.as_i64()).unwrap_or(0);
                        let e = b.field("end_min").and_then(|v| v.as_i64()).unwrap_or(0);
                        let title = self.block_title(b);
                        let done = self.block_done(b);
                        div()
                            .flex()
                            .gap(px(8.))
                            .px(px(6.))
                            .py(px(4.))
                            .rounded(px(6.))
                            .text_size(px(12.))
                            .hover(|s| s.bg(k.surface_2))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.))
                                    .text_color(k.text_3)
                                    .child(format!("{} – {}", hhmm(s), hhmm(e))),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .when(done, |d| d.text_color(k.text_3).line_through())
                                    .child(if title.is_empty() {
                                        t("todos.tt.untitled")
                                    } else {
                                        title
                                    }),
                            )
                    }))
            }))
    }

    /// 지금 뷰의 범위로 블록을 다시 읽고, 처음 보는 범위면 스크롤을 맞춘다(오늘이 보이면 지금 - 1시간, 아니면 09:00)
    pub(crate) fn reload_blocks(&mut self) {
        let (from, to) = range(self.tt_view, &self.date);
        self.tt_blocks = amber_gpui::blocks::list_blocks(&self.db, &from, &to).unwrap_or_default();
        if self.tt_view == View::Month {
            return;
        }
        let key = format!("{}:{from}", self.tt_view.id());
        if self.tt_scrolled.as_deref() == Some(key.as_str()) {
            return;
        }
        self.tt_scrolled = Some(key);
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let target = if (from.as_str()..=to.as_str()).contains(&today.as_str()) {
            (timetable::now_minute(chrono::Local::now().timestamp_millis()) - 60).max(0)
        } else {
            9 * 60
        };
        self.tt_scroll
            .set_offset(point(px(0.), px(-min_to_y(target as f64))));
    }
}
