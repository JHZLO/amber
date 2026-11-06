// 빠른 검색(⌘K, ⌘P) — 지금 앱 SearchModal.tsx. 개념(제목, 요약, 태그), 노트와 다이어그램 파일(이름 + 본문)을 한 자리에서.
// 종류마다 8개까지(한 종류가 목록을 독점하지 않게), 200ms 늦춰 찾는다. ↑↓ 로 고르고 Enter 로 연다.
// 결과 칸 높이는 고정 — 치는 동안 창이 커졌다 작아지지 않게.

use std::time::Duration;

use amber_gpui::concepts::{self, ConceptFilter};
use amber_gpui::i18n::t;
use amber_gpui::vault;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::assets::icon;
use crate::day_card::Tag;
use crate::settings_view::glass;
use crate::theme::Tokens;
use crate::{Section, Spike};

const PER_KIND: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Concept(i64),
    Note(String),
    Diagram(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub kind: Kind,
    pub title: String,
    pub sub: String,
    pub r#where: String,
}

pub struct QuickSearch {
    pub input: Entity<InputState>,
    pub hits: Vec<Hit>,
    pub idx: usize,
    pub searching: bool,
    pub scroll: ScrollHandle,
    debounce: Option<Task<()>>,
}

/// 찾기 — 개념, 노트, 다이어그램 순. 루트가 사라졌거나 실패한 종류는 조용히 비운다
pub fn run_search(db: &amber_gpui::db::Db, q: &str) -> Vec<Hit> {
    let concepts = concepts::list_concepts(
        db,
        &ConceptFilter {
            status: "all".into(),
            search: Some(q.to_string()),
            tags: None,
            sort: Some("recent_updated".into()),
            limit: Some(PER_KIND as f64),
        },
    )
    .unwrap_or_default();
    let mut out: Vec<Hit> = concepts
        .iter()
        .map(|c| Hit {
            kind: Kind::Concept(c.get("id").and_then(Json::as_i64).unwrap_or_default()),
            title: c
                .get("title")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            sub: c
                .get("summary")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            r#where: crate::concept_detail::tags_of(c)
                .iter()
                .take(3)
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" "),
        })
        .collect();
    let files = |tree: vault::Tree, note: bool| -> Vec<Hit> {
        tree.search(q, PER_KIND)
            .unwrap_or_default()
            .into_iter()
            .map(|m| Hit {
                kind: if note {
                    Kind::Note(m.path.clone())
                } else {
                    Kind::Diagram(m.path.clone())
                },
                title: m.name,
                sub: m.snippet.unwrap_or_default(),
                r#where: vault::parent_of(&m.path).to_string(),
            })
            .collect()
    };
    out.extend(files(vault::notes(db), true));
    out.extend(files(vault::Tree::new(db, &vault::DIAGRAMS), false));
    out
}

impl Spike {
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("app.quicksearch.placeholder")));
        cx.subscribe_in(
            &input,
            window,
            |this, s, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    let q = vault::js_trim(&s.read(cx).value()).to_string();
                    let Some(qs) = this.search.as_mut() else {
                        return;
                    };
                    if q.is_empty() {
                        qs.hits.clear();
                        qs.idx = 0;
                        qs.searching = false;
                        qs.debounce = None;
                        cx.notify();
                        return;
                    }
                    qs.searching = true;
                    qs.debounce = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(200))
                            .await;
                        let _ = this.update(cx, |s, cx| {
                            let hits = run_search(&s.db, &q);
                            if let Some(qs) = s.search.as_mut() {
                                qs.hits = hits;
                                qs.idx = 0;
                                qs.searching = false;
                            }
                            cx.notify();
                        });
                    }));
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    let hit = this
                        .search
                        .as_ref()
                        .and_then(|qs| qs.hits.get(qs.idx).cloned());
                    if let Some(h) = hit {
                        this.open_hit(h, window, cx);
                    }
                }
                _ => {}
            },
        )
        .detach();
        input.update(cx, |st, cx| st.focus(window, cx));
        self.search = Some(QuickSearch {
            input,
            hits: Vec::new(),
            idx: 0,
            searching: false,
            scroll: ScrollHandle::new(),
            debounce: None,
        });
        cx.notify();
    }

    /// 고른 결과를 연다 — 탭을 바꾸고 그 개념/노트/다이어그램으로
    /// 닫는다 — 사라진 입력칸이 포커스를 쥐고 있으면 다음 키가 뿌리에 안 닿는다
    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn open_hit(&mut self, hit: Hit, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search(window, cx);
        match hit.kind {
            Kind::Concept(id) => self.go_to_concept(id, window, cx),
            Kind::Note(path) => {
                self.switch_section(Section::Notes, cx);
                self.request_open_note(&path, cx);
            }
            Kind::Diagram(path) => {
                self.switch_section(Section::Diagrams, cx);
                self.request_open_diagram(&path, cx);
            }
        }
        cx.notify();
    }

    fn search_move(&mut self, delta: i64) {
        if let Some(qs) = self.search.as_mut()
            && !qs.hits.is_empty()
        {
            let n = qs.hits.len() as i64;
            qs.idx = ((qs.idx as i64 + delta + n) % n) as usize;
            // 고른 줄이 보이게(키보드 이동이라 애니메이션 없이)
            qs.scroll.scroll_to_item(qs.idx);
        }
    }

    pub(crate) fn search_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let qs = self.search.as_ref()?;
        let query = qs.input.read(cx).value().to_string();
        let mut list = div()
            .id("search-list")
            .tag("search-list")
            .h(px(320.))
            .mt(px(14.))
            .mx(px(-20.))
            .mb(px(-20.))
            .overflow_y_scroll()
            .track_scroll(&qs.scroll)
            .flex()
            .flex_col();
        let hover = glass(k.glass_fill);
        for (i, h) in qs.hits.iter().enumerate() {
            let selected = i == qs.idx;
            let kind = match h.kind {
                Kind::Concept(_) => t("app.quicksearch.kind.concept"),
                Kind::Note(_) => t("app.quicksearch.kind.note"),
                Kind::Diagram(_) => t("app.quicksearch.kind.diagram"),
            };
            let hit = h.clone();
            let hover = hover.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("search-hit-{i}")))
                    .tag(format!("search-hit-{i}"))
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
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(h.title.clone()),
                    )
                    .when(!h.sub.is_empty(), |d| {
                        d.child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(13.))
                                .text_color(k.text_2)
                                .child(h.sub.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(crate::concepts_view::chip(k).child(kind))
                            .when(!h.r#where.is_empty(), |d| d.child(h.r#where.clone())),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_hit(hit.clone(), window, cx)
                    })),
            );
        }
        if qs.hits.is_empty() {
            list = list.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(k.text_3)
                    .child(t(if vault::js_trim(&query).is_empty() {
                        "app.quicksearch.emptyPrompt"
                    } else if qs.searching {
                        "app.quicksearch.searching"
                    } else {
                        "app.empty.noResults"
                    })),
            );
        }
        let field = div()
            .relative()
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
            .tag("search-input")
            .child(
                div()
                    .absolute()
                    .left(px(11.))
                    .child(icon("search", 15.).text_color(k.text_3)),
            )
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&qs.input).appearance(false).px_0()),
            );
        let body = div()
            .flex()
            .flex_col()
            // ↑↓ 는 입력칸보다 먼저 받는다(한 줄 입력칸은 커서를 처음/끝으로 옮긴다)
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let d = match e.keystroke.key.as_str() {
                    "down" => 1,
                    "up" => -1,
                    _ => return,
                };
                this.search_move(d);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(field)
            .child(list);
        Some(crate::dialog::shell_split(
            k,
            640.,
            t("app.quicksearch.title"),
            body.into_any_element(),
            vec![
                div()
                    .id("search-hint")
                    .flex()
                    .items_center()
                    .text_color(k.text_3)
                    .child(t("app.quicksearch.hint")),
            ],
            Vec::new(),
            cx.listener(|this, _, window, cx| this.close_search(window, cx)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, run_search};
    use amber_gpui::db::{Clock, Db};

    #[test]
    fn concepts_come_first_then_notes_then_diagrams() {
        let dir = std::env::temp_dir().join(format!("amber-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("vault/notes/Eng")).unwrap();
        std::fs::create_dir_all(dir.join("vault/diagrams")).unwrap();
        std::fs::write(
            dir.join("vault/notes/Eng/Log.md"),
            "# Log\n\nwrite-ahead log\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("vault/diagrams/log.mmd"),
            "flowchart LR\n  log --> disk\n",
        )
        .unwrap();
        let db = Db::open(dir.join("amber.db").to_str().unwrap(), Clock::System).unwrap();
        amber_gpui::migrate::run(&db).unwrap();
        db.execute(
            "INSERT INTO concepts (ulid, title, summary, detail_path) VALUES ('01J', 'Write-ahead log', 's', 'concepts/01J/index.md')",
            &[],
        )
        .unwrap();
        let hits = run_search(&db, "log");
        let kinds: Vec<&str> = hits
            .iter()
            .map(|h| match h.kind {
                Kind::Concept(_) => "c",
                Kind::Note(_) => "n",
                Kind::Diagram(_) => "d",
            })
            .collect();
        assert_eq!(kinds, ["c", "n", "d"]);
        assert_eq!(hits[1].r#where, "Eng");
    }
}
