// 개념 탭 — 지금 앱 App.tsx 의 개념 판. 머리 줄(검색, [+ 추가]), 상태 탭(학습 중/배움/전체 + 수)과 정렬,
// 태그 칩(최대 12, 여럿 고르면 모두 가진 것만), 왼쪽 목록 카드 | 오른쪽 상세 카드.
// 정렬은 탭마다 따로 기억한다(학습 중 = 낮은 확신부터, 배움/전체 = 최근 고친 순). 검색은 200ms 늦춰 거른다.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use amber_gpui::concepts::{self, ConceptFilter};
use amber_gpui::db::Row;
use amber_gpui::i18n::t;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, primary_button};
use crate::menu::{MenuItem, select_menu};
use crate::settings_view::glass;
use crate::theme::Tokens;

/// 개념 목록 판(amber.concepts.list-width) — usePaneResize 기본값
pub const LIST_PANE: crate::pane::PaneGrab = crate::pane::PaneGrab {
    key: "amber.concepts.list-width",
    min: 240.,
    max: 640.,
};
pub const LIST_DEFAULT: f32 = 340.;

const TABS: [(&str, &str); 3] = [
    ("learning", "common.status.learning"),
    ("learned", "common.status.learned"),
    ("all", "app.tab.all"),
];

const SORTS: [(&str, &str); 4] = [
    ("canonical", "app.sort.canonical"),
    ("recent_updated", "app.sort.recentUpdated"),
    ("recent_created", "app.sort.recentCreated"),
    ("title", "app.sort.title"),
];

fn default_sort(tab: &str) -> &'static str {
    if tab == "learning" {
        "canonical"
    } else {
        "recent_updated"
    }
}

pub struct ConceptsUi {
    pub status: &'static str,
    /// 탭별 정렬 — 사용자가 바꾸면 그 탭만
    pub sort_by_tab: HashMap<&'static str, &'static str>,
    pub search_input: Entity<InputState>,
    /// 늦춰서 확정된 검색어
    pub search: String,
    pub active_tags: Vec<String>,
    pub list: Vec<Row>,
    pub counts: (i64, i64, i64),
    pub tags: Vec<(String, i64)>,
    pub selected: Option<i64>,
    pub ready: bool,
    pub load_error: Option<String>,
    pub width: f32,
    pub sort_open: bool,
    pub sort_anchor: Rc<Cell<Bounds<Pixels>>>,
    pub list_scroll: ScrollHandle,
    debounce: Option<Task<()>>,
    /// 고른 개념의 본문과 편집 상태
    pub detail: Option<crate::concept_detail::DetailState>,
    /// 초안이 있는데 다른 곳으로 가려 했다 — '계속 편집 / 버리고 이동'을 묻는 동안 미뤄 둔 이동
    pub pending: Option<Box<dyn FnOnce(&mut Spike, &mut Context<Spike>)>>,
    /// AI 보강 창
    pub augment: Option<crate::concept_augment::Augment>,
}

impl ConceptsUi {
    pub fn new(width: f32, window: &mut Window, cx: &mut Context<Spike>) -> Self {
        let _ = width;
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("app.search.placeholder")));
        cx.subscribe_in(&search_input, window, |this, s, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let v = s.read(cx).value().to_string();
                // 200ms 늦춰 거른다 — 새 글자가 오면 앞의 것은 버린다
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        // 편집 중엔 거르기를 미룬다 — 저장/취소로 초안이 사라지면 그때 반영
                        if s.concepts.detail.as_ref().is_some_and(|d| d.dirty) {
                            return;
                        }
                        s.concepts.search = v;
                        s.reload_concepts();
                        cx.notify();
                    });
                });
                this.concepts.debounce = Some(task);
            }
        })
        .detach();
        ConceptsUi {
            status: "learning",
            sort_by_tab: HashMap::new(),
            search_input,
            search: String::new(),
            active_tags: Vec::new(),
            list: Vec::new(),
            counts: (0, 0, 0),
            tags: Vec::new(),
            selected: None,
            ready: false,
            load_error: None,
            width: LIST_DEFAULT,
            sort_open: false,
            sort_anchor: Rc::default(),
            list_scroll: ScrollHandle::new(),
            debounce: None,
            detail: None,
            pending: None,
            augment: None,
        }
    }

    pub fn sort(&self) -> &'static str {
        self.sort_by_tab
            .get(self.status)
            .copied()
            .unwrap_or_else(|| default_sort(self.status))
    }
}

fn row_i64(r: &Row, k: &str) -> i64 {
    r.get(k).and_then(Json::as_i64).unwrap_or_default()
}

fn row_str<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).and_then(Json::as_str).unwrap_or_default()
}

impl Spike {
    /// 목록, 상태별 수, 태그를 다시 읽는다
    pub(crate) fn reload_concepts(&mut self) {
        let c = &self.concepts;
        let search = c.search.trim().to_string();
        let filter = ConceptFilter {
            status: c.status.to_string(),
            search: (!search.is_empty()).then_some(search),
            tags: (!c.active_tags.is_empty()).then(|| c.active_tags.clone()),
            sort: Some(c.sort().to_string()),
            limit: None,
        };
        let res = (|| -> Result<_, String> {
            let list = concepts::list_concepts(&self.db, &filter).map_err(|e| e.to_string())?;
            let counts = concepts::status_counts(&self.db).map_err(|e| e.to_string())?;
            let tags = concepts::all_tags(&self.db).map_err(|e| e.to_string())?;
            Ok((list, counts, tags))
        })();
        let c = &mut self.concepts;
        match res {
            Ok((list, counts, tags)) => {
                c.list = list;
                let n = |k: &str| counts[k].as_i64().unwrap_or_default();
                c.counts = (n("learning"), n("learned"), n("all"));
                c.tags = tags
                    .iter()
                    .map(|t| (row_str(t, "name").to_string(), row_i64(t, "count")))
                    .collect();
                c.ready = true;
                c.load_error = None;
            }
            Err(e) => c.load_error = Some(e),
        }
    }

    fn set_concept_status(&mut self, status: &'static str, cx: &mut Context<Self>) {
        self.concepts.status = status;
        self.reload_concepts();
        cx.notify();
    }

    fn toggle_concept_tag(&mut self, name: String, cx: &mut Context<Self>) {
        let tags = &mut self.concepts.active_tags;
        if let Some(i) = tags.iter().position(|t| *t == name) {
            tags.remove(i);
        } else {
            tags.push(name);
        }
        self.reload_concepts();
        cx.notify();
    }

    /// 머리 줄의 개념 탭 몫 — 검색 칸(최대 440), 빈칸, [+ 추가]
    pub(crate) fn concepts_topbar(&self, k: &Tokens, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            div()
                .relative()
                .flex_1()
                .max_w(px(440.))
                .h(px(36.))
                .flex()
                .items_center()
                .pl(px(34.))
                .pr(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(k.border)
                .bg(k.surface_2)
                .text_size(px(13.33))
                .tag("concept-search")
                .child(
                    div()
                        .absolute()
                        .left(px(11.))
                        .child(icon("search", 15.).text_color(k.text_3)),
                )
                .child(
                    div().flex_1().child(
                        Input::new(&self.concepts.search_input)
                            .appearance(false)
                            .px_0(),
                    ),
                )
                .into_any_element(),
            div().flex_1().into_any_element(),
            primary_button("concept-add", k, false)
                .h(px(34.))
                .px(px(14.))
                .text_size(px(13.))
                .child(icon("plus", 15.).text_color(k.text))
                .child(t("app.add"))
                .into_any_element(),
        ]
    }

    /// 탭 줄 + 태그 줄 + 목록 | 상세
    pub(crate) fn concepts_body(&self, body_w: f32, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let c = &self.concepts;
        let width = crate::pane::effective(c.width, LIST_PANE.min, body_w);
        let count = |id: &str| match id {
            "learning" => c.counts.0,
            "learned" => c.counts.1,
            _ => c.counts.2,
        };
        let seg = div()
            .flex()
            .gap(px(2.))
            .p(px(3.))
            .bg(k.surface_2)
            .border_1()
            .border_color(k.border)
            .rounded(px(7.))
            .children(TABS.iter().map(|&(id, label)| {
                let active = c.status == id;
                div()
                    .id(SharedString::from(format!("concept-tab-{id}")))
                    .tag(format!("concept-tab-{id}"))
                    .flex()
                    .items_center()
                    .py(px(5.))
                    .px(px(12.))
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .when(active, |d| {
                        d.bg(k.glass_raise)
                            .shadow({
                                let mut s = k.glass_shadow(false);
                                s.truncate(1);
                                s.push(BoxShadow {
                                    color: k.shadow_sm,
                                    offset: point(px(0.), px(1.)),
                                    blur_radius: px(2.),
                                    spread_radius: px(0.),
                                    inset: false,
                                });
                                s
                            })
                            .text_color(k.text)
                    })
                    .when(!active, |d| {
                        let text = k.text;
                        d.text_color(k.text_2).hover(move |s| s.text_color(text))
                    })
                    .child(t(label))
                    .child(
                        div()
                            .ml(px(5.))
                            .text_color(k.text_2)
                            .child(count(id).to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.concept_guard(cx, move |s, cx| s.set_concept_status(id, cx))
                    }))
            }));
        let sort_label = SORTS
            .iter()
            .find(|(id, _)| *id == c.sort())
            .map(|(_, l)| t(l))
            .unwrap_or_default();
        let anchor = c.sort_anchor.clone();
        let (hover_fill, hover_edge) = (glass(k.glass_fill_hover), k.glass_edge_hover);
        let sort = div()
            .id("concept-sort")
            .tag("concept-sort")
            .relative()
            .h(px(34.))
            .pl(px(12.))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(k.glass_edge)
            .bg(glass(k.glass_fill))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_fill).border_color(hover_edge))
            .child(sort_label)
            .child(icon("chevron-down", 14.).text_color(k.text_3))
            .child(
                canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.concepts.sort_open = !this.concepts.sort_open;
                cx.notify();
            }));
        let tabsbar = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pt(px(6.))
            .px(px(14.))
            .pb(px(2.))
            .child(seg)
            .child(div().flex_1())
            .child(sort);
        let tagbar =
            (!c.tags.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .pt(px(6.))
                    .px(px(14.))
                    .children(c.tags.iter().take(12).map(|(name, _)| {
                        let on = c.active_tags.contains(name);
                        let n = name.clone();
                        tag_chip(k, name, on)
                            .id(SharedString::from(format!("concept-tag-{name}")))
                            .tag(format!("concept-tag-{name}"))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_concept_tag(n.clone(), cx)
                            }))
                    }))
                    .when(!c.active_tags.is_empty(), |d| {
                        d.child(
                            chip(k)
                                .id("concept-tags-clear")
                                .tag("concept-tags-clear")
                                .cursor_pointer()
                                .child(t("app.filter.clear"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.concepts.active_tags.clear();
                                    this.reload_concepts();
                                    cx.notify();
                                })),
                        )
                    })
            });
        let card = |d: Div| {
            d.border_1()
                .border_color(k.glass_edge)
                .rounded(px(12.))
                .bg(glass(k.glass_fill))
                .shadow(k.glass_shadow(false))
        };
        let mut list = div()
            .id("concept-list")
            .tag("concept-list")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&c.list_scroll)
            .flex()
            .flex_col();
        if let Some(e) = c.load_error.clone() {
            list = list.child(
                crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                    .m(px(12.))
                    .child(e),
            );
        }
        if c.ready && c.list.is_empty() && c.load_error.is_none() {
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .p(px(40.))
                    .text_size(px(14.))
                    .text_color(k.text_3)
                    .child(t(if c.status == "learning" {
                        "app.empty.learning"
                    } else {
                        "app.empty.noResults"
                    }))
                    .child(
                        primary_button("concept-add-first", k, false)
                            .child(icon("plus", 14.).text_color(k.text))
                            .child(t("app.empty.addFirst")),
                    ),
            );
        }
        for r in &c.list {
            list = list.child(self.concept_row(r, k, cx));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(tabsbar)
            .children(tagbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .relative()
                            .w(px(width))
                            .flex_none()
                            .h_full()
                            .pt(px(8.))
                            .pb(px(8.))
                            .pl(px(8.))
                            .pr(px(4.))
                            .child(card(div().size_full().overflow_hidden()).child(list))
                            .child(self.pane_resizer(LIST_PANE, width, body_w, k, cx)),
                    )
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
                                card(div().size_full().overflow_hidden())
                                    .child(self.concept_detail_view(k, cx)),
                            ),
                    ),
            )
    }

    /// 목록 한 줄 — 제목과 확신 점, 요약 한 줄, 상태 배지와 태그 셋
    fn concept_row(&self, r: &Row, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = row_i64(r, "id");
        let selected = self.concepts.selected == Some(id);
        let confidence = row_i64(r, "confidence");
        let tags: Vec<String> = r
            .get("tags")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let hover = glass(k.glass_fill);
        div()
            .id(SharedString::from(format!("concept-row-{id}")))
            .tag(format!("concept-row-{id}"))
            .flex()
            .flex_col()
            .gap(px(5.))
            .py(px(13.))
            .px(px(18.))
            .border_b_1()
            .border_color(k.glass_edge)
            .cursor_pointer()
            .when(selected, |d| {
                d.bg(glass(k.glass_fill_accent)).shadow(vec![BoxShadow {
                    color: k.text,
                    offset: point(px(2.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                }])
            })
            .when(!selected, |d| d.hover(move |s| s.bg(hover)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(row_str(r, "title").to_string()),
                    )
                    .child(confidence_dots(k, confidence)),
            )
            .child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.))
                    .text_color(k.text_2)
                    .child(row_str(r, "summary").to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(status_badge(k, row_str(r, "status")))
                    .children(tags.iter().take(3).map(|t| chip(k).child(format!("#{t}")))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.concept_guard(cx, move |s, cx| {
                    s.concepts.selected = Some(id);
                    cx.notify();
                })
            }))
    }

    /// 초안이 있으면 확인 창으로 미루고, 없으면 그대로(지금 앱 App.tsx 의 guard)
    pub(crate) fn concept_guard(
        &mut self,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut Spike, &mut Context<Spike>) + 'static,
    ) {
        if self.concepts.detail.as_ref().is_some_and(|d| d.dirty) {
            self.concepts.pending = Some(Box::new(run));
            cx.notify();
        } else {
            run(self, cx);
        }
    }

    pub(crate) fn concept_unsaved_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if self.section != crate::Section::Til || self.concepts.pending.is_none() {
            return None;
        }
        let keep = || {
            cx.listener(|this, _, _, cx| {
                this.concepts.pending = None;
                cx.notify();
            })
        };
        Some(crate::dialog::shell(
            k,
            440.,
            t("common.unsaved.title"),
            div().child(t("common.unsaved.body")).into_any_element(),
            vec![
                crate::day_card::button("concept-unsaved-keep", k, false, false)
                    .child(t("common.unsaved.keep"))
                    .on_click(keep()),
                crate::day_card::danger_button("concept-unsaved-discard", k)
                    .child(t("common.unsaved.discard"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(d) = this.concepts.detail.as_mut() {
                            d.edit = None;
                            d.dirty = false;
                        }
                        if let Some(run) = this.concepts.pending.take() {
                            run(this, cx);
                        }
                        cx.notify();
                    })),
            ],
            keep(),
        ))
    }

    pub(crate) fn concept_sort_menu(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Deferred> {
        if self.section != crate::Section::Til || !self.concepts.sort_open {
            return None;
        }
        let cur = self.concepts.sort();
        let items = SORTS
            .iter()
            .map(|(id, label)| MenuItem {
                label: t(label),
                checked: *id == cur,
                ..Default::default()
            })
            .collect();
        let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
        Some(select_menu(
            "concept-sort-menu",
            self.concepts.sort_anchor.get(),
            items,
            k,
            move |i, _, cx| {
                let _ = pick.update(cx, |this, cx| {
                    let c = &mut this.concepts;
                    c.sort_open = false;
                    c.sort_by_tab.insert(c.status, SORTS[i].0);
                    this.reload_concepts();
                    cx.notify();
                });
            },
            move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    this.concepts.sort_open = false;
                    cx.notify();
                });
            },
        ))
    }
}

/// .chip — 유리 칩
pub(crate) fn chip(k: &Tokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .whitespace_nowrap()
        .text_size(px(12.))
        .text_color(k.text_2)
        .border_1()
        .border_color(k.glass_edge)
        .rounded(px(6.))
        .px(px(9.))
        .py(px(3.))
        .bg(glass(k.glass_fill))
        .shadow(k.glass_shadow(false))
}

/// 태그 칩 — 고른 것은 강조 유리
fn tag_chip(k: &Tokens, name: &str, on: bool) -> Div {
    let c = chip(k).child(format!("#{name}"));
    if on {
        c.bg(glass(k.glass_fill_accent))
            .border_color(k.glass_edge_accent)
            .text_color(k.text)
    } else {
        c
    }
}

/// 확신 점 셋 — 찬 만큼 글자색으로 채운다
pub(crate) fn confidence_dots(k: &Tokens, value: i64) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.))
        .children((1..=3).map(|i| {
            let on = i <= value;
            div()
                .size(px(7.))
                .rounded_full()
                .border_1()
                .border_color(if on { k.text } else { k.border_strong })
                .when(on, |d| d.bg(k.text))
        }))
}

/// .badge — 상태(학습 중 = 아웃라인, 배움 = 채움)
pub(crate) fn status_badge(k: &Tokens, status: &str) -> Div {
    let learned = status == "learned";
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .py(px(2.))
        .px(px(8.))
        .rounded(px(6.))
        .when(learned, |d| d.bg(k.primary_bg).text_color(k.primary_fg))
        .when(!learned, |d| {
            d.border_1()
                .border_color(k.border_strong)
                .text_color(k.text_2)
        })
        .child(
            t(if learned {
                "common.status.learned"
            } else {
                "common.status.learning"
            })
            .to_uppercase(),
        )
}
