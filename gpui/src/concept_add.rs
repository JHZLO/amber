// 새 개념 추가 — 지금 앱 AddConceptModal.tsx. 원문 붙여넣기(+ 지시문, 다음에도 기본값) → Claude 로 정리 또는 직접 작성 →
// 검토(제목, 요약, 태그, 확신 1~3, 상세 노트 원문 | 미리보기) → 저장(학습 중). DB 행을 먼저 만들고 본문 파일을 쓴다 —
// 파일이 실패하면 행을 되돌린다(어느 화면에도 안 보이는 고아 파일을 남기지 않게).
// 결과가 떠 있거나 쓰는 중에 닫으면 한 번 묻는다.

use amber_core::ai as core_ai;
use amber_gpui::concepts::{self, CreateConceptInput};
use amber_gpui::i18n::{t, t_with};
use amber_gpui::vault;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::concept_detail::parse_tags;
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{Note, field_label, hint, note_box};
use crate::notes_view::Block;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Paste,
    Loading,
    Preview,
}

/// 노트에서 올리기 — 드래그한 글과 노트 전체(문맥)
#[derive(Clone, Debug)]
pub struct PromoteTarget {
    pub note_rel: String,
    pub selection: String,
    pub note_body: String,
}

pub struct AddConcept {
    /// 노트에서 올리는 중이면 그 대상(붙여넣기 단계가 없고, 출처를 남긴다)
    pub promote: Option<PromoteTarget>,
    pub step: Step,
    pub transcript: Entity<TextareaState>,
    pub instruction: Entity<TextareaState>,
    pub error: Option<String>,
    pub saving: bool,
    pub title: Entity<InputState>,
    pub summary: Entity<TextareaState>,
    pub tags: Entity<InputState>,
    pub confidence: i64,
    pub body: Entity<TextareaState>,
    pub show_preview: bool,
    pub preview: Vec<Block>,
    pub confirm_close: bool,
    seq: u64,
}

/// JS 문자열 길이(UTF-16 단위)
fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

impl Spike {
    pub(crate) fn open_add_concept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let area =
            |rows: usize, ph: Option<String>, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    let mut st = TextareaState::new(window, cx).auto_grow(rows, rows);
                    if let Some(ph) = ph {
                        st = st.placeholder(ph);
                    }
                    st
                })
            };
        let transcript = area(12, Some(t("concepts.add.pastePlaceholder")), window, cx);
        let instruction = area(
            3,
            Some(t("concepts.add.instructionPlaceholder")),
            window,
            cx,
        );
        // 저장된 기본 지시문(한 번 써 두면 계속)
        if let Ok(Some(v)) = concepts::get_setting(&self.db, "default_instruction")
            && !v.is_empty()
        {
            instruction.update(cx, |st, cx| st.set_value(v, window, cx));
        }
        cx.observe(&transcript, |_, _, cx| cx.notify()).detach();
        transcript.update(cx, |st, cx| st.focus(window, cx));
        self.concepts.add = Some(AddConcept {
            step: Step::Paste,
            transcript,
            instruction,
            error: None,
            saving: false,
            title: cx.new(|cx| InputState::new(window, cx)),
            summary: area(2, None, window, cx),
            tags: cx.new(|cx| InputState::new(window, cx)),
            confidence: 1,
            body: area(16, None, window, cx),
            show_preview: false,
            preview: Vec::new(),
            confirm_close: false,
            seq: 0,
            promote: None,
        });
        cx.notify();
    }

    /// 노트의 선택을 개념으로 — 연결이 있으면 곧장 정리하고, 없으면 선택을 씨앗으로 손으로 쓴다
    pub(crate) fn open_promote(
        &mut self,
        target: PromoteTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_add_concept(window, cx);
        let connected = crate::note_ai::ai_args(&self.db).is_some();
        if let Some(a) = self.concepts.add.as_mut() {
            a.promote = Some(target.clone());
        }
        if connected {
            let input = format!(
                "[개념으로 만들 부분]\n{}\n\n[출처 노트 전체 (문맥)]\n{}",
                target.selection, target.note_body
            );
            let instruction = "위 [개념으로 만들 부분]을 복습용 개념 카드로 만들어라. 그 부분을 중심 주제로 삼되, \
[출처 노트 전체]를 문맥으로만 참고해 제목·요약·상세 노트·태그를 작성하라. \
노트 전체를 요약하지 말고 선택한 개념에 집중하라."
                .to_string();
            self.run_generate(input, instruction, false, window, cx);
        } else {
            self.promote_seed(window, cx);
        }
    }

    /// 선택을 씨앗으로 — 제목은 앞 40자(UTF-16), 본문은 선택 그대로
    fn promote_seed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sel) = self
            .concepts
            .add
            .as_ref()
            .and_then(|a| a.promote.as_ref())
            .map(|p| p.selection.clone())
        else {
            return;
        };
        let units: Vec<u16> = sel.encode_utf16().collect();
        let title = String::from_utf16_lossy(&units[..units.len().min(40)]);
        self.add_fill(title, String::new(), String::new(), 1, sel, window, cx);
    }

    fn add_request_close(&mut self, cx: &mut Context<Self>) {
        if let Some(a) = self.concepts.add.as_mut() {
            // 올리기는 늘 묻는다(저장 중에는 닫지 않는다)
            if a.promote.is_some() {
                if !a.saving {
                    a.confirm_close = true;
                }
            } else if matches!(a.step, Step::Loading | Step::Preview) {
                a.confirm_close = true;
            } else {
                self.concepts.add = None;
            }
        }
        cx.notify();
    }

    /// Esc — 확인 창이 떠 있으면 그것부터. 닫은 게 있으면 true
    pub(crate) fn add_concept_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(a) = self.concepts.add.as_mut() else {
            return false;
        };
        if a.confirm_close {
            a.confirm_close = false;
        } else {
            self.add_request_close(cx);
        }
        true
    }

    /// 결과 칸들을 채운다(창이 있어야 값을 넣는다)
    pub(crate) fn add_fill(
        &mut self,
        title: String,
        summary: String,
        tags: String,
        confidence: i64,
        body: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.concepts.add.as_mut() else {
            return;
        };
        a.title.update(cx, |st, cx| st.set_value(title, window, cx));
        a.summary
            .update(cx, |st, cx| st.set_value(summary, window, cx));
        a.tags.update(cx, |st, cx| st.set_value(tags, window, cx));
        a.body.update(cx, |st, cx| st.set_value(body, window, cx));
        a.confidence = confidence;
        // 올리기는 미리보기부터, 추가는 원문부터
        a.show_preview = a.promote.is_some();
        a.step = Step::Preview;
        if a.show_preview {
            let md = a.body.read(cx).value().to_string();
            let text = crate::hex(Tokens::get(cx).text);
            let blocks = crate::notes_view::build_blocks(&md, &text, cx).0;
            if let Some(a) = self.concepts.add.as_mut() {
                a.preview = blocks;
            }
        }
        // 사라진 칸이 포커스를 쥐고 있으면 Esc 가 뿌리에 안 닿는다
        self.focus.focus(window, cx);
    }

    fn add_generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.concepts.add.as_ref() else {
            return;
        };
        let transcript = a.transcript.read(cx).value().to_string();
        let instruction = a.instruction.read(cx).value().to_string();
        self.run_generate(transcript, instruction, true, window, cx);
    }

    /// 정리 — remember = 지시문을 다음 추가의 기본값으로 남긴다(붙여넣기 추가만)
    fn run_generate(
        &mut self,
        transcript: String,
        instruction: String,
        remember: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((model, cli, provider, lang)) = crate::note_ai::ai_args(&self.db) else {
            return;
        };
        let Some(a) = self.concepts.add.as_mut() else {
            return;
        };
        a.error = None;
        a.step = Step::Loading;
        a.seq += 1;
        let seq = a.seq;
        self.focus.focus(window, cx);
        if remember {
            let _ = concepts::set_setting(
                &self.db,
                "default_instruction",
                vault::js_trim(&instruction),
            );
        }
        let rx = crate::ai::spawn(core_ai::ai_generate(
            transcript,
            Some(instruction),
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
                if s.concepts.add.as_ref().is_none_or(|a| a.seq != seq) {
                    return;
                }
                match res {
                    Ok(g) => {
                        let n = g.note;
                        s.add_fill(
                            n.title,
                            n.summary,
                            n.tags.join(", "),
                            n.confidence_suggestion as i64,
                            n.detail_markdown,
                            window,
                            cx,
                        );
                    }
                    Err(e) => {
                        let promote = s.concepts.add.as_ref().is_some_and(|a| a.promote.is_some());
                        // 올리기는 실패해도 선택으로 손으로 이어 쓸 수 있게
                        if promote {
                            s.promote_seed(window, cx);
                        }
                        if let Some(a) = s.concepts.add.as_mut() {
                            a.error = Some(crate::ai::err_text(&e));
                            if !promote {
                                a.step = Step::Paste;
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 저장 — DB 행 먼저, 본문 파일 다음(실패하면 행을 되돌린다)
    fn add_save(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.concepts.add.as_mut() else {
            return;
        };
        let (title, summary, tags, body, transcript) = (
            a.title.read(cx).value().to_string(),
            a.summary.read(cx).value().to_string(),
            a.tags.read(cx).value().to_string(),
            a.body.read(cx).value().to_string(),
            a.transcript.read(cx).value().to_string(),
        );
        if vault::js_trim(&title).is_empty() || vault::js_trim(&summary).is_empty() {
            a.error = Some(t("concepts.form.required"));
            cx.notify();
            return;
        }
        let id = ulid::Ulid::new().to_string();
        let promote = a.promote.clone();
        // 출처 = 작업 폴더 + 노트 경로 + 앵커 — 폴더를 바꾼 뒤 같은 이름의 다른 노트를 조용히 열지 않게
        let (source, source_kind) = match &promote {
            Some(p) => (
                Some(
                    serde_json::json!({
                        "root": vault::notes(&self.db).full(""),
                        "noteRel": p.note_rel,
                        "anchor": p.selection,
                    })
                    .to_string(),
                ),
                "file",
            ),
            None => ((!transcript.is_empty()).then_some(transcript), "paste"),
        };
        let input = CreateConceptInput {
            ulid: id.clone(),
            title: vault::js_trim(&title).to_string(),
            summary: vault::js_trim(&summary).to_string(),
            detail_path: vault::detail_path_for(&id),
            tags: Some(parse_tags(&tags)),
            confidence: Some(a.confidence as f64),
            source,
            source_kind: Some(source_kind.into()),
        };
        let title = vault::js_trim(&title).to_string();
        let res = concepts::create_concept(&self.db, &input)
            .map_err(|e| e.to_string())
            .and_then(|cid| {
                vault::write_concept_note(&self.db, &id, &body)
                    .map(|_| cid)
                    .inspect_err(|_| {
                        let _ = concepts::delete_concept(&self.db, cid);
                    })
            })
            .and_then(|cid| match &promote {
                // 노트 쪽 역참조(노트 → 개념)
                Some(p) => amber_gpui::note_concepts::add_note_concept(
                    &self.db,
                    &p.note_rel,
                    &serde_json::json!({
                        "conceptId": cid,
                        "title": title,
                        "anchor": p.selection,
                        "createdAt": chrono::Local::now().timestamp_millis(),
                    }),
                ),
                None => Ok(()),
            });
        match res {
            Ok(()) => {
                self.concepts.add = None;
                self.reload_concepts();
                if let Some(p) = promote
                    && self.notes.selected.as_deref() == Some(p.note_rel.as_str())
                {
                    self.notes.made =
                        amber_gpui::note_concepts::load_note_concepts(&self.db, &p.note_rel);
                }
            }
            Err(e) => {
                if let Some(a) = self.concepts.add.as_mut() {
                    a.error = Some(e);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn add_concept_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let a = self.concepts.add.as_ref()?;
        // 올리기는 노트 탭에서 뜬다
        if self.section != crate::Section::Til && a.promote.is_none() {
            return None;
        }
        let promote = a.promote.is_some();
        if a.confirm_close {
            let running = a.step == Step::Loading;
            let keep = || {
                cx.listener(|this, _, _, cx| {
                    if let Some(a) = this.concepts.add.as_mut() {
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
                    button("add-keep", k, false, false)
                        .child(t("common.aiDiscard.keep"))
                        .on_click(keep()),
                    danger_button("add-discard", k)
                        .child(t("common.aiDiscard.discard"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.concepts.add = None;
                            cx.notify();
                        })),
                ],
                keep(),
            ));
        }
        let area_box = |e: &Entity<TextareaState>, mono: bool, tag: &'static str| {
            div()
                .rounded(px(6.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .px(px(3.))
                .py(px(4.))
                .tag(tag)
                .when(mono, |d| d.font_family(crate::markdown::MONO))
                .child(
                    Textarea::new(e)
                        .appearance(false)
                        .small()
                        .text_size(px(if mono { 13. } else { 14. }))
                        .line_height(px(if mono { 20. } else { 22. })),
                )
        };
        let input_box = |e: &Entity<InputState>, tag: &'static str| {
            div()
                .max_w(px(440.))
                .rounded(px(6.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .px(px(11.))
                .py(px(4.))
                .text_size(px(14.))
                .tag(tag)
                .child(Input::new(e).appearance(false).px_0())
        };
        let field = |label: String, control: AnyElement| {
            div()
                .flex()
                .flex_col()
                .mb(px(16.))
                .child(field_label(label, k))
                .child(control)
        };
        let mut body = div().flex().flex_col();
        if let Some(e) = a.error.clone() {
            body = body.child(note_box(Note::Error, k).mb(px(12.)).child(e));
        }
        let (left, buttons): (Vec<Stateful<Div>>, Vec<Stateful<Div>>) = match a.step {
            Step::Paste => {
                let text = a.transcript.read(cx).value().to_string();
                let short = js_len(vault::js_trim(&text)) < 20;
                let mut count = t_with(
                    "concepts.add.charCount",
                    &[("n", &js_len(&text).to_string())],
                );
                if short {
                    count.push(' ');
                    count.push_str(&t("concepts.add.tooShort"));
                }
                body = body
                    .child(
                        field(
                            t("concepts.add.pasteLabel"),
                            area_box(&a.transcript, false, "add-transcript").into_any_element(),
                        )
                        .mb_0(),
                    )
                    .child(hint(count, k))
                    .child(
                        field(
                            t("concepts.add.instructionLabel"),
                            div()
                                .flex()
                                .flex_col()
                                .child(area_box(&a.instruction, false, "add-instruction"))
                                .child(hint(t("concepts.add.instructionHint"), k))
                                .into_any_element(),
                        )
                        .mt(px(16.)),
                    );
                (
                    Vec::new(),
                    vec![
                        button("add-manual", k, false, false)
                            .child(t("concepts.add.manual"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_fill(
                                    String::new(),
                                    String::new(),
                                    String::new(),
                                    1,
                                    String::new(),
                                    window,
                                    cx,
                                );
                                if let Some(a) = this.concepts.add.as_mut() {
                                    a.error = None;
                                }
                                cx.notify();
                            })),
                        primary_button("add-generate", k, short)
                            .child(icon("sparkles", 15.).text_color(k.text))
                            .child(t("concepts.add.generate"))
                            .when(!short, |b| {
                                b.on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.add_generate(window, cx)
                                    }),
                                )
                            }),
                    ],
                )
            }
            Step::Loading => {
                body = body.child(crate::note_ai_view::ai_thinking(
                    "add-thinking",
                    t(if promote {
                        "concepts.promote.thinking"
                    } else {
                        "concepts.add.thinking"
                    }),
                    Some(t(if promote {
                        "concepts.promote.thinkingHint"
                    } else {
                        "concepts.add.thinkingHint"
                    })),
                    false,
                    k,
                    cx.reduce_motion(),
                ));
                (Vec::new(), Vec::new())
            }
            Step::Preview => {
                let on_link: crate::markdown::OnLink =
                    std::sync::Arc::new(|url, _, _, cx| cx.open_url(url));
                let conf = a.confidence;
                body = body
                    .child(field(
                        t("concepts.field.title"),
                        input_box(&a.title, "add-title").into_any_element(),
                    ))
                    .child(field(
                        t("concepts.field.summary"),
                        area_box(&a.summary, false, "add-summary").into_any_element(),
                    ))
                    .child(field(
                        t("concepts.field.tags"),
                        input_box(&a.tags, "add-tags").into_any_element(),
                    ))
                    .when(!promote, |d| {
                        d.child(field(
                            t("concepts.field.confidence"),
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .children((1..=3).map(|n| {
                                    let id = SharedString::from(format!("add-conf-{n}"));
                                    let b = if conf == n {
                                        primary_button(id, k, false)
                                    } else {
                                        button(id, k, false, false)
                                    };
                                    b.child(n.to_string()).on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(a) = this.concepts.add.as_mut() {
                                                a.confidence = n;
                                            }
                                            cx.notify();
                                        },
                                    ))
                                }))
                                .child(hint(t("concepts.add.confidenceHint"), k).mt_0().ml(px(6.)))
                                .into_any_element(),
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .mb(px(5.))
                                    .child(field_label(t("concepts.field.detailNote"), k).mb_0())
                                    .child(div().flex_1())
                                    .child(
                                        button("add-preview-toggle", k, false, false)
                                            .child(t(if a.show_preview {
                                                "concepts.preview.source"
                                            } else {
                                                "concepts.preview.show"
                                            }))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let text = crate::hex(Tokens::get(cx).text);
                                                if let Some(a) = this.concepts.add.as_mut() {
                                                    a.show_preview = !a.show_preview;
                                                    if a.show_preview {
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
                            .child(if a.show_preview {
                                div()
                                    .tag("add-preview")
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
                                                6_000_000 + i as u64,
                                                &on_link,
                                                k,
                                                cx,
                                            ),
                                        }
                                    }))
                                    .into_any_element()
                            } else {
                                area_box(&a.body, true, "add-body").into_any_element()
                            }),
                    );
                let saving = a.saving;
                if promote {
                    return Some(crate::dialog::shell_split(
                        k,
                        820.,
                        t("concepts.promote.title"),
                        body.into_any_element(),
                        Vec::new(),
                        vec![
                            button("add-cancel", k, false, saving)
                                .child(t("common.cancel"))
                                .on_click(cx.listener(|this, _, _, cx| this.add_request_close(cx))),
                            primary_button("add-save", k, saving)
                                .child(icon("layers", 15.).text_color(k.text))
                                .child(t(if saving {
                                    "concepts.saving"
                                } else {
                                    "concepts.promote.save"
                                }))
                                .when(!saving, |b| {
                                    b.on_click(cx.listener(|this, _, _, cx| this.add_save(cx)))
                                }),
                        ],
                        cx.listener(|this, _, _, cx| this.add_request_close(cx)),
                    ));
                }
                (
                    vec![
                        button("add-back", k, false, false)
                            .child(icon("chevron-left", 14.).text_color(k.text_2))
                            .child(t("concepts.add.backToSource"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(a) = this.concepts.add.as_mut() {
                                    a.step = Step::Paste;
                                }
                                cx.notify();
                            })),
                    ],
                    vec![
                        button("add-cancel", k, false, false)
                            .child(t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.add_request_close(cx))),
                        primary_button("add-save", k, saving)
                            .child(t(if saving {
                                "concepts.saving"
                            } else {
                                "concepts.add.save"
                            }))
                            .when(!saving, |b| {
                                b.on_click(cx.listener(|this, _, _, cx| this.add_save(cx)))
                            }),
                    ],
                )
            }
        };
        Some(crate::dialog::shell_split(
            k,
            820.,
            t(if promote {
                "concepts.promote.title"
            } else {
                "concepts.add.title"
            }),
            body.into_any_element(),
            left,
            buttons,
            cx.listener(|this, _, _, cx| this.add_request_close(cx)),
        ))
    }
}
