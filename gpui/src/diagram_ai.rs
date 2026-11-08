// 다이어그램 AI 모달 — 지금 앱 DiagramAiModal.tsx. 스키마 DDL → ai_erd_generate_stream → 미리보기 → 편집 초안으로.
// 파일에 바로 쓰지 않는다 — 적용하면 편집이 열리고 ⌘S 로 저장한다(AI 출력은 초안). 노트 AI 와 달리 뒤에서 돌지 않는다:
// 닫으면(묻고 나서) 실행도 결과도 버린다.

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amber_core::Channel;
use amber_core::ai as core_ai;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::line_diff::{Row, diff_rows};
use amber_gpui::note_ai_text::compose_instruction;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{
    Note, ai_thinking, choice_chip, diff_view, field_label, hash2, hint, note_box, seg_tab,
    segmented,
};
use crate::theme::Tokens;

/// 변환 방향을 바꾸는 자주 쓰는 단서(프롬프트의 기본 규칙을 덮어쓴다)
const PRESETS: [&str; 5] = [
    "diagrams.ai.preset.noAudit",
    "diagrams.ai.preset.noIndex",
    "diagrams.ai.preset.inferFk",
    "diagrams.ai.preset.coreOnly",
    "diagrams.ai.preset.append",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Prompt,
    Loading,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Diagram,
    Source,
    Diff,
}

pub struct DgmAi {
    pub step: Step,
    pub ddl: Entity<TextareaState>,
    pub instr: Entity<TextareaState>,
    /// 고른 빠른 지시(PRESETS 자리) — 글은 보낼 때 합친다
    pub chosen: Vec<usize>,
    pub confirm_close: bool,
    pub error: Option<String>,
    pub result: String,
    /// 소스 탭 입력칸 — 탭에 들어갈 때 만들고(render 훅), 고친 글은 result 로 간다
    pub source: Option<Entity<TextareaState>>,
    pub stream: String,
    pub view: View,
    /// 연 때의 원문(편집 중이면 초안) — 이어 붙이기, 변경점의 기준
    pub current: String,
    /// 실행 세대 — 버려진 실행의 글자가 새 실행에 섞이지 않게
    seq: u64,
    key: Option<String>,
    stream_scroll: ScrollHandle,
    fig: Option<(String, Result<crate::figure::Figure, String>)>,
    diff: Option<(u64, Rc<(Vec<Row>, usize, usize)>)>,
}

fn close_ai(this: &mut Spike, _: &ClickEvent, _: &mut Window, cx: &mut Context<Spike>) {
    this.request_close_diagram_ai();
    cx.notify();
}

fn too_short(ddl: &str) -> bool {
    ddl.trim().encode_utf16().count() < 20
}

impl Spike {
    /// 머리의 'DDL → ERD' — 연결이 없거나 파일을 못 읽었으면 막혀 있다
    pub(crate) fn open_diagram_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.diagrams.selected.is_none() || self.diagrams.read_error.is_some() {
            return;
        }
        let current = self
            .diagram_draft(cx)
            .unwrap_or_else(|| self.diagrams.body.clone());
        let ddl = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(12, 12)
                .placeholder(t("diagrams.ai.ddlPh"))
        });
        let instr = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 2)
                .placeholder(t("diagrams.ai.instrPh"))
        });
        // 짧음 판정과 버튼 막힘이 글자마다 바뀐다
        cx.observe(&ddl, |_, _, cx| cx.notify()).detach();
        ddl.update(cx, |s, cx| s.focus(window, cx));
        self.diagrams.ai = Some(DgmAi {
            step: Step::Prompt,
            ddl,
            instr,
            chosen: Vec::new(),
            confirm_close: false,
            error: None,
            result: String::new(),
            source: None,
            stream: String::new(),
            view: View::Diagram,
            current,
            seq: 0,
            key: None,
            stream_scroll: ScrollHandle::new(),
            fig: None,
            diff: None,
        });
    }

    /// render 에서 — 소스 탭 입력칸(손으로 고친 결과가 적용된다)
    pub(crate) fn prepare_diagram_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.diagrams.ai.as_mut() else {
            return;
        };
        let want = a.step == Step::Preview && a.view == View::Source;
        if want && a.source.is_none() {
            let v = a.result.clone();
            let src = cx.new(|cx| {
                let mut s = TextareaState::new(window, cx).auto_grow(18, 18);
                s.set_value(v, window, cx);
                s
            });
            cx.observe(&src, |this, src, cx| {
                let v = src.read(cx).value().to_string();
                if let Some(a) = this.diagrams.ai.as_mut()
                    && a.result != v
                {
                    a.result = v;
                    cx.notify();
                }
            })
            .detach();
            a.source = Some(src);
        } else if !want {
            a.source = None;
        }
    }

    fn run_diagram_ai(&mut self, cx: &mut Context<Self>) {
        let Some((model, cli, provider, lang)) = crate::note_ai::ai_args(&self.db) else {
            return;
        };
        let Some(a) = self.diagrams.ai.as_mut() else {
            return;
        };
        let ddl = a.ddl.read(cx).value().to_string();
        if too_short(&ddl) {
            return;
        }
        // 내가 친 말 → 빠른 지시
        let extras: Vec<String> = PRESETS
            .iter()
            .enumerate()
            .filter(|(i, _)| a.chosen.contains(i))
            .map(|(_, k)| t(k))
            .collect();
        let instruction = compose_instruction(&a.instr.read(cx).value(), &extras);
        a.error = None;
        a.stream.clear();
        a.step = Step::Loading;
        a.seq += 1;
        let seq = a.seq;
        let key = format!("dgm-ai-{}", amber_gpui::comments::new_comment_id());
        a.key = Some(key.clone());
        let live: Arc<Mutex<String>> = Arc::default();
        let l = live.clone();
        let on_delta = Channel::new(move |d: String| {
            if let Ok(mut s) = l.lock() {
                s.push_str(&d);
            }
        });
        let current = a.current.clone();
        let mut rx = crate::ai::spawn(async move {
            core_ai::ai_erd_generate_stream(
                ddl,
                Some(instruction),
                Some(current),
                model,
                cli,
                provider,
                None,
                Some(lang),
                on_delta,
                Some(key),
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            loop {
                ex.timer(Duration::from_millis(100)).await;
                let done = match rx.try_recv() {
                    Ok(None) => None,
                    Ok(Some(r)) => Some(Some(r)),
                    Err(_) => Some(None),
                };
                let keep = this.update(cx, |s, cx| {
                    // 중단, 다시 실행, 닫기 — 이 실행은 버려졌다
                    let Some(a) = s.diagrams.ai.as_mut().filter(|a| a.seq == seq) else {
                        return false;
                    };
                    if let Ok(t) = live.lock()
                        && t.len() != a.stream.len()
                    {
                        a.stream = t.clone();
                        a.stream_scroll.scroll_to_bottom();
                        cx.notify();
                    }
                    let Some(done) = done else {
                        return true;
                    };
                    a.key = None;
                    match done {
                        Some(Ok(r)) => {
                            a.result = r.mermaid;
                            a.view = View::Diagram;
                            a.step = Step::Preview;
                        }
                        Some(Err(e)) => {
                            a.error = Some(crate::ai::err_text(&e));
                            a.step = Step::Prompt;
                        }
                        None => a.step = Step::Prompt,
                    }
                    cx.notify();
                    false
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// 중단 — CLI 를 죽이고 기다리지 않고 바로 지시 화면으로
    fn stop_diagram_ai(&mut self) {
        let Some(a) = self.diagrams.ai.as_mut() else {
            return;
        };
        a.seq += 1;
        if let Some(k) = a.key.take() {
            core_ai::ai_cancel(k);
        }
        a.stream.clear();
        a.step = Step::Prompt;
    }

    /// Esc — 확인 창이면 그것만 닫고, 아니면 닫기를 청한다(입력이 있으면 먼저 묻는다)
    pub(crate) fn dgm_ai_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(a) = self.diagrams.ai.as_mut() else {
            return false;
        };
        if !std::mem::take(&mut a.confirm_close) {
            self.esc_close("dgm-ai", window, cx, |s, _, _| s.request_close_diagram_ai());
        }
        true
    }

    /// 닫기 — 결과가 있거나 생성 중이면 한 번 묻는다. 몇 분 걸린 생성물이 X 한 번에 사라지면 안 된다
    fn request_close_diagram_ai(&mut self) {
        let Some(a) = self.diagrams.ai.as_mut() else {
            return;
        };
        if a.step != Step::Prompt || !a.result.trim().is_empty() {
            a.confirm_close = true;
        } else {
            self.diagrams.ai = None;
        }
    }

    fn discard_diagram_ai(&mut self) {
        if self
            .diagrams
            .ai
            .as_ref()
            .is_some_and(|a| a.step == Step::Loading)
        {
            self.stop_diagram_ai();
        }
        self.diagrams.ai = None;
    }

    /// 적용 — 편집 초안으로(파일은 ⌘S 전까지 그대로)
    fn apply_diagram_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.diagrams.ai.take() else {
            return;
        };
        if self.diagrams.edit.is_none() {
            self.start_diagram_edit(window, cx);
        }
        let Some(state) = self.diagrams.edit.as_ref().map(|e| e.state.clone()) else {
            return;
        };
        state.update(cx, |s, cx| s.set_value(a.result.clone(), window, cx));
        let dirty = a.result != self.diagrams.body;
        if let Some(e) = self.diagrams.edit.as_mut() {
            e.dirty = dirty;
        }
        self.diagrams.canvas.set_chart(&a.result);
    }

    pub(crate) fn diagram_ai_view(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let connected = self.ai_connected();
        let reduce = cx.reduce_motion();
        let a = self.diagrams.ai.as_mut()?;
        let has_existing = !a.current.trim().is_empty();
        // 그림과 변경점은 비싸다 — 결과가 그대로면 다시 만들지 않는다
        if a.step == Step::Preview
            && a.view == View::Diagram
            && a.fig.as_ref().is_none_or(|(src, _)| *src != a.result)
        {
            a.fig = Some((a.result.clone(), crate::figure::mermaid(&a.result)));
        }
        if a.step == Step::Preview && a.view == View::Diff {
            let h = hash2(&a.current, &a.result);
            if a.diff.as_ref().is_none_or(|(x, _)| *x != h) {
                a.diff = Some((h, Rc::new(diff_rows(&a.current, &a.result))));
            }
        }
        let a = self.diagrams.ai.as_ref()?;
        let short = too_short(&a.ddl.read(cx).value());

        let mut left: Vec<Stateful<Div>> = Vec::new();
        let mut right: Vec<Stateful<Div>> = Vec::new();
        match a.step {
            Step::Prompt => {
                right.push(
                    button("dgm-ai-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(close_ai)),
                );
                right.push(
                    primary_button("dgm-ai-run", k, short || !connected)
                        .child(icon("sparkles", 15.).text_color(k.text))
                        .child(t("diagrams.ai.convert"))
                        .when(!short && connected, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| {
                                this.run_diagram_ai(cx);
                                cx.notify();
                            }))
                        }),
                );
            }
            Step::Loading => right.push(
                danger_button("dgm-ai-stop", k)
                    .child(icon("x", 14.).text_color(k.danger))
                    .child(t("diagrams.ai.stop"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stop_diagram_ai();
                        cx.notify();
                    })),
            ),
            Step::Preview => {
                left.push(
                    button("dgm-ai-back", k, false, false)
                        .child(icon("chevron-left", 14.).text_color(k.text_2))
                        .child(t("diagrams.ai.back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(a) = this.diagrams.ai.as_mut() {
                                a.step = Step::Prompt;
                            }
                            cx.notify();
                        })),
                );
                right.push(
                    button("dgm-ai-close", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(close_ai)),
                );
                right.push(
                    primary_button("dgm-ai-apply", k, false)
                        .child(icon("check", 15.).text_color(k.text))
                        .child(t("diagrams.ai.apply"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_diagram_ai(window, cx);
                            cx.notify();
                        })),
                );
            }
        }

        let mut body = div().flex().flex_col();
        if let Some(e) = &a.error {
            body = body.child(note_box(Note::Error, k).mb(px(12.)).child(e.clone()));
        }
        body = match a.step {
            Step::Prompt => body.children(self.diagram_ai_fields(a, k, cx)),
            Step::Loading => body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(ai_thinking(
                        "dgm-ai-bar",
                        t("diagrams.ai.generating"),
                        a.stream.is_empty().then(|| t("diagrams.ai.waiting")),
                        !a.stream.is_empty(),
                        k,
                        reduce,
                    ))
                    .when(!a.stream.is_empty(), |d| {
                        d.child(
                            div()
                                .id("dgm-ai-stream")
                                .tag("dgm-ai-stream")
                                .max_h(px(420.))
                                .overflow_y_scroll()
                                .track_scroll(&a.stream_scroll)
                                .px(px(16.))
                                .py(px(14.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.border)
                                .bg(k.surface_2)
                                .font_family(crate::markdown::MONO)
                                .text_size(px(12.5))
                                .line_height(relative(1.65))
                                .text_color(k.text)
                                .child(a.stream.clone()),
                        )
                    }),
            ),
            Step::Preview => body.child(self.diagram_ai_result(a, has_existing, k, cx)),
        };

        let modal = crate::dialog::shell_fixed(
            k,
            820.,
            t("diagrams.ai.title"),
            body.into_any_element(),
            left,
            right,
            cx.listener(close_ai),
        );
        let confirm = a.confirm_close.then(|| {
            crate::dialog::shell(
                k,
                440.,
                t("common.aiDiscard.title"),
                div()
                    .child(t(if a.step == Step::Loading {
                        "common.aiDiscard.bodyRunning"
                    } else {
                        "common.aiDiscard.body"
                    }))
                    .into_any_element(),
                vec![
                    button("dgm-ai-keep", k, false, false)
                        .child(t("common.aiDiscard.keep"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(a) = this.diagrams.ai.as_mut() {
                                a.confirm_close = false;
                            }
                            cx.notify();
                        })),
                    danger_button("dgm-ai-discard", k)
                        .child(t("common.aiDiscard.discard"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.discard_diagram_ai();
                            cx.notify();
                        })),
                ],
                cx.listener(|this, _, _, cx| {
                    if let Some(a) = this.diagrams.ai.as_mut() {
                        a.confirm_close = false;
                    }
                    cx.notify();
                }),
            )
        });
        Some(div().absolute().inset_0().child(modal).children(confirm))
    }

    fn diagram_ai_fields(&self, a: &DgmAi, k: &Tokens, cx: &mut Context<Self>) -> Vec<Div> {
        let boxed = |s: &Entity<TextareaState>, mono: bool| {
            div()
                .rounded(px(4.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .px(px(3.))
                .py(px(7.))
                .child(
                    Textarea::new(s)
                        .appearance(false)
                        .small()
                        .when(mono, |d| d.font_family(crate::markdown::MONO))
                        .text_size(px(13.))
                        .line_height(relative(1.6)),
                )
        };
        // ddlHint — 문장 사이에 끼운 표기 둘은 코드처럼 바탕을 깐다
        let parts = [
            (t("diagrams.ai.ddlHint1"), false),
            ("varchar?".to_string(), true),
            (t("diagrams.ai.ddlHint2"), false),
            ("(enc)".to_string(), true),
            (t("diagrams.ai.ddlHint3"), false),
        ];
        let mut text = String::new();
        let mut marks = Vec::new();
        for (p, code) in parts {
            if code {
                marks.push((
                    text.len()..text.len() + p.len(),
                    HighlightStyle {
                        background_color: Some(k.surface_3),
                        color: Some(k.text_2),
                        ..Default::default()
                    },
                ));
            }
            text.push_str(&p);
        }
        let ddl_hint = div()
            .mt(px(6.))
            .text_size(px(12.))
            .text_color(k.text_3)
            .child(StyledText::new(text).with_highlights(marks));
        let n = a.chosen.len();
        let chips = PRESETS.iter().enumerate().map(|(i, key)| {
            choice_chip(
                SharedString::from(format!("dgm-ai-chip-{i}")),
                t(key),
                a.chosen.contains(&i),
                "plus",
                k,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(a) = this.diagrams.ai.as_mut() {
                    match a.chosen.iter().position(|x| *x == i) {
                        Some(p) => {
                            a.chosen.remove(p);
                        }
                        None => a.chosen.push(i),
                    }
                }
                cx.notify();
            }))
        });
        vec![
            div()
                .mb(px(16.))
                .child(field_label(t("diagrams.ai.ddlLabel"), k))
                .child(boxed(&a.ddl, true).tag("dgm-ai-ddl"))
                .child(ddl_hint),
            div()
                .mb(px(16.))
                .child(field_label(t("diagrams.ai.instrLabel"), k))
                .child(boxed(&a.instr, false)),
            div()
                .child(field_label(t("diagrams.ai.presetsLabel"), k))
                .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                .when(n > 0, |d| {
                    d.child(hint(
                        t_with("common.ai.chosenCount", &[("n", &n.to_string())]),
                        k,
                    ))
                }),
        ]
    }

    fn diagram_ai_result(
        &self,
        a: &DgmAi,
        has_existing: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let tab = |id: &'static str, label: String, v: View| {
            seg_tab(id, label, a.view == v, k).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(a) = this.diagrams.ai.as_mut() {
                    a.view = v;
                }
                cx.notify();
            }))
        };
        let tabs = segmented(k)
            .child(tab(
                "dgm-ai-tab-diagram",
                t("diagrams.ai.tabDiagram"),
                View::Diagram,
            ))
            .child(tab(
                "dgm-ai-tab-source",
                t("diagrams.ai.tabSource"),
                View::Source,
            ))
            .when(has_existing, |d| {
                d.child(tab("dgm-ai-tab-diff", t("diagrams.ai.tabDiff"), View::Diff))
            });
        let content = match a.view {
            View::Source => a
                .source
                .as_ref()
                .map(|s| {
                    div()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(k.border_strong)
                        .bg(k.surface)
                        .px(px(3.))
                        .py(px(7.))
                        .child(
                            Textarea::new(s)
                                .appearance(false)
                                .small()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(13.))
                                .line_height(relative(1.6)),
                        )
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            View::Diff => a
                .diff
                .as_ref()
                .map(|(_, d)| diff_view(d, k).into_any_element())
                .unwrap_or_else(|| div().into_any_element()),
            // 노트 안 mermaid 그림과 같은 흰 판
            // 노트 안 mermaid 그림과 같은 카드(누르면 확대)
            View::Diagram => match a.fig.as_ref().map(|(_, f)| f) {
                Some(Ok(fig)) => self
                    .mermaid_card("dgm-ai-preview", fig, 740., k, cx)
                    .tag("dgm-ai-preview")
                    .into_any_element(),
                Some(Err(e)) => self
                    .mermaid_fail("dgm-ai", e, &a.result, k, cx)
                    .into_any_element(),
                None => div().into_any_element(),
            },
        };
        let apply = t("diagrams.ai.apply");
        let rest = t("diagrams.ai.applyHint");
        let hint_el = StyledText::new(format!("{apply}{rest}")).with_highlights([(
            0..apply.len(),
            HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            },
        )]);
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .mb(px(5.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text_2)
                            .child(t("diagrams.ai.resultLabel")),
                    )
                    .child(tabs),
            )
            .child(content)
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(hint_el),
            )
    }
}
