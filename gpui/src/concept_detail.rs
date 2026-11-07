// 개념 상세 — 지금 앱 ConceptDetail.tsx. 제목과 확신 점, 상태 배지, 요약, 태그, 동작 줄(상태 바꾸기, 확신 ±, 편집,
// AI 보강 | 지우기), 본문(개념 노트 파일, 노트와 같은 마크다운), 출처 노트, 메타 한 줄.
// 편집은 제목, 요약, 태그(쉼표), 본문을 한꺼번에 — 파일을 먼저 쓰고 DB 를 나중에(실패해도 DB 가 파일을 앞지르지 않게).
// 읽기 실패를 본문으로 위장하지 않는다(그 글이 초안이 되면 원본이 날아간다). 파일만 사라졌으면 편집해서 되살릴 수 있다.

use amber_gpui::concepts;
use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{Note, field_label, note_box};
use crate::notes_view::Block;
use crate::theme::Tokens;
use crate::tip::tip;

pub struct ConceptEdit {
    pub title: Entity<InputState>,
    pub summary: Entity<TextareaState>,
    pub tags: Entity<InputState>,
    pub body: Entity<TextareaState>,
}

/// 고른 개념의 본문과 화면 상태 — 다른 개념을 고르면 새로 만든다
pub struct DetailState {
    pub id: i64,
    pub body: String,
    pub blocks: Vec<Block>,
    pub read_error: Option<String>,
    pub missing: bool,
    pub write_error: Option<String>,
    pub edit: Option<ConceptEdit>,
    pub busy: bool,
    pub confirm_delete: bool,
    /// 편집 중 고친 게 있나 — 목록 이동을 막는 확인 창이 본다
    pub dirty: bool,
}

/// 쉼표나 줄바꿈으로 나누고 앞의 # 를 뗀다
pub fn parse_tags(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(|t| {
            let t = vault::js_trim(t);
            t.strip_prefix('#').unwrap_or(t).to_string()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// 노트에서 올린 개념이면 출처(root, noteRel, anchor). root 는 v0.17.11 부터 — 없는 옛 행은 None
pub fn source_note(r: &Row) -> Option<(Option<String>, String, String)> {
    if r.get("source_kind").and_then(Json::as_str) != Some("file") {
        return None;
    }
    let d: Json = serde_json::from_str(r.get("source")?.as_str()?).ok()?;
    let note = d["noteRel"].as_str()?.to_string();
    let anchor = match &d["anchor"] {
        Json::Null => String::new(),
        Json::String(s) => s.clone(),
        v => v.to_string(),
    };
    Some((d["root"].as_str().map(str::to_string), note, anchor))
}

fn s<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).and_then(Json::as_str).unwrap_or_default()
}

fn n(r: &Row, k: &str) -> i64 {
    r.get(k).and_then(Json::as_i64).unwrap_or_default()
}

pub fn tags_of(r: &Row) -> Vec<String> {
    r.get("tags")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

impl Spike {
    pub(crate) fn selected_concept(&self) -> Option<&Row> {
        let id = self.concepts.selected?;
        self.concepts.list.iter().find(|r| n(r, "id") == id)
    }

    /// 그리기 전에 — 고른 개념이 바뀌었으면 본문을 읽는다
    pub(crate) fn prepare_concept_detail(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept() else {
            self.concepts.detail = None;
            return;
        };
        let id = n(r, "id");
        if self.concepts.detail.as_ref().is_some_and(|d| d.id == id) {
            return;
        }
        let path = s(r, "detail_path").to_string();
        let (body, read_error, missing) = match vault::read_concept_note(&self.db, &path) {
            Ok(b) => (b, None, false),
            Err(e) => {
                if vault::concept_note_exists(&self.db, &path) {
                    (String::new(), Some(e), false)
                } else {
                    (String::new(), None, true)
                }
            }
        };
        let text = crate::hex(Tokens::get(cx).text);
        let blocks = crate::notes_view::build_blocks(&body, &text, cx).0;
        self.concepts.detail = Some(DetailState {
            id,
            body,
            blocks,
            read_error,
            missing,
            write_error: None,
            edit: None,
            busy: false,
            confirm_delete: false,
            dirty: false,
        });
    }

    fn concept_changed(&mut self, cx: &mut Context<Self>) {
        self.reload_concepts();
        cx.notify();
    }

    fn concept_toggle_status(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept() else {
            return;
        };
        let (id, next) = (
            n(r, "id"),
            if s(r, "status") == "learning" {
                "learned"
            } else {
                "learning"
            },
        );
        if let Err(e) = concepts::set_status(&self.db, id, next) {
            self.set_detail_error(e.to_string());
        }
        self.concept_changed(cx);
    }

    fn concept_confidence(&mut self, delta: f64, cx: &mut Context<Self>) {
        let Some(id) = self.concepts.selected else {
            return;
        };
        if let Err(e) = concepts::adjust_confidence(&self.db, id, delta) {
            self.set_detail_error(e.to_string());
        }
        self.concept_changed(cx);
    }

    fn set_detail_error(&mut self, e: String) {
        if let Some(d) = self.concepts.detail.as_mut() {
            d.write_error = Some(e);
        }
    }

    fn concept_start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let Some(body) = self.concepts.detail.as_ref().map(|d| d.body.clone()) else {
            return;
        };
        let input = |v: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut s = InputState::new(window, cx);
                s.set_value(v, window, cx);
                s
            })
        };
        let area = |v: String, rows: usize, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut s = TextareaState::new(window, cx).auto_grow(rows, rows);
                s.set_value(v, window, cx);
                s
            })
        };
        let edit = ConceptEdit {
            title: input(s(&r, "title").to_string(), window, cx),
            summary: area(s(&r, "summary").to_string(), 2, window, cx),
            tags: input(tags_of(&r).join(", "), window, cx),
            body: area(body, 18, window, cx),
        };
        // 고친 게 있나 — 처음 값과 견준다
        let base = (
            s(&r, "title").to_string(),
            s(&r, "summary").to_string(),
            tags_of(&r).join(", "),
            self.concepts
                .detail
                .as_ref()
                .map(|d| d.body.clone())
                .unwrap_or_default(),
        );
        let watch = move |this: &mut Spike, cx: &mut Context<Spike>| {
            let Some(d) = this.concepts.detail.as_mut() else {
                return;
            };
            let Some(e) = d.edit.as_ref() else {
                return;
            };
            let now = (
                e.title.read(cx).value().to_string(),
                e.summary.read(cx).value().to_string(),
                e.tags.read(cx).value().to_string(),
                e.body.read(cx).value().to_string(),
            );
            let dirty = now != base;
            if d.dirty != dirty {
                d.dirty = dirty;
                cx.notify();
            }
        };
        let watch = std::rc::Rc::new(watch);
        for e in [&edit.title, &edit.tags] {
            let w = watch.clone();
            cx.subscribe_in(e, window, move |this, _, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    w(this, cx);
                }
            })
            .detach();
        }
        for e in [&edit.summary, &edit.body] {
            let w = watch.clone();
            cx.subscribe_in(e, window, move |this, _, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    w(this, cx);
                }
            })
            .detach();
        }
        edit.title.update(cx, |s, cx| s.focus(window, cx));
        if let Some(d) = self.concepts.detail.as_mut() {
            d.edit = Some(edit);
            d.write_error = None;
            d.dirty = false;
        }
        cx.notify();
    }

    fn concept_cancel_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(d) = self.concepts.detail.as_mut() {
            d.edit = None;
            d.dirty = false;
        }
        self.sync_concept_search(cx);
        cx.notify();
    }

    /// 편집 중 미뤄 둔 검색어를 반영한다
    pub(crate) fn sync_concept_search(&mut self, cx: &mut Context<Self>) {
        let v = self.concepts.search_input.read(cx).value().to_string();
        if v != self.concepts.search {
            self.concepts.search = v;
            self.reload_concepts();
        }
    }

    /// Esc — 안쪽 창부터 닫는다. 닫은 게 있으면 true
    pub(crate) fn concept_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.add_concept_escape(window, cx) {
            return true;
        }
        if let Some(a) = self.concepts.augment.as_mut() {
            if a.confirm_close {
                a.confirm_close = false;
            } else {
                self.esc_close("augment", window, cx, |s, _, cx| {
                    s.augment_close_request(cx)
                });
            }
            return true;
        }
        if self.concepts.pending.take().is_some() {
            return true;
        }
        if let Some(d) = self.concepts.detail.as_mut()
            && d.confirm_delete
        {
            d.confirm_delete = false;
            return true;
        }
        false
    }

    /// 저장 — 파일 먼저, DB 나중
    pub(crate) fn concept_save(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let Some(d) = self.concepts.detail.as_ref() else {
            return;
        };
        if d.busy || d.read_error.is_some() {
            return;
        }
        let Some(e) = d.edit.as_ref() else {
            return;
        };
        let (title, summary, tags, body) = (
            e.title.read(cx).value().to_string(),
            e.summary.read(cx).value().to_string(),
            e.tags.read(cx).value().to_string(),
            e.body.read(cx).value().to_string(),
        );
        let id = n(&r, "id");
        let res = (|| -> Result<(), String> {
            vault::write_concept_note(&self.db, s(&r, "ulid"), &body)?;
            concepts::update_concept_content(
                &self.db,
                id,
                Some(vault::js_trim(&title)),
                Some(vault::js_trim(&summary)),
            )
            .map_err(|e| e.to_string())?;
            concepts::set_concept_tags(&self.db, id, &parse_tags(&tags)).map_err(|e| e.to_string())
        })();
        let text = crate::hex(Tokens::get(cx).text);
        match res {
            Ok(()) => {
                let blocks = crate::notes_view::build_blocks(&body, &text, cx).0;
                if let Some(d) = self.concepts.detail.as_mut() {
                    d.body = body;
                    d.blocks = blocks;
                    d.missing = false;
                    d.edit = None;
                    d.dirty = false;
                    d.write_error = None;
                }
                self.sync_concept_search(cx);
                self.concept_changed(cx);
            }
            Err(e) => {
                self.set_detail_error(e);
                cx.notify();
            }
        }
    }

    /// 지우기 — 노트 쪽 역참조부터(실패해도 개념 삭제는 막지 않는다), 파일 먼저, DB 나중
    fn concept_delete(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let id = n(&r, "id");
        if let Some((_, note, _)) = source_note(&r) {
            let _ = amber_gpui::note_concepts::remove_note_concept(&self.db, &note, id as f64);
        }
        let res = vault::delete_concept_dir(&self.db, s(&r, "ulid"))
            .and_then(|_| concepts::delete_concept(&self.db, id).map_err(|e| e.to_string()));
        match res {
            Ok(()) => {
                self.concepts.selected = None;
                self.concepts.detail = None;
            }
            Err(e) => {
                if let Some(d) = self.concepts.detail.as_mut() {
                    d.write_error = Some(e);
                    d.confirm_delete = false;
                }
            }
        }
        self.concept_changed(cx);
    }

    /// 오른쪽 카드 — 고른 개념 또는 빈 안내
    pub(crate) fn concept_detail_view(&self, k: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let (Some(r), Some(d)) = (self.selected_concept(), self.concepts.detail.as_ref()) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.))
                .text_color(k.text_3)
                .child(t("app.empty.selectConcept"))
                .into_any_element();
        };
        let scroll = div()
            .id("concept-detail")
            .tag("concept-detail")
            .size_full()
            .overflow_y_scroll()
            .py(px(32.))
            .px(px(40.))
            .flex()
            .flex_col();
        if let Some(e) = d.edit.as_ref() {
            return scroll
                .child(self.concept_edit_form(d, e, k, cx))
                .into_any_element();
        }
        let confidence = n(r, "confidence");
        let learning = s(r, "status") == "learning";
        let busy = d.busy;
        let tags = tags_of(r);
        let can_edit = d.read_error.is_none();
        let connected = crate::note_ai::ai_args(&self.db).is_some();
        let can_augment = can_edit && !d.missing && connected;
        let toggle = if learning {
            primary_button("concept-status", k, busy)
                .child(icon("check", 14.).text_color(k.text))
                .child(t("concepts.action.markLearned"))
        } else {
            button("concept-status", k, false, busy)
                .child(icon("undo", 14.).text_color(k.text_2))
                .child(t("concepts.action.backToLearning"))
        }
        .on_click(cx.listener(|this, _, _, cx| this.concept_toggle_status(cx)));
        let conf = |id: &'static str,
                    sign: &'static str,
                    delta: f64,
                    off: bool,
                    cx: &mut Context<Self>| {
            button(id, k, false, off)
                .child(t("concepts.field.confidence"))
                .child(icon(sign, 13.).text_color(k.text_2))
                .when(!off, |b| {
                    b.on_click(
                        cx.listener(move |this, _, _, cx| this.concept_confidence(delta, cx)),
                    )
                })
        };
        let actions =
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .py(px(12.))
                .mb(px(20.))
                .border_t_1()
                .border_color(k.border)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(toggle)
                        .child(conf(
                            "concept-conf-up",
                            "plus",
                            1.,
                            busy || confidence >= 3,
                            cx,
                        ))
                        .child(conf(
                            "concept-conf-down",
                            "minus",
                            -1.,
                            busy || confidence <= 1,
                            cx,
                        ))
                        .child(
                            button("concept-edit", k, false, busy || !can_edit)
                                .child(icon("pencil", 14.).text_color(k.text_2))
                                .child(t("concepts.action.edit"))
                                .when(!busy && can_edit, |b| {
                                    b.on_click(cx.listener(|this, _, window, cx| {
                                        this.concept_start_edit(window, cx)
                                    }))
                                }),
                        )
                        .child(
                            button("concept-augment", k, false, busy || !can_augment)
                                .tooltip(tip(t("concepts.action.augmentTitle")))
                                .child(icon("sparkles", 14.).text_color(k.text_2))
                                .child(t("concepts.action.augment"))
                                .when(!busy && can_augment, |b| {
                                    b.on_click(cx.listener(|this, _, window, cx| {
                                        this.open_augment(window, cx)
                                    }))
                                }),
                        ),
                )
                .child(
                    danger_button("concept-delete", k)
                        .h(px(28.))
                        .child(icon("trash", 14.).text_color(k.danger))
                        .child(t("common.delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(d) = this.concepts.detail.as_mut() {
                                d.confirm_delete = true;
                            }
                            cx.notify();
                        })),
                );
        let on_link = open_link();
        let mut body = div().tag("concept-body").flex().flex_col();
        if let Some(e) = d.read_error.clone() {
            body = body.child(
                note_box(Note::Error, k).child(t_with("concepts.readError", &[("err", &e)])),
            );
        } else if d.missing {
            body = body.child(
                div()
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(t("concepts.missingFile")),
            );
        } else {
            for (i, b) in d.blocks.iter().enumerate() {
                body = body.child(match b {
                    Block::Text(state) => {
                        crate::markdown::view(state, on_link.clone(), k, None).into_any_element()
                    }
                    other => self.note_block(other, 4_000_000 + i as u64, &on_link, k, cx),
                });
            }
        }
        let source = source_note(r).map(|(root, note, anchor)| {
            let cur = vault::notes(&self.db).full("");
            let other = root.as_ref().is_some_and(|r| *r != cur);
            let name = match (&root, other) {
                (Some(root), true) => {
                    let label = if root == "vault/notes" {
                        t("settings.root.default")
                    } else {
                        root.split('/')
                            .filter(|s| !s.is_empty())
                            .next_back()
                            .unwrap_or(root)
                            .to_string()
                    };
                    format!("{label} / {note}")
                }
                _ => note.clone(),
            };
            let open = note.clone();
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .mt(px(20.))
                .child(
                    button("concept-source", k, false, other)
                        .tooltip(tip(if other {
                            t("concepts.sourceNote.otherRoot")
                        } else {
                            anchor
                        }))
                        .child(icon("book", 13.).text_color(k.text_2))
                        .child(t("concepts.sourceNote.open"))
                        .when(!other, |b| {
                            b.on_click(cx.listener(move |this, _, _, cx| {
                                this.section = crate::Section::Notes;
                                amber_gpui::prefs::set(&this.db, "til.section", "notes");
                                this.reload_notes();
                                this.open_note(&open, cx);
                                cx.notify();
                            }))
                        }),
                )
                .child(div().text_size(px(12.)).text_color(k.text_3).child(name))
        });
        let now = chrono::Local::now().timestamp_millis();
        let meta = t_with(
            "concepts.meta",
            &[
                (
                    "created",
                    &crate::notes_view::locale_date(n(r, "created_at")),
                ),
                (
                    "updated",
                    &crate::notes_view::time_ago(n(r, "updated_at"), now),
                ),
                ("seen", &n(r, "seen_count").to_string()),
            ],
        );
        scroll
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .mb(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(23.))
                            .line_height(px(29.9))
                            .font_weight(FontWeight::BOLD)
                            .child(s(r, "title").to_string()),
                    )
                    .child(crate::concepts_view::confidence_dots(k, confidence)),
            )
            .child(
                div()
                    .mb(px(8.))
                    .flex()
                    .child(crate::concepts_view::status_badge(k, s(r, "status"))),
            )
            .child(
                div()
                    .mt(px(4.))
                    .mb(px(14.))
                    .text_size(px(14.))
                    .line_height(px(22.4))
                    .text_color(k.text_2)
                    .child(s(r, "summary").to_string()),
            )
            .when(!tags.is_empty(), |el| {
                el.child(
                    div().flex().flex_wrap().gap(px(6.)).mb(px(14.)).children(
                        tags.iter()
                            .map(|t| crate::concepts_view::chip(k).child(format!("#{t}"))),
                    ),
                )
            })
            .child(actions)
            .when_some(d.write_error.clone(), |el, e| {
                el.child(
                    note_box(Note::Error, k)
                        .mb(px(12.))
                        .child(t_with("concepts.writeError", &[("err", &e)])),
                )
            })
            .child(body)
            .children(source)
            .child(
                div()
                    .mt(px(26.))
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(k.border)
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(meta),
            )
            .into_any_element()
    }

    fn concept_edit_form(
        &self,
        d: &DetailState,
        e: &ConceptEdit,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let field = |label: String, control: AnyElement| {
            div()
                .flex()
                .flex_col()
                .mb(px(16.))
                .child(field_label(label, k))
                .child(control)
        };
        let boxed = |d: Div| {
            d.max_w(px(440.))
                .rounded(px(6.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
        };
        let busy = d.busy;
        div()
            .flex()
            .flex_col()
            .child(field(
                t("concepts.field.title"),
                boxed(div().px(px(11.)).py(px(4.)).text_size(px(14.)))
                    .tag("concept-edit-title")
                    .child(Input::new(&e.title).appearance(false).px_0())
                    .into_any_element(),
            ))
            .child(field(
                t("concepts.field.summary"),
                boxed(div().px(px(3.)).py(px(4.)))
                    .tag("concept-edit-summary")
                    .child(
                        Textarea::new(&e.summary)
                            .appearance(false)
                            .small()
                            .text_size(px(14.))
                            .line_height(px(22.)),
                    )
                    .into_any_element(),
            ))
            .child(field(
                t("concepts.field.tags"),
                boxed(div().px(px(11.)).py(px(4.)).text_size(px(14.)))
                    .tag("concept-edit-tags")
                    .child(Input::new(&e.tags).appearance(false).px_0())
                    .into_any_element(),
            ))
            .child(field(
                t("concepts.field.detail"),
                boxed(div().px(px(3.)).py(px(4.)))
                    .tag("concept-edit-body")
                    .font_family(crate::markdown::MONO)
                    .child(
                        Textarea::new(&e.body)
                            .appearance(false)
                            .small()
                            .text_size(px(13.))
                            .line_height(px(20.)),
                    )
                    .into_any_element(),
            ))
            .when_some(d.write_error.clone(), |el, err| {
                el.child(
                    note_box(Note::Error, k)
                        .mb(px(12.))
                        .child(t_with("concepts.writeError", &[("err", &err)])),
                )
            })
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        primary_button("concept-save", k, busy)
                            .child(t(if busy {
                                "concepts.saving"
                            } else {
                                "common.save"
                            }))
                            .when(!busy, |b| {
                                b.on_click(cx.listener(|this, _, _, cx| this.concept_save(cx)))
                            }),
                    )
                    .child(
                        button("concept-cancel", k, false, busy)
                            .child(t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.concept_cancel_edit(cx))),
                    ),
            )
    }

    /// 지우기 확인 — 제목만 굵게, 되돌릴 수 없다는 한 줄
    pub(crate) fn concept_delete_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if self.section != crate::Section::Til {
            return None;
        }
        let d = self.concepts.detail.as_ref().filter(|d| d.confirm_delete)?;
        let r = self.selected_concept()?;
        let busy = d.busy;
        let close = || {
            cx.listener(|this, _, _, cx| {
                if let Some(d) = this.concepts.detail.as_mut() {
                    d.confirm_delete = false;
                }
                cx.notify();
            })
        };
        let title = s(r, "title").to_string();
        let sentence = t("concepts.delete.confirm");
        let (before, after) = sentence.split_once("{title}").unwrap_or((&sentence, ""));
        let text = format!("{before}{title}{after}");
        let bold = before.len()..before.len() + title.len();
        Some(crate::dialog::shell(
            k,
            440.,
            t("concepts.delete.title"),
            div()
                .child(StyledText::new(text).with_highlights(vec![(
                    bold,
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                )]))
                .child(t("concepts.delete.irreversible"))
                .into_any_element(),
            vec![
                button("concept-delete-cancel", k, false, busy)
                    .child(t("common.cancel"))
                    .on_click(close()),
                danger_button("concept-delete-confirm", k)
                    .child(t(if busy {
                        "concepts.delete.deleting"
                    } else {
                        "common.delete"
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.concept_delete(cx))),
            ],
            close(),
        ))
    }
}

/// 본문 링크는 브라우저로
fn open_link() -> crate::markdown::OnLink {
    std::sync::Arc::new(|url, _, _, cx| cx.open_url(url))
}

#[cfg(test)]
mod tests {
    use super::parse_tags;

    #[test]
    fn tags_split_on_commas_and_newlines_and_drop_the_hash() {
        assert_eq!(
            parse_tags(" #db, storage\n\n,#,api "),
            ["db", "storage", "api"]
        );
    }
}
