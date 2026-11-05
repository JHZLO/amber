// 할 일 탭 오른쪽 서랍 — 지금 앱 TodoDrawer.tsx. '언젠가'(날짜를 뗀 항목)는 원래 묶음별로 모으고
// 오래 묵은 것부터 보인다. 카드를 누르면 **오늘**로 올린다(서랍은 날짜를 모르는 곳, 꺼내는 뜻은 '지금 한다').
// '제안' 탭(AI 가 훑은 후보)은 AI 실행을 붙이는 단계에서 채운다.

use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::tree::{Node, ancestor_path};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Confirm;
use crate::theme::Tokens;

pub const WIDTH: f32 = 296.;
const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Suggest,
    Parked,
}

/// 내려놓은 지 며칠 — 그날 안이면 0
pub fn parked_days(parked_at: i64, now: i64) -> i64 {
    ((now - parked_at).div_euclid(DAY_MS)).max(0)
}

/// 맨 위 줄만 — 하위는 부모와 함께 내려가므로 따로 세우면 같은 덩어리가 두 번 보인다
pub fn parked_roots(rows: &[Row]) -> Vec<Row> {
    rows.iter()
        .filter(|r| r.parent().is_none_or(|p| !rows.iter().any(|x| x.id() == p)))
        .cloned()
        .collect()
}

/// 같은 묶음에서 내려온 것끼리 — 머리글은 그 묶음의 첫 카드가 나온 순서(오래 묵은 것부터)
pub fn group_parked(roots: Vec<Row>, ancestors: &[Row]) -> Vec<(String, Vec<Row>)> {
    let mut out: Vec<(String, Vec<Row>)> = Vec::new();
    for r in roots {
        let key = ancestor_path(ancestors, r.parent()).join(" › ");
        match out.last_mut() {
            Some((k, items)) if *k == key => items.push(r),
            _ => out.push((key, vec![r])),
        }
    }
    out
}

impl Spike {
    pub(crate) fn drawer(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let roots = parked_roots(&self.parked);
        let tab =
            |id: &'static str, label: String, count: usize, which: Tab, cx: &mut Context<Self>| {
                let active = self.drawer_tab == which;
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(8.))
                    .pt(px(3.))
                    .pb(px(4.))
                    .rounded(px(6.))
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .text_color(if active { k.text } else { k.text_2 })
                    .when(active, |d| {
                        d.bg(k.glass_raise)
                            .shadow(k.glass_shadow(false)[..1].to_vec())
                    })
                    .child(label)
                    .when(count > 0, |d| {
                        d.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(k.text_3)
                                .child(count.to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.drawer_tab = which;
                        cx.notify();
                    }))
            };
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pt(px(12.))
            .pr(px(10.))
            .pb(px(10.))
            .pl(px(14.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .child(tab(
                        "tab-suggest",
                        t("todos.suggest.title"),
                        0,
                        Tab::Suggest,
                        cx,
                    ))
                    .child(tab(
                        "tab-parked",
                        t("todos.parked.title"),
                        roots.len(),
                        Tab::Parked,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("drawer-close")
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(k.text_mix(0.06)))
                    .child(icon("chevron-right", 14.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_parked_open(false);
                        cx.notify();
                    })),
            );

        let note = |text: String| {
            div()
                .px(px(4.))
                .py(px(6.))
                .text_size(px(11.5))
                .line_height(relative(1.6))
                .text_color(k.text_3)
                .child(text)
        };
        let body = div()
            .id("drawer-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(10.));
        let (body, foot) = match self.drawer_tab {
            Tab::Suggest => (body.child(note(t("todos.suggest.idle"))), None),
            Tab::Parked if roots.is_empty() => (body.child(note(t("todos.parked.empty"))), None),
            Tab::Parked => {
                let now = chrono::Local::now().timestamp_millis();
                let groups = group_parked(roots, &self.parked_anc);
                let body =
                    body.children(groups.into_iter().enumerate().map(|(gi, (key, items))| {
                        div()
                            .flex()
                            .flex_col()
                            .when(gi > 0, |d| d.mt(px(16.)))
                            .when(!key.is_empty(), |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .pl(px(22.))
                                        .pr(px(10.))
                                        .mb(px(6.))
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .text_size(px(10.5))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(k.text_3)
                                                .child(key.to_uppercase()),
                                        )
                                        .child(
                                            div().flex_1().min_w(px(12.)).h(px(1.)).bg(k.border),
                                        ),
                                )
                            })
                            .children(items.iter().map(|r| self.parked_card(r, now, k, cx)))
                    }));
                (
                    body,
                    Some(
                        div()
                            .px(px(14.))
                            .pt(px(10.))
                            .pb(px(14.))
                            .child(note(t("todos.parked.hint"))),
                    ),
                )
            }
        };

        div()
            .w(px(WIDTH))
            .flex_shrink_0()
            .h_full()
            .pt(px(8.))
            .pr(px(8.))
            .pb(px(8.))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
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
                    .child(head)
                    .child(body)
                    .children(foot),
            )
    }

    /// 카드 하나 = 동작 하나. 올라가면 왼쪽에 화살표가 뜬다 — 움직이는 방향이 곧 가는 곳(오늘 목록은 왼쪽)
    fn parked_card(&self, r: &Row, now: i64, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, name) = (
            r.id(),
            r.field("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        );
        let kids = self
            .parked
            .iter()
            .filter(|x| x.parent() == Some(id))
            .count();
        let days = r
            .field("parked_at")
            .and_then(|v| v.as_i64())
            .map_or(0, |at| parked_days(at, now));
        let small = |s: String| {
            div()
                .flex_shrink_0()
                .text_size(px(10.5))
                .text_color(k.text_3)
                .child(s)
        };
        let (danger, soft) = (k.danger, k.danger_soft);
        div()
            .id(SharedString::from(format!("parked-{id}")))
            .group("parked-card")
            .relative()
            .flex()
            .items_center()
            .gap(px(8.))
            .pt(px(9.))
            .pb(px(9.))
            .pr(px(10.))
            .pl(px(22.))
            .mb(px(6.))
            .border_1()
            .border_color(k.border)
            .rounded(px(8.))
            .bg(k.surface_2)
            .cursor_pointer()
            .hover(|s| s.border_color(k.border_strong).bg(k.surface))
            .child(
                div()
                    .absolute()
                    .left(px(4.))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover("parked-card", |s| s.opacity(1.))
                    .child(icon("chevron-left", 12.).text_color(k.text_3)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .line_height(relative(1.45))
                    .child(name.clone()),
            )
            .when(kids > 0, |d| {
                d.child(small(t_with(
                    "todos.parked.kids",
                    &[("n", &kids.to_string())],
                )))
            })
            .child(small(if days == 0 {
                t("todos.parked.age.zero")
            } else {
                t_with("todos.parked.age", &[("n", &days.to_string())])
            }))
            .child(
                div()
                    .id(SharedString::from(format!("parked-del-{id}")))
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .opacity(0.)
                    .group_hover("parked-card", |s| s.opacity(1.))
                    .text_color(k.text_3)
                    .hover(move |s| s.bg(soft).text_color(danger))
                    .child(icon("trash", 13.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.confirm = Some(Confirm::Parked {
                            id,
                            name: name.clone(),
                        });
                        cx.notify();
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pull(id);
                cx.notify();
            }))
    }

    /// 서랍에서 오늘로. 붙은 자리(원래 부모 또는 그 날 새로 살아난 묶음)의 완료를 다시 센다
    fn pull(&mut self, id: i64) {
        let today = amber_gpui::dates::today_str(chrono::Local::now().timestamp_millis());
        let r =
            amber_gpui::todos::unpark_subtree(&self.db, id, &today).and_then(
                |parent| match parent {
                    Some(p) => amber_gpui::todos::recompute_chain_from(&self.db, Some(p)),
                    None => Ok(()),
                },
            );
        if let Err(e) = r {
            eprintln!("unpark: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    pub(crate) fn set_parked_open(&mut self, open: bool) {
        self.parked_open = open;
        amber_gpui::prefs::set(
            &self.db,
            "amber.todo.parked-open",
            if open { "1" } else { "0" },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_floor_and_never_negative() {
        assert_eq!(parked_days(0, DAY_MS - 1), 0);
        assert_eq!(parked_days(0, DAY_MS * 3 + 5), 3);
        assert_eq!(parked_days(10, 0), 0);
    }
}
