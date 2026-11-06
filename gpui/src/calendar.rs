// 할 일 탭의 달력 카드 — 지금 앱 MiniCalendar.tsx 와 styles.css 의 .todo-cal, .cal-* 를 옮긴 것.
// 날짜 칸은 타일이다: 판이 넓으면(안쪽 520px 이상) 칸이 64px 로 커지고 공휴일/휴가 이름과 개수가 들어간다.
// 오늘 = 링, 고른 날 = 강조 유리 필(둘은 다른 축이라 겹치면 둘 다 읽힌다). 일요일, 공휴일 = 빨강, 토요일 = 파랑, 휴가 = 호박.

use std::collections::HashMap;

use amber_gpui::dates::{self, YearMonth};
use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    Day,
    Week,
}

impl Unit {
    pub fn id(self) -> &'static str {
        match self {
            Unit::Day => "day",
            Unit::Week => "week",
        }
    }
}

/// 제목을 눌러 오르내리는 단계 — 일 격자 → 월 고르기 → 연 고르기(천장)
#[derive(Clone, Copy, PartialEq)]
pub enum Level {
    Day,
    Month,
    Year,
}

/// 연 판은 10년 한 판 + 앞뒤 한 칸(흐리게) = 12칸 3×4 — 월 판과 같은 판형
const DECADE: i32 = 10;

fn decade_start(y: i32) -> i32 {
    y.div_euclid(DECADE) * DECADE
}

/// 달력이 그리는 데 쓰는 것 — 보고 있는 달, 날짜별 (끝낸 수, 전체), 휴가
pub struct CalendarData {
    pub cursor: YearMonth,
    pub level: Level,
    pub unit: Unit,
    pub counts: HashMap<String, (usize, usize)>,
    /// 주 시작일 → 그 주의 '주 할 일' (끝낸 수, 전체). 주 단위에서 그 주 첫 칸에 표식으로만 쓴다
    pub week_counts: HashMap<String, (usize, usize)>,
    pub vacations: HashMap<String, String>,
}

/// 카드 안쪽 폭이 이 이상이면 칸이 커지고 이름과 개수가 들어간다(@container min-width: 520px)
const WIDE: f32 = 520.;

fn glass_fill(k: &Tokens, accent: bool) -> Background {
    let (a, b) = if accent {
        k.glass_fill_accent
    } else {
        k.glass_fill
    };
    linear_gradient(150., linear_color_stop(a, 0.), linear_color_stop(b, 1.))
}

impl Spike {
    pub(crate) fn calendar(&self, k: &Tokens, inner_width: f32, cx: &mut Context<Self>) -> Div {
        let cal = &self.cal;
        let wide = inner_width >= WIDE;
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let sel_week = dates::week_start_of(&self.date);
        let YearMonth { year, month } = cal.cursor;
        // AI 가 그날 리포트를 만드는 중이면 막대가 레일의 생성 중 점과 같은 호흡으로 깜빡인다
        let generating = self.reports.runs.running_dates();
        let reduce = cx.reduce_motion();

        let segmented = div()
            .flex()
            .gap(px(2.))
            .p(px(2.))
            .bg(k.surface_2)
            .border_1()
            .border_color(k.border)
            .rounded(px(7.))
            .children([Unit::Day, Unit::Week].map(|u| {
                let active = cal.unit == u;
                let label = t(if u == Unit::Day {
                    "todos.unit.day"
                } else {
                    "todos.unit.week"
                });
                div()
                    .id(u.id())
                    .tag(format!("unit-{}", u.id()))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if active { k.text } else { k.text_2 })
                    .cursor_pointer()
                    .when(active, |d| {
                        let mut shadow = k.glass_shadow(false);
                        shadow.truncate(1);
                        shadow.push(BoxShadow {
                            color: k.shadow_sm,
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(2.),
                            spread_radius: px(0.),
                            inset: false,
                        });
                        d.bg(k.glass_raise).shadow(shadow)
                    })
                    .when(!active, |d| d.hover(|s| s.text_color(k.text)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| this.set_unit(u, window, cx)))
            }));

        let nav = |id: &'static str, name: &str, delta: i32, cx: &mut Context<Self>| {
            div()
                .id(id)
                .tag(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_color(k.text_2)
                .cursor_pointer()
                .hover(|s| s.bg(k.surface_2).text_color(k.text))
                .child(icon(name, 15.).text_color(k.text_2))
                // 화살표 한 번 = 지금 보고 있는 판 하나(달, 해, 10년)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let c = this.cal.cursor;
                    this.cal.cursor = match this.cal.level {
                        Level::Day => dates::add_months(c, delta),
                        Level::Month => YearMonth {
                            year: c.year + delta,
                            ..c
                        },
                        Level::Year => YearMonth {
                            year: c.year + delta * DECADE,
                            ..c
                        },
                    };
                    this.reload_calendar();
                    cx.notify();
                }))
        };
        let level = cal.level;
        let start = decade_start(year);
        let title = match level {
            Level::Day => dates::format_month_title(year, month),
            Level::Month => dates::format_year_title(year),
            Level::Year => format!("{start} – {}", start + DECADE - 1),
        };

        let head = div()
            .flex()
            .items_center()
            .mx(px(-12.))
            .mb(px(8.))
            .pl(px(12.))
            .pr(px(10.))
            .py(px(6.))
            .border_b_1()
            .border_color(k.glass_edge)
            // .segmented 는 inline-flex — 내용 폭만 차지한다. 월/연 판에선 고를 날이 없어 숨기되 자리는 둔다
            .child(
                div()
                    .flex_1()
                    .flex()
                    .when(level == Level::Day, |d| d.child(segmented)),
            )
            .child(
                // 연 단계가 천장이라 더 올라갈 곳이 없다 — 누를 수 없게 둔다(흐려지진 않는다)
                div()
                    .id("cal-title")
                    .tag("cal-title")
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text)
                    .child(title)
                    .when(level != Level::Year, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(k.surface_2))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cal.level = match this.cal.level {
                                    Level::Day => Level::Month,
                                    _ => Level::Year,
                                };
                                cx.notify();
                            }))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_end()
                    .gap(px(2.))
                    .child(nav("cal-prev", "chevron-left", -1, cx))
                    .child(nav("cal-next", "chevron-right", 1, cx)),
            );

        let dow =
            div()
                .flex()
                .mb(px(2.))
                .children(
                    dates::weekdays_short(0)
                        .into_iter()
                        .enumerate()
                        .map(|(i, d)| {
                            div()
                                .flex_1()
                                .py(px(4.))
                                .flex()
                                .justify_center()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(match i {
                                    0 => k.cal_sun,
                                    6 => k.cal_sat,
                                    _ => k.text_3,
                                })
                                .child(d)
                        }),
                );

        let (tile_h, gap) = if wide { (64., 4.) } else { (44., 3.) };
        let grid = dates::month_grid_dates(year, month, 0);
        let weeks = grid.chunks(7).map(|week| {
            div().flex().gap(px(gap)).children(week.iter().map(|date| {
                let d = dates::parse(date).expect("격자 날짜");
                let in_month = dates::month_of(date) == Some(cal.cursor);
                // 일 모드는 오늘과 고른 날이 배타(today ? today : selected ? selected : adjacent),
                // 주 모드는 고른 주 7칸이 띠가 되고 오늘 링이 그 위에 겹친다
                let is_today = date == &today;
                let (selected, adjacent) = match cal.unit {
                    Unit::Day => (
                        date == &self.date && !is_today,
                        !in_month && !is_today && date != &self.date,
                    ),
                    Unit::Week => (dates::week_start_of(date) == sel_week, !in_month),
                };
                let vac = cal.vacations.get(date);
                let hol = dates::holiday_of(date);
                let w = chrono::Datelike::weekday(&d).num_days_from_sunday();
                let num_color = if is_today {
                    k.text
                } else if vac.is_some() {
                    k.vacation
                } else if w == 0 || hol.is_some() {
                    k.cal_sun
                } else if w == 6 {
                    k.cal_sat
                } else if adjacent {
                    k.text_3
                } else {
                    k.text
                };
                let dim = adjacent && (vac.is_some() || w == 0 || w == 6 || hol.is_some());
                let label = match (vac, &hol) {
                    (Some(kind), _) => t(&format!("todos.vac.kind.{kind}")),
                    (None, Some(h)) => h.name.clone(),
                    _ => String::new(),
                };
                let count = cal.counts.get(date).copied();
                let busy = generating.contains(date);
                // 주 할 일은 일별 진행을 뺏지 않고 그 주 첫 칸에 표식을 더한다(있을 때만)
                let week_mark = (cal.unit == Unit::Week && dates::week_start_of(date) == *date)
                    .then(|| cal.week_counts.get(date).copied())
                    .flatten()
                    .filter(|(_, total)| *total > 0);
                let date_owned = date.clone();

                // 고른 휴가 날(일 모드)은 강조 유리 대신 흐린 링 — 휴가 이름의 호박색이 묻히지 않게
                let sel_vac = cal.unit == Unit::Day && selected && vac.is_some();
                let accent_fill = selected && !sel_vac;
                let sheen = k.glass_shadow(true)[0].clone();
                let shadow = match cal.unit {
                    Unit::Day if is_today => {
                        vec![Tokens::ring(k.glass_edge_accent, 1.5), sheen.clone()]
                    }
                    Unit::Day if sel_vac => vec![Tokens::ring(k.text_3, 1.5)],
                    Unit::Day if selected => {
                        vec![Tokens::ring(k.glass_edge_accent, 1.), sheen.clone()]
                    }
                    Unit::Week if is_today => vec![Tokens::ring(k.glass_edge_accent, 1.5), sheen],
                    Unit::Week if selected => vec![Tokens::ring(k.glass_edge_accent, 1.)],
                    _ => Vec::new(),
                };

                div()
                    .id(SharedString::from(format!("cal-{date}")))
                    .tag(format!("cal-{date}"))
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(tile_h))
                    .when(wide, |d| d.pt(px(7.)).px(px(8.)).pb(px(8.)))
                    .when(!wide, |d| d.pt(px(6.)).px(px(7.)).pb(px(7.)))
                    .rounded(px(8.))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .cursor_pointer()
                    .when(!adjacent, |d| d.bg(k.text_mix(0.04)))
                    .when(accent_fill, |d| d.bg(glass_fill(k, true)))
                    .when(!accent_fill, |d| d.hover(|s| s.bg(k.text_mix(0.07))))
                    .shadow(shadow)
                    .child(
                        div()
                            .text_size(px(if wide { 13. } else { 12. }))
                            .line_height(px(if wide { 13. } else { 12. }))
                            .font_weight(if is_today {
                                FontWeight::EXTRA_BOLD
                            } else if vac.is_some() {
                                FontWeight::BOLD
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(num_color)
                            .when(dim, |d| d.opacity(0.5))
                            .child(chrono::Datelike::day(&d).to_string()),
                    )
                    .when(wide && in_month && !label.is_empty(), |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(7.))
                                .right(px(8.))
                                .max_w(relative(0.6))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(10.5))
                                .line_height(px(10.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if vac.is_some() { k.vacation } else { k.cal_sun })
                                .child(label),
                        )
                    })
                    .when(count.is_some() || busy, |d| {
                        // 다 끝낸 날은 톤을 낮춘다 — 지난 날들이 하얗게 줄지어 서면 오늘보다 눈에 띈다.
                        // 리포트를 만드는 날은 할 일이 없어도 빈 막대를 띄워 깜빡인다(어느 날이 도는지 보여야 한다)
                        let (done, total) = count.unwrap_or_default();
                        let pct = if done > 0 {
                            ((done * 100) as f32 / total as f32).round().max(6.)
                        } else {
                            0.
                        };
                        let full = count.is_some() && done >= total;
                        let bar = div()
                            .flex_1()
                            .h(px(3.))
                            .rounded(px(2.))
                            .bg(k.text_mix(0.12))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(pct / 100.))
                                    .rounded(px(2.))
                                    .bg(if full { k.text_3 } else { k.text_2 }),
                            );
                        let bar = if busy && !reduce {
                            bar.with_animation(
                                ElementId::Name(format!("cal-gen-{date_owned}").into()),
                                Animation::new(std::time::Duration::from_millis(1100)).repeat(),
                                |d, x| {
                                    d.opacity(
                                        0.3 + 0.7 * (1. - (std::f32::consts::PI * 2. * x).cos())
                                            / 2.,
                                    )
                                },
                            )
                            .into_any_element()
                        } else {
                            bar.into_any_element()
                        };
                        d.child(div().flex().items_center().gap(px(6.)).child(bar).when(
                            wide && count.is_some(),
                            |d| {
                                d.child(
                                    div()
                                        .text_size(px(10.5))
                                        .line_height(px(10.5))
                                        .text_color(k.text_3)
                                        .child(format!("{done}/{total}")),
                                )
                            },
                        ))
                    })
                    .when_some(week_mark, |d, (done, total)| {
                        let on = done >= total;
                        d.child(
                            div()
                                .absolute()
                                .top(px(7.))
                                .right(px(8.))
                                .w(px(7.))
                                .h(px(2.))
                                .rounded(px(1.))
                                .border_1()
                                .border_color(if on { k.text } else { k.text_3 })
                                .when(on, |d| d.bg(k.text)),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.go_date(date_owned.clone());
                        cx.notify();
                    }))
            }))
        });

        let card = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(12.))
            .pb(px(12.))
            .border_1()
            .border_color(k.glass_edge)
            .rounded(px(12.))
            .bg(glass_fill(k, false))
            .shadow(k.glass_shadow(false))
            .child(head);
        if level == Level::Day {
            return card
                .child(dow)
                .child(div().flex().flex_col().gap(px(gap)).children(weeks));
        }

        // 월/연 고르기 — 알약 12칸(3×4). 판을 오갈 때 아래 시간표가 밀리지 않게 일 판과 같은 높이
        let (ty, tm) = dates::month_of(&today).map_or((0, 0), |m| (m.year, m.month));
        let cells: Vec<(String, String, bool, bool, bool, YearMonth, Level)> = match level {
            Level::Month => dates::months_short()
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    let m = i as u32 + 1;
                    let at = YearMonth { year, month: m };
                    (
                        format!("pick-m-{m}"),
                        label,
                        year == ty && m == tm,
                        m == month,
                        false,
                        at,
                        Level::Day,
                    )
                })
                .collect(),
            _ => (start - 1..start + DECADE + 1)
                .map(|y| {
                    let outside = y < start || y >= start + DECADE;
                    let at = YearMonth { year: y, month };
                    (
                        format!("pick-y-{y}"),
                        y.to_string(),
                        y == ty,
                        y == year,
                        outside,
                        at,
                        Level::Month,
                    )
                })
                .collect(),
        };
        let pick = |(tag, label, is_today, selected, adjacent, at, next): (
            String,
            String,
            bool,
            bool,
            bool,
            YearMonth,
            Level,
        )| {
            // 지금 앱처럼 배타 — today ? today : selected ? selected : adjacent
            let selected = selected && !is_today;
            let adjacent = adjacent && !is_today && !selected;
            let sheen = k.glass_shadow(true)[0].clone();
            let seat = k.glass_shadow(false)[1..].to_vec();
            let mut shadow = Vec::new();
            if is_today {
                shadow.push(Tokens::ring(k.glass_edge_accent, 1.5));
                shadow.push(sheen.clone());
            }
            if selected {
                shadow.push(sheen);
                shadow.extend(seat);
                shadow.push(Tokens::ring(k.glass_edge_accent, 1.));
            }
            div()
                .id(SharedString::from(tag.clone()))
                .tag(tag)
                .flex_1()
                .h(px(44.))
                .m(px(3.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .cursor_pointer()
                .font_weight(if is_today {
                    FontWeight::BOLD
                } else if selected {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(if adjacent { k.text_3 } else { k.text })
                .when(selected, |d| d.bg(glass_fill(k, true)))
                .when(!selected, |d| d.hover(|s| s.bg(glass_fill(k, false))))
                .shadow(shadow)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cal.cursor = at;
                    this.cal.level = next;
                    this.reload_calendar();
                    cx.notify();
                }))
        };
        let min_h = 26. + 6. * tile_h + 5. * gap;
        let rows: Vec<_> = cells.into_iter().map(pick).collect::<Vec<_>>();
        let mut grid = div().min_h(px(min_h)).flex().flex_col().justify_center();
        let mut it = rows.into_iter().peekable();
        while it.peek().is_some() {
            grid = grid.child(div().flex().children(it.by_ref().take(3)));
        }
        card.child(grid)
    }
}
