// 개념 AI 보강 — 지금 앱 AugmentModal.tsx. 지시(입력 + 빠른 지시 칩, 보낼 때 합친다) → 기다림 → 결과 검토
// (제목, 요약, 태그, 보강된 본문 — 미리보기 | 원문 고치기) → 적용. 결과가 떠 있거나 쓰는 중에 닫으면 한 번 묻는다.
// 적용은 내용만 바꾼다(확신, 학습 상태는 그대로).

use amber_core::ai as core_ai;
use amber_gpui::concepts;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::note_ai_text::compose_instruction;
use amber_gpui::vault;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::Spike;
use crate::assets::icon;
use crate::concept_detail::{parse_tags, tags_of};
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{Note, choice_chip, field_label, hint, note_box};
use crate::notes_view::Block;
use crate::theme::Tokens;

const PRESETS: [&str; 5] = [
    "concepts.augment.preset.examples",
    "concepts.augment.preset.deeper",
    "concepts.augment.preset.simpler",
    "concepts.augment.preset.concise",
    "concepts.augment.preset.pitfalls",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Prompt,
    Loading,
    Preview,
}

pub struct Augment {
    pub step: Step,
    pub instruction: Entity<TextareaState>,
    pub chosen: Vec<usize>,
    pub confirm_close: bool,
    pub error: Option<String>,
    pub saving: bool,
    pub title: Entity<InputState>,
    pub summary: Entity<TextareaState>,
    pub tags: Entity<InputState>,
    pub body: Entity<TextareaState>,
    pub show_source: bool,
    pub preview: Vec<Block>,
    /// 늦게 온 결과(닫은 뒤, 다시 실행한 뒤)는 버린다
    seq: u64,
}

impl Spike {
    /// 열 때마다 지금 개념과 본문에서 새로 시작한다
    pub(crate) fn open_augment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let body = self
            .concepts
            .detail
            .as_ref()
            .map(|d| d.body.clone())
            .unwrap_or_default();
        let s = |k: &str| {
            r.get(k)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let input = |v: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut st = InputState::new(window, cx);
                st.set_value(v, window, cx);
                st
            })
        };
        let area = |v: String,
                    rows: usize,
                    ph: Option<String>,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut st = TextareaState::new(window, cx).auto_grow(rows, rows);
                if let Some(ph) = ph {
                    st = st.placeholder(ph);
                }
                st.set_value(v, window, cx);
                st
            })
        };
        let instruction = area(
            String::new(),
            4,
            Some(t("concepts.augment.promptPlaceholder")),
            window,
            cx,
        );
        cx.observe(&instruction, |_, _, cx| cx.notify()).detach();
        instruction.update(cx, |st, cx| st.focus(window, cx));
        let text = crate::hex(Tokens::get(cx).text);
        let preview = crate::notes_view::build_blocks(&body, &text, cx).0;
        self.concepts.augment = Some(Augment {
            step: Step::Prompt,
            instruction,
            chosen: Vec::new(),
            confirm_close: false,
            error: None,
            saving: false,
            title: input(s("title"), window, cx),
            summary: area(s("summary"), 2, None, window, cx),
            tags: input(tags_of(&r).join(", "), window, cx),
            body: area(body, 16, None, window, cx),
            show_source: false,
            preview,
            seq: 0,
        });
        cx.notify();
    }

    pub(crate) fn augment_close_request(&mut self, cx: &mut Context<Self>) {
        self.augment_request_close(cx);
    }

    fn augment_request_close(&mut self, cx: &mut Context<Self>) {
        if let Some(a) = self.concepts.augment.as_mut() {
            if matches!(a.step, Step::Loading | Step::Preview) {
                a.confirm_close = true;
            } else {
                self.concepts.augment = None;
            }
        }
        cx.notify();
    }

    fn augment_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let Some((model, cli, provider, lang)) = crate::note_ai::ai_args(&self.db) else {
            return;
        };
        let Some(a) = self.concepts.augment.as_mut() else {
            return;
        };
        let typed = a.instruction.read(cx).value().to_string();
        let extras: Vec<String> = PRESETS
            .iter()
            .enumerate()
            .filter(|(i, _)| a.chosen.contains(i))
            .map(|(_, k)| t(k))
            .collect();
        let instruction = compose_instruction(&typed, &extras);
        if instruction.chars().count() < 2 {
            return;
        }
        a.error = None;
        a.step = Step::Loading;
        a.seq += 1;
        let seq = a.seq;
        self.focus.focus(window, cx);
        let s = |k: &str| {
            r.get(k)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let body = self
            .concepts
            .detail
            .as_ref()
            .map(|d| d.body.clone())
            .unwrap_or_default();
        let rx = crate::ai::spawn(core_ai::ai_augment(
            s("title"),
            s("summary"),
            tags_of(&r),
            body,
            instruction,
            model,
            cli,
            provider,
            None,
            Some(lang),
        ));
        cx.spawn_in(window, async move |this, cx| {
            let ex = cx.background_executor().clone();
            let Some(res) = crate::ai::wait(rx, ex).await else {
                return;
            };
            let _ = this.update_in(cx, |s, window, cx| {
                let Some(a) = s.concepts.augment.as_mut().filter(|a| a.seq == seq) else {
                    return;
                };
                match res {
                    Ok(g) => {
                        let n = g.note;
                        a.title
                            .update(cx, |st, cx| st.set_value(n.title, window, cx));
                        a.summary
                            .update(cx, |st, cx| st.set_value(n.summary, window, cx));
                        a.tags
                            .update(cx, |st, cx| st.set_value(n.tags.join(", "), window, cx));
                        a.body.update(cx, |st, cx| {
                            st.set_value(n.detail_markdown.clone(), window, cx)
                        });
                        a.show_source = false;
                        a.step = Step::Preview;
                        let text = crate::hex(Tokens::get(cx).text);
                        let blocks =
                            crate::notes_view::build_blocks(&n.detail_markdown, &text, cx).0;
                        if let Some(a) = s.concepts.augment.as_mut() {
                            a.preview = blocks;
                        }
                    }
                    Err(e) => {
                        a.error = Some(crate::ai::err_text(&e));
                        a.step = Step::Prompt;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 적용 — 제목과 요약이 비면 막는다. 내용만 바꾸고 본문 파일을 쓴다
    fn augment_apply(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.selected_concept().cloned() else {
            return;
        };
        let Some(a) = self.concepts.augment.as_mut() else {
            return;
        };
        let (title, summary, tags, body) = (
            a.title.read(cx).value().to_string(),
            a.summary.read(cx).value().to_string(),
            a.tags.read(cx).value().to_string(),
            a.body.read(cx).value().to_string(),
        );
        if vault::js_trim(&title).is_empty() || vault::js_trim(&summary).is_empty() {
            a.error = Some(t("concepts.form.required"));
            cx.notify();
            return;
        }
        let id = r.get("id").and_then(Json::as_i64).unwrap_or_default();
        let ulid = r
            .get("ulid")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let res = (|| -> Result<(), String> {
            concepts::update_concept_content(
                &self.db,
                id,
                Some(vault::js_trim(&title)),
                Some(vault::js_trim(&summary)),
            )
            .map_err(|e| e.to_string())?;
            concepts::set_concept_tags(&self.db, id, &parse_tags(&tags))
                .map_err(|e| e.to_string())?;
            vault::write_concept_note(&self.db, &ulid, &body).map(|_| ())
        })();
        match res {
            Ok(()) => {
                let text = crate::hex(Tokens::get(cx).text);
                let blocks = crate::notes_view::build_blocks(&body, &text, cx).0;
                if let Some(d) = self.concepts.detail.as_mut() {
                    d.body = body;
                    d.blocks = blocks;
                    d.missing = false;
                }
                self.concepts.augment = None;
                self.reload_concepts();
            }
            Err(e) => {
                if let Some(a) = self.concepts.augment.as_mut() {
                    a.error = Some(e);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn augment_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if self.section != crate::Section::Til {
            return None;
        }
        let a = self.concepts.augment.as_ref()?;
        if a.confirm_close {
            let running = a.step == Step::Loading;
            let keep = || {
                cx.listener(|this, _, _, cx| {
                    if let Some(a) = this.concepts.augment.as_mut() {
                        a.confirm_close = false;
                    }
                    cx.notify();
                })
            };
            return Some(crate::dialog::shell(
                k,
                440.,
                t("common.aiDiscard.title"),
                div()
                    .child(t(if running {
                        "common.aiDiscard.bodyRunning"
                    } else {
                        "common.aiDiscard.body"
                    }))
                    .into_any_element(),
                vec![
                    button("augment-keep", k, false, false)
                        .child(t("common.aiDiscard.keep"))
                        .on_click(keep()),
                    danger_button("augment-discard", k)
                        .child(t("common.aiDiscard.discard"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.concepts.augment = None;
                            cx.notify();
                        })),
                ],
                keep(),
            ));
        }
        let mut body = div().flex().flex_col();
        if let Some(e) = a.error.clone() {
            body = body.child(note_box(Note::Error, k).mb(px(12.)).child(e));
        }
        let reduce = cx.reduce_motion();
        let (left, buttons): (Vec<Stateful<Div>>, Vec<Stateful<Div>>) = match a.step {
            Step::Prompt => {
                body = body
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .mb(px(16.))
                            .child(field_label(t("concepts.augment.promptLabel"), k))
                            .child(
                                div()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(k.border_strong)
                                    .bg(k.surface)
                                    .px(px(3.))
                                    .py(px(4.))
                                    .tag("augment-instruction")
                                    .child(
                                        Textarea::new(&a.instruction)
                                            .appearance(false)
                                            .small()
                                            .text_size(px(14.))
                                            .line_height(px(22.)),
                                    ),
                            )
                            .child(hint(t("concepts.augment.promptHint"), k)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(field_label(t("concepts.augment.presets"), k))
                            .child(div().flex().flex_wrap().gap(px(6.)).children(
                                PRESETS.iter().enumerate().map(|(i, key)| {
                                    let on = a.chosen.contains(&i);
                                    choice_chip(
                                        SharedString::from(format!("augment-chip-{i}")),
                                        t(key),
                                        on,
                                        "plus",
                                        k,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(a) = this.concepts.augment.as_mut() {
                                                if let Some(p) =
                                                    a.chosen.iter().position(|c| *c == i)
                                                {
                                                    a.chosen.remove(p);
                                                } else {
                                                    a.chosen.push(i);
                                                }
                                            }
                                            cx.notify();
                                        },
                                    ))
                                }),
                            ))
                            .when(!a.chosen.is_empty(), |d| {
                                d.child(hint(
                                    t_with(
                                        "common.ai.chosenCount",
                                        &[("n", &a.chosen.len().to_string())],
                                    ),
                                    k,
                                ))
                            }),
                    );
                let short = vault::js_trim(&a.instruction.read(cx).value())
                    .chars()
                    .count()
                    < 2;
                let connected = crate::note_ai::ai_args(&self.db).is_some();
                let off = short || !connected;
                (
                    Vec::new(),
                    vec![
                        button("augment-cancel", k, false, false)
                            .child(t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.augment_request_close(cx))),
                        primary_button("augment-run", k, off)
                            .child(icon("sparkles", 15.).text_color(k.text))
                            .child(t("concepts.augment.run"))
                            .when(!off, |b| {
                                b.on_click(
                                    cx.listener(|this, _, window, cx| this.augment_run(window, cx)),
                                )
                            }),
                    ],
                )
            }
            Step::Loading => {
                body = body.child(crate::note_ai_view::ai_thinking(
                    "augment-thinking",
                    t("concepts.augment.thinking"),
                    Some(t("concepts.augment.thinkingHint")),
                    false,
                    k,
                    reduce,
                ));
                (Vec::new(), Vec::new())
            }
            Step::Preview => {
                let boxed = |d: Div| {
                    d.max_w(px(440.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(k.border_strong)
                        .bg(k.surface)
                };
                let field = |label: String, control: AnyElement| {
                    div()
                        .flex()
                        .flex_col()
                        .mb(px(16.))
                        .child(field_label(label, k))
                        .child(control)
                };
                let on_link: crate::markdown::OnLink =
                    std::sync::Arc::new(|url, _, _, cx| cx.open_url(url));
                body = body
                    .child(field(
                        t("concepts.field.title"),
                        boxed(div().px(px(11.)).py(px(4.)).text_size(px(14.)))
                            .child(Input::new(&a.title).appearance(false).px_0())
                            .into_any_element(),
                    ))
                    .child(field(
                        t("concepts.field.summary"),
                        boxed(div().px(px(3.)).py(px(4.)))
                            .child(
                                Textarea::new(&a.summary)
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
                            .child(Input::new(&a.tags).appearance(false).px_0())
                            .into_any_element(),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .mb(px(5.))
                                    .child(field_label(t("concepts.field.augmentedNote"), k).mb_0())
                                    .child(div().flex_1())
                                    .child(
                                        button("augment-source", k, false, false)
                                            .child(t(if a.show_source {
                                                "concepts.preview.show"
                                            } else {
                                                "concepts.preview.source"
                                            }))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let text = crate::hex(Tokens::get(cx).text);
                                                if let Some(a) = this.concepts.augment.as_mut() {
                                                    a.show_source = !a.show_source;
                                                    if !a.show_source {
                                                        let md =
                                                            a.body.read(cx).value().to_string();
                                                        a.preview =
                                                            crate::notes_view::build_blocks(
                                                                &md, &text, cx,
                                                            )
                                                            .0;
                                                    }
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(if a.show_source {
                                div()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(k.border_strong)
                                    .bg(k.surface)
                                    .px(px(3.))
                                    .py(px(4.))
                                    .font_family(crate::markdown::MONO)
                                    .tag("augment-body-src")
                                    .child(
                                        Textarea::new(&a.body)
                                            .appearance(false)
                                            .small()
                                            .text_size(px(13.))
                                            .line_height(px(20.)),
                                    )
                                    .into_any_element()
                            } else {
                                div()
                                    .tag("augment-preview")
                                    .min_h(px(200.))
                                    .p(px(12.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(k.border)
                                    .flex()
                                    .flex_col()
                                    .children(a.preview.iter().enumerate().map(|(i, b)| {
                                        match b {
                                            Block::Text(state) => crate::markdown::view(
                                                state,
                                                on_link.clone(),
                                                k,
                                                None,
                                            )
                                            .into_any_element(),
                                            other => self.note_block(
                                                other,
                                                5_000_000 + i as u64,
                                                &on_link,
                                                k,
                                                cx,
                                            ),
                                        }
                                    }))
                                    .into_any_element()
                            }),
                    );
                let saving = a.saving;
                (
                    vec![
                        button("augment-again", k, false, false)
                            .child(icon("chevron-left", 14.).text_color(k.text_2))
                            .child(t("concepts.augment.again"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(a) = this.concepts.augment.as_mut() {
                                    a.step = Step::Prompt;
                                }
                                cx.notify();
                            })),
                    ],
                    vec![
                        button("augment-cancel", k, false, saving)
                            .child(t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.augment_request_close(cx))),
                        primary_button("augment-apply", k, saving)
                            .child(t(if saving {
                                "concepts.augment.applying"
                            } else {
                                "concepts.augment.apply"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| this.augment_apply(cx))),
                    ],
                )
            }
        };
        Some(crate::dialog::shell_split(
            k,
            820.,
            t("concepts.augment.title"),
            body.into_any_element(),
            left,
            buttons,
            cx.listener(|this, _, _, cx| this.augment_request_close(cx)),
        ))
    }
}
