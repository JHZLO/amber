// 스키마 개요 — 지금 앱 SchemaOverview.tsx. ERD 탭의 [개요] 보기: 통계 한 줄, 테이블 목록(찾기), 참조 통계와 미해결 목록.
// 스냅샷(schema.json)으로 그린다 — 연결이 끊긴 채로도 마지막 구조가 보인다. 머리줄(이름, 보기 전환)은 erd_view 가 갖는다.

use amber_gpui::erd_gen::{SchemaSnapshot, SnapshotTable, infer_references};
use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

fn is_audit(name: &str) -> bool {
    name.ends_with("_aud")
}

/// Number.prototype.toFixed — f64 의 정확한 십진 전개에서 반올림하고, 딱 가운데면 큰 쪽.
/// Rust 의 서식은 가운데를 짝수 쪽으로 보내서(86.5 → 86) 다르다. 1 이상의 값만 쓴다(소수 52자리 안에서 정확)
pub fn to_fixed(x: f64, digits: usize) -> String {
    let exact = format!("{:.60}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let (keep, rest) = frac.split_at(digits.min(frac.len()));
    let mut out: Vec<u8> = int.bytes().chain(keep.bytes()).collect();
    if rest.as_bytes().first().is_some_and(|d| *d >= b'5') {
        // 끝에서부터 올린다
        let mut i = out.len();
        loop {
            if i == 0 {
                out.insert(0, b'1');
                break;
            }
            i -= 1;
            if out[i] == b'9' {
                out[i] = b'0';
            } else {
                out[i] += 1;
                break;
            }
        }
    }
    let s = String::from_utf8(out).unwrap_or_default();
    let (i, f) = s.split_at(s.len() - digits);
    let sign = if x < 0. { "-" } else { "" };
    if digits == 0 {
        format!("{sign}{i}")
    } else {
        format!("{sign}{i}.{f}")
    }
}

/// 행 수 추정치 — 1.2M, 86K 처럼 짧게
pub fn compact(n: f64) -> String {
    if n >= 1_000_000. {
        format!(
            "{}M",
            to_fixed(n / 1_000_000., if n >= 10_000_000. { 0 } else { 1 })
        )
    } else if n >= 1_000. {
        format!(
            "{}K",
            to_fixed(n / 1_000., if n >= 10_000. { 0 } else { 1 })
        )
    } else {
        amber_gpui::resin::svg::js(n as f64)
    }
}

pub struct Stats {
    pub tables: usize,
    pub columns: usize,
    pub audit: usize,
    pub physical: usize,
    pub logical: usize,
    pub unresolved: Vec<(String, String)>,
}

/// 통계 — Envers 의 rev → revinfo 물리 FK 는 도메인 관계가 아니라 다이어그램에서도 선으로 그리지 않으니 뺀다
pub fn stats(s: &SchemaSnapshot) -> Stats {
    let (edges, unresolved) = infer_references(s);
    let domain: Vec<_> = edges
        .iter()
        .filter(|e| !is_audit(&e.from) && e.to != "revinfo")
        .collect();
    let physical = domain.iter().filter(|e| e.physical).count();
    Stats {
        tables: s.tables.len(),
        columns: s.tables.iter().map(|tb| tb.columns.len()).sum(),
        audit: s.tables.iter().filter(|tb| is_audit(&tb.name)).count(),
        physical,
        logical: domain.len() - physical,
        unresolved: unresolved
            .into_iter()
            .map(|u| (u.table, u.column))
            .collect(),
    }
}

/// 목록 — 감사 테이블은 뒤로, 이름순(로캘 없는 localeCompare). 찾기는 이름, 코멘트, 컬럼 이름과 코멘트에서
pub fn rows<'a>(s: &'a SchemaSnapshot, query: &str) -> Vec<&'a SnapshotTable> {
    let q = query.trim().to_lowercase();
    let mut v: Vec<&SnapshotTable> = s.tables.iter().collect();
    v.sort_by(|a, b| {
        is_audit(&a.name)
            .cmp(&is_audit(&b.name))
            .then_with(|| amber_gpui::vault::default_compare(&a.name, &b.name))
    });
    if q.is_empty() {
        return v;
    }
    v.into_iter()
        .filter(|tb| {
            tb.name.to_lowercase().contains(&q)
                || tb.comment.to_lowercase().contains(&q)
                || tb.columns.iter().any(|c| {
                    c.name.to_lowercase().contains(&q) || c.comment.to_lowercase().contains(&q)
                })
        })
        .collect()
}

impl Spike {
    /// [개요] 로 바꿀 때 — 찾기 칸은 창이 있어야 만들어진다
    pub(crate) fn erd_overview_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.erd.ov_input.is_some() {
            return;
        }
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("erd.db.overview.searchPh")));
        cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                this.erd.ov_query = input.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        self.erd.ov_input = Some(input);
    }

    /// .db-ov — 통계 한 줄, 왼쪽 테이블 목록, 오른쪽 참조 통계와 미해결 목록
    pub(crate) fn erd_overview(
        &self,
        snap: Option<&SchemaSnapshot>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = snap else {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .text_size(px(13.))
                .text_color(k.text_3)
                .child(icon("table", 22.).text_color(k.text_3))
                .child(t("erd.db.overview.noSnapshot"))
                .into_any_element();
        };
        let st = stats(s);
        let eyebrow = |text: String| {
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(k.text_3)
                .child(text.to_uppercase())
        };
        let panel = || {
            div()
                .flex()
                .flex_col()
                .min_h_0()
                .bg(k.surface)
                .border_1()
                .border_color(k.border)
                .rounded(px(10.))
                .overflow_hidden()
        };
        let head = || {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(8.))
                .border_b_1()
                .border_color(k.border)
                .text_size(px(11.5))
                .text_color(k.text_3)
        };
        // 원래 화면의 구분점(<span>·</span>)을 그대로 둔다
        let dot = || div().child("\u{00B7}");
        let kv = div()
            .tag("erd-ov-kv")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(8.))
            .gap_y(px(6.))
            .text_size(px(12.))
            .text_color(k.text_3)
            .child(t_with(
                "erd.db.overview.stats",
                &[
                    ("tables", &st.tables.to_string()),
                    ("columns", &st.columns.to_string()),
                ],
            ))
            .child(dot())
            .child(t_with(
                "erd.db.overview.audit",
                &[("n", &st.audit.to_string())],
            ))
            .child(dot())
            .child(t_with(
                "erd.db.overview.snapshot",
                &[
                    ("time", &crate::erd_view::header_time(s.synced_at)),
                    ("server", &s.server),
                ],
            ));
        let list = rows(s, &self.erd.ov_query);
        let hover = k.text.opacity(0.04);
        let mut items = div()
            .id("erd-ov-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .text_size(px(12.5));
        if list.is_empty() {
            items = items.child(
                div()
                    .py(px(18.))
                    .px(px(12.))
                    .flex()
                    .justify_center()
                    .text_size(px(12.5))
                    .text_color(k.text_3)
                    .child(t("erd.db.overview.empty")),
            );
        }
        for tb in list {
            let audit = is_audit(&tb.name);
            let name = tb.name.clone();
            items =
                items.child(
                    div()
                        .id(SharedString::from(format!("erd-ov-{}", tb.name)))
                        .tag(&format!("erd-ov-{}", tb.name))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(12.))
                        .py(px(6.))
                        .border_b_1()
                        .border_color(k.border)
                        .text_color(k.text_2)
                        .cursor_pointer()
                        .hover(move |st| st.bg(hover))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_family(crate::markdown::MONO)
                                        .text_size(px(12.))
                                        .when(audit, |d| d.text_color(k.text_3))
                                        .when(!audit, |d| {
                                            d.text_color(k.text).font_weight(FontWeight::MEDIUM)
                                        })
                                        .child(tb.name.clone()),
                                )
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(11.5))
                                        .text_color(k.text_3)
                                        .child(if tb.comment.is_empty() {
                                            t("erd.db.overview.noComment")
                                        } else {
                                            tb.comment.clone()
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.5))
                                .text_color(k.text_3)
                                .child(t_with(
                                    "erd.db.overview.cols",
                                    &[("n", &tb.columns.len().to_string())],
                                )),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.5))
                                .text_color(k.text_3)
                                .children(tb.rows_estimate.as_ref().and_then(|n| n.as_f64()).map(
                                    |n| t_with("erd.db.overview.rows", &[("n", &compact(n))]),
                                )),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.erd_pick_from_overview(&name, cx);
                            cx.notify();
                        })),
                );
        }
        let search = div()
            .w(px(200.))
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_1()
            .border_color(k.border_strong)
            .rounded(px(6.))
            .bg(k.surface)
            .child(icon("search", 12.).text_color(k.text_3))
            .children(self.erd.ov_input.as_ref().map(|i| {
                div()
                    .tag("erd-ov-search")
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .child(Input::new(i).appearance(false).px_0())
            }));
        let left = panel()
            .flex_basis(relative(0.))
            .flex_grow(1.35)
            .min_w_0()
            .child(
                head()
                    .child(eyebrow(t("erd.db.overview.tableList")))
                    .child(div().flex_1())
                    .child(search),
            )
            .child(items);
        let stat = |label: &str, v: usize| {
            div()
                .flex_1()
                .border_1()
                .border_color(k.border)
                .rounded(px(8.))
                .px(px(10.))
                .py(px(8.))
                .bg(k.surface)
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text_3)
                        .child(t(label).to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::BOLD)
                        .line_height(relative(1.3))
                        .text_color(k.text)
                        .child(v.to_string()),
                )
        };
        let mut side = div()
            .flex_basis(relative(0.))
            .flex_grow(1.)
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(stat("erd.db.stat.physical", st.physical))
                    .child(stat("erd.db.stat.logical", st.logical))
                    .child(stat("erd.db.stat.unresolved", st.unresolved.len())),
            );
        if !st.unresolved.is_empty() {
            side = side.child(
                panel()
                    .flex_1()
                    .child(head().child(eyebrow(t("erd.db.stat.unresolvedList"))))
                    .child(
                        div()
                            .id("erd-ov-unresolved")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .text_size(px(12.5))
                            .children(st.unresolved.iter().map(|(tb, col)| {
                                div()
                                    .px(px(12.))
                                    .py(px(4.))
                                    .border_b_1()
                                    .border_color(k.border)
                                    .flex()
                                    .font_family(crate::markdown::MONO)
                                    .text_size(px(12.))
                                    .child(
                                        div()
                                            .text_color(k.text)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(tb.clone()),
                                    )
                                    .child(div().text_color(k.text_3).child(format!(".{col}")))
                            })),
                    )
                    .child(
                        div()
                            .px(px(12.))
                            .py(px(8.))
                            .border_t_1()
                            .border_color(k.border)
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(t("erd.db.stat.unresolvedHint")),
                    ),
            );
        }
        div()
            .tag("erd-overview")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(18.))
            .py(px(16.))
            .overflow_hidden()
            .child(kv)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(14.))
                    .child(left)
                    .child(side),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{compact, to_fixed};

    /// node 로 낸 값 — (1.25).toFixed(1) = 1.3, (86.5).toFixed(0) = 87, 9.95 는 2진수로 9.9499… 라 9.9
    #[test]
    fn compact_rounds_like_to_fixed() {
        for (n, want) in [
            (1_250_000., "1.3M"),
            (1_050_000., "1.1M"),
            (86_500., "87K"),
            (12_345_678., "12M"),
            (1_000., "1.0K"),
            (999., "999"),
            (9_950., "9.9K"),
        ] {
            assert_eq!(compact(n), want, "{n}");
        }
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(9.96, 1), "10.0");
    }
}
