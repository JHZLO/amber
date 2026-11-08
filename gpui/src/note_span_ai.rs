// 노트의 일부만 AI 로 고쳐 쓰는 모달 — 지금 앱 NoteSpanAiModal.tsx. 전문 재작성과 나뉜 이유는 비용이다: 출력이
// 순차 생성이라 길이가 곧 대기 시간이다. 노트 전문은 참고 입력으로만 보내고 조각만 받아 원래 자리에 끼운다.
//
// 두 길로 들어온다:
//   selection — 편집 중 원문 칸에서 고른 구간(바이트 = 소스 좌표라 되끼울 자리를 찾을 필요가 없다)
//   section   — 제목으로 나눈 절(note_ai_text::split_sections). 여러 개 고를 수 있고, 붙은 것끼리 한 덩어리로
//               묶어 한 번에 고친다(이음새 문장이 어긋나지 않게). 떨어진 묶음은 따로 부른다.
// 실행은 스토어(note_ai.rs)가 전문 작성과 같은 자리에서 돌린다 — 닫아도 끝까지 간다.

use std::collections::HashMap;
use std::rc::Rc;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::line_diff::{Row, diff_rows};
use amber_gpui::note_ai_text::{merge_runs, split_sections};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::note_ai::{Kind, Phase, Span, StartSpan};
use crate::note_ai_view::{
    Note, ai_thinking, choice_chip, diff_view, field_label, hash2, hint, load_recent, note_box,
    ref_dir_name, run_wait_line,
};
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanMode {
    Selection,
    Section,
}

pub struct SpanModal {
    pub path: String,
    pub mode: SpanMode,
    /// 절 고르기 화면인가(아니면 지시 화면)
    pub picking: bool,
    pub runs: Vec<Span>,
    /// 고른 절(split_sections 의 번호)
    pub picked: Vec<usize>,
    pub input: Option<Entity<TextareaState>>,
    pub seed: Option<String>,
    pub recent: Vec<String>,
    pub ref_on: Vec<String>,
    pub revise: bool,
    pub confirm_discard: bool,
    /// 검토: 변경점(false) / 원문(true)
    pub source_view: bool,
    sources: Vec<Entity<TextareaState>>,
    stream_scroll: ScrollHandle,
    stream_len: usize,
    diffs: HashMap<usize, (u64, Rc<(Vec<Row>, usize, usize)>)>,
}

/// toLocaleString — 세 자리마다 쉼표(ko, en 같다)
fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 글자 수 — 지금 앱은 JS 문자열 길이(UTF-16)로 센다
fn chars(body: &str, start: usize, end: usize) -> usize {
    body.get(start..end).map_or(0, |s| s.encode_utf16().count())
}

/// 고른 절 → 붙은 것끼리 묶은 덩어리. 묶인 절이 여럿이면 '처음 … 끝'
fn runs_from_picked(body: &str, picked: &[usize]) -> Vec<Span> {
    let sections = split_sections(body);
    let chosen: Vec<_> = picked.iter().filter_map(|i| sections.get(*i)).collect();
    let spans: Vec<(usize, usize)> = chosen.iter().map(|s| (s.start, s.end)).collect();
    merge_runs(&spans)
        .into_iter()
        .map(|(start, end, items)| {
            let first = &chosen[items[0]].title;
            let label = if items.len() == 1 {
                first.clone()
            } else {
                format!("{first} \u{2026} {}", chosen[*items.last().unwrap()].title)
            };
            Span {
                section: true,
                start,
                end,
                label,
            }
        })
        .collect()
}

impl SpanModal {
    /// 닫기 전에 묻기(close_dirty)가 세는 입력칸 — 지시와 원문 조각 칸
    pub(crate) fn field_ids(&self) -> Vec<EntityId> {
        self.input
            .iter()
            .map(|i| i.entity_id())
            .chain(self.sources.iter().map(|s| s.entity_id()))
            .collect()
    }
}

impl Spike {
    /// 열 때마다 — 이 노트에 부분 수정 실행이 남아 있으면 그때의 구간, 지시, 폴더를 되살리고, 없으면 새로
    /// (selection 은 받은 구간으로 바로, section 은 고르는 화면부터)
    pub(crate) fn open_span_modal(
        &mut self,
        mode: SpanMode,
        sel: Option<std::ops::Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.notes.selected.clone() else {
            return;
        };
        let prior = self
            .notes
            .ai
            .get(&path)
            .filter(|r| r.kind == Kind::Span)
            .cloned();
        let (runs, picking, seed, ref_on) = match prior {
            Some(r) => (r.spans.clone(), false, r.typed.clone(), r.ref_dirs.clone()),
            None => match sel.filter(|s| s.end > s.start) {
                Some(s) if mode == SpanMode::Selection => (
                    vec![Span {
                        section: false,
                        start: s.start,
                        end: s.end,
                        label: t("notes.spanAi.selectionLabel"),
                    }],
                    false,
                    String::new(),
                    Vec::new(),
                ),
                _ => (Vec::new(), true, String::new(), Vec::new()),
            },
        };
        self.notes.span_modal = Some(SpanModal {
            path,
            mode,
            picking,
            runs,
            picked: Vec::new(),
            input: None,
            seed: Some(seed),
            recent: load_recent(&self.db),
            ref_on,
            revise: false,
            confirm_discard: false,
            source_view: false,
            sources: Vec::new(),
            stream_scroll: ScrollHandle::new(),
            stream_len: 0,
            diffs: HashMap::new(),
        });
        cx.notify();
    }

    /// render 에서 — 지시 칸(⌘↩ 로 보낸다), 원문 탭의 조각 칸들
    pub(crate) fn apply_span_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.notes.span_modal.as_mut() else {
            return;
        };
        if let Some(v) = m.seed.take() {
            let input = cx.new(|cx| {
                let mut s = TextareaState::new(window, cx)
                    .auto_grow(3, 8)
                    .placeholder(t("notes.spanAi.instructionPh"));
                s.set_value(v, window, cx);
                s
            });
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            // ⌘↩ 로 보낸다 — 입력칸이 Enter { secondary } 로 받아 PressEnter 로 알린다
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
                if let InputEvent::PressEnter {
                    secondary: true, ..
                } = ev
                {
                    this.run_span(cx);
                    cx.notify();
                }
            })
            .detach();
            // 절 고르기 화면에서는 칸이 그려지지 않는다 — 거기에 포커스를 두면 Esc 와 ⌘ 키가 갈 곳이 없다
            if !m.picking {
                input.update(cx, |s, cx| s.focus(window, cx));
            }
            m.input = Some(input);
        }
        let path = m.path.clone();
        let results = self
            .notes
            .ai
            .get(&path)
            .filter(|r| r.kind == Kind::Span && r.phase != Phase::Running)
            .map(|r| r.span_results.clone());
        let Some(m) = self.notes.span_modal.as_mut() else {
            return;
        };
        match (m.source_view, results) {
            (true, Some(results)) if m.sources.len() != m.runs.len() => {
                let rows = if m.runs.len() > 1 { 8 } else { 14 };
                let mut sources = Vec::new();
                for i in 0..m.runs.len() {
                    let v = results.get(i).cloned().unwrap_or_default();
                    let src = cx.new(|cx| {
                        let mut s = TextareaState::new(window, cx).auto_grow(rows, rows);
                        s.set_value(v, window, cx);
                        s
                    });
                    let p = path.clone();
                    // 조각 하나를 손으로 고쳤다 — 전문도 다시 끼운다(setNoteSpanResult)
                    cx.observe(&src, move |this, src, cx| {
                        let v = src.read(cx).value().to_string();
                        let cur = this
                            .notes
                            .ai
                            .get(&p)
                            .and_then(|r| r.span_results.get(i).cloned());
                        if cur.as_deref() != Some(v.as_str()) {
                            this.notes.ai.set_span_result(&p, i, v);
                            cx.notify();
                        }
                    })
                    .detach();
                    sources.push(src);
                }
                m.sources = sources;
            }
            (false, _) => m.sources.clear(),
            _ => {}
        }
    }

    fn span_instruction(&self, cx: &App) -> String {
        self.notes
            .span_modal
            .as_ref()
            .and_then(|m| m.input.as_ref())
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn run_span(&mut self, cx: &mut Context<Self>) {
        let instruction = self.span_instruction(cx);
        let Some(m) = self.notes.span_modal.as_ref() else {
            return;
        };
        if m.runs.is_empty() || instruction.trim().encode_utf16().count() < 2 {
            return;
        }
        let p = StartSpan {
            path: m.path.clone(),
            title: crate::notes_view::file_name(&m.path).to_string(),
            markdown: self.ai_current_body(cx),
            spans: m.runs.clone(),
            instruction,
            ref_dirs: m
                .recent
                .iter()
                .filter(|d| m.ref_on.contains(d))
                .cloned()
                .collect(),
        };
        if let Some(m) = self.notes.span_modal.as_mut() {
            m.revise = false;
        }
        self.start_span_ai(p, cx);
    }

    fn apply_span(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.notes.span_modal.as_ref().map(|m| m.path.clone()) else {
            return;
        };
        let Some(md) = self.notes.ai.get(&path).and_then(|r| r.result.clone()) else {
            return;
        };
        self.notes.pending_apply = Some(md);
        self.dismiss_note_ai(&path);
        self.notes.span_modal = None;
        cx.notify();
    }

    /// 실행을 시작한 모달로 되돌아간다 — 부분 수정 결과를 전문 작성 검토에 띄우면 맞지 않는다
    pub(crate) fn reopen_note_ai(&mut self, cx: &mut Context<Self>) {
        let run = self
            .notes
            .selected
            .as_ref()
            .and_then(|p| self.notes.ai.get(p))
            .cloned();
        match run {
            Some(r) if r.kind == Kind::Span => {
                let mode = if r.spans.first().is_some_and(|s| !s.section) {
                    SpanMode::Selection
                } else {
                    SpanMode::Section
                };
                self.open_span_modal(mode, None, cx);
            }
            _ => self.open_ai_modal(cx),
        }
    }

    pub(crate) fn span_modal_view(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let path = self.notes.span_modal.as_ref()?.path.clone();
        let run = self.notes.ai.get(&path).cloned();
        let span_run = run.clone().filter(|r| r.kind == Kind::Span);
        let body = self.ai_current_body(cx);
        let connected = self.ai_connected();
        let reduce = cx.reduce_motion();
        let instruction = self.span_instruction(cx);
        let m = self.notes.span_modal.as_mut()?;
        #[derive(PartialEq, Clone, Copy)]
        enum Step {
            Pick,
            Prompt,
            Loading,
            Preview,
        }
        let stage = if m.picking { Step::Pick } else { Step::Prompt };
        let step = match &span_run {
            None => stage,
            Some(_) if m.revise => stage,
            Some(r) if r.phase == Phase::Running => Step::Loading,
            Some(r) if r.phase == Phase::Done || r.result.is_some() => Step::Preview,
            _ => stage,
        };
        let error = span_run
            .as_ref()
            .filter(|r| !m.revise && r.phase == Phase::Error)
            .and_then(|r| r.error.clone());
        let compose_busy = run
            .as_ref()
            .is_some_and(|r| r.kind == Kind::Compose && r.phase == Phase::Running);
        let stream = span_run
            .as_ref()
            .map(|r| r.stream.clone())
            .unwrap_or_default();
        if step == Step::Loading && stream.len() != m.stream_len {
            m.stream_len = stream.len();
            m.stream_scroll.scroll_to_bottom();
        }
        // 검토는 시작한 때의 본문과 견준다 — 그 사이 노트를 고쳐도 변경점이 흔들리지 않게
        let base = span_run
            .as_ref()
            .map(|r| r.base.clone())
            .unwrap_or_else(|| body.clone());
        let results = span_run
            .as_ref()
            .map(|r| r.span_results.clone())
            .unwrap_or_default();
        let mut diffs: Vec<Option<Rc<(Vec<Row>, usize, usize)>>> = Vec::new();
        if step == Step::Preview && !m.source_view {
            for (i, r) in m.runs.clone().iter().enumerate() {
                let old = base.get(r.start..r.end).unwrap_or_default();
                let new = results.get(i).map(String::as_str).unwrap_or_default();
                let h = hash2(old, new);
                if m.diffs.get(&i).is_none_or(|(x, _)| *x != h) {
                    m.diffs.insert(i, (h, Rc::new(diff_rows(old, new))));
                }
                diffs.push(m.diffs.get(&i).map(|(_, d)| d.clone()));
            }
        }
        let m = self.notes.span_modal.as_ref()?;
        let (mode, revise, confirm_discard, source_view) =
            (m.mode, m.revise, m.confirm_discard, m.source_view);
        let runs = m.runs.clone();
        let picked = m.picked.clone();
        let too_short = instruction.trim().encode_utf16().count() < 2;
        let run_at = span_run.as_ref().map_or(0, |r| r.span_at);

        let mut left: Vec<Stateful<Div>> = Vec::new();
        let mut right: Vec<Stateful<Div>> = Vec::new();
        let close = || {
            cx.listener(|this, _, _, cx| {
                this.notes.span_modal = None;
                cx.notify();
            })
        };
        match step {
            Step::Pick => {
                right.push(
                    button("span-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(close()),
                );
                right.push(
                    primary_button("span-next", k, picked.is_empty())
                        .child(t("notes.spanAi.next"))
                        .child(icon("chevron-right", 14.).text_color(k.text))
                        .when(!picked.is_empty(), |d| {
                            d.on_click(cx.listener(|this, _, window, cx| {
                                let body = this.ai_current_body(cx);
                                if let Some(m) = this.notes.span_modal.as_mut() {
                                    m.runs = runs_from_picked(&body, &m.picked);
                                    m.picking = false;
                                    // 칸이 이미 있으면 그대로(친 지시를 지우지 않는다), 없으면 다음 그림에서 만든다
                                    match &m.input {
                                        Some(input) => {
                                            input.update(cx, |s, cx| s.focus(window, cx))
                                        }
                                        None => {
                                            m.seed.get_or_insert_default();
                                        }
                                    }
                                }
                                cx.notify();
                            }))
                        }),
                );
            }
            Step::Prompt => {
                if revise && span_run.as_ref().is_some_and(|r| r.result.is_some()) {
                    left.push(
                        button("span-back-result", k, false, false)
                            .child(icon("chevron-left", 14.).text_color(k.text_2))
                            .child(t("notes.ai.backToResult"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(m) = this.notes.span_modal.as_mut() {
                                    m.revise = false;
                                }
                                cx.notify();
                            })),
                    );
                } else if mode == SpanMode::Section {
                    left.push(
                        button("span-back-pick", k, false, false)
                            .child(icon("chevron-left", 14.).text_color(k.text_2))
                            .child(t("notes.spanAi.backToPick"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(m) = this.notes.span_modal.as_mut() {
                                    m.picking = true;
                                }
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    );
                }
                right.push(
                    button("span-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(close()),
                );
                let off = too_short || !connected || runs.is_empty() || compose_busy;
                right.push(
                    primary_button("span-run", k, off)
                        .child(icon("sparkles", 15.).text_color(k.text))
                        .child(t("notes.spanAi.run"))
                        .when(!off, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| {
                                this.run_span(cx);
                                cx.notify();
                            }))
                        }),
                );
            }
            Step::Loading => {
                let p = path.clone();
                left.push(
                    crate::day_card::danger_button("span-stop", k)
                        .child(icon("x", 14.).text_color(k.danger))
                        .child(t("notes.ai.stop"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.stop_note_ai(&p);
                            cx.notify();
                        })),
                );
                right.push(
                    button("span-keep", k, false, false)
                        .child(t("notes.ai.bg.keep"))
                        .on_click(close()),
                );
            }
            Step::Preview => {
                left.push(
                    button("span-revise", k, false, false)
                        .child(icon("chevron-left", 14.).text_color(k.text_2))
                        .child(t("notes.ai.back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.notes.span_modal.as_mut() {
                                m.revise = true;
                            }
                            cx.notify();
                        })),
                );
                left.push(
                    crate::day_card::danger_button("span-discard", k)
                        .child(t("notes.ai.discard"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.notes.span_modal.as_mut() {
                                m.confirm_discard = true;
                            }
                            cx.notify();
                        })),
                );
                right.push(
                    button("span-close", k, false, false)
                        .child(t("common.close"))
                        .on_click(close()),
                );
                let none = !results.iter().any(|r| !r.trim().is_empty());
                right.push(
                    primary_button("span-apply", k, none)
                        .child(icon("check", 15.).text_color(k.text))
                        .child(t("notes.spanAi.apply"))
                        .when(!none, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.apply_span(cx)))
                        }),
                );
            }
        }

        let mut out = div().flex().flex_col();
        if let Some(e) = error {
            out = out.child(note_box(Note::Error, k).mb(px(12.)).child(e));
        }
        if compose_busy {
            out = out.child(
                note_box(Note::Warn, k)
                    .mb(px(12.))
                    .child(t("notes.spanAi.composeBusy")),
            );
        }
        let label_row = |text: String, right: Option<AnyElement>| {
            div()
                .flex()
                .items_center()
                .mb(px(5.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(k.text_2)
                        .child(text),
                )
                .children(right)
        };
        let small_hint = |text: String| div().text_size(px(12.)).text_color(k.text_3).child(text);
        let run_head = |r: &Span, size: Option<usize>| {
            div()
                .flex()
                .items_baseline()
                .gap(px(8.))
                .mb(px(4.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(k.text_2)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(r.label.clone()),
                )
                .when_some(size, |d, n| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .font_family(crate::markdown::MONO)
                            .text_size(px(10.5))
                            .text_color(k.text_3)
                            .child(t_with("notes.spanAi.chars", &[("n", &fmt_num(n))])),
                    )
                })
        };
        match step {
            Step::Pick => {
                let sections = split_sections(&body);
                let picked_chars: usize = picked
                    .iter()
                    .filter_map(|i| sections.get(*i))
                    .map(|s| chars(&body, s.start, s.end))
                    .sum();
                let count = (!picked.is_empty()).then(|| {
                    small_hint(t_with(
                        "notes.spanAi.pickCount",
                        &[
                            ("n", &picked.len().to_string()),
                            ("chars", &fmt_num(picked_chars)),
                        ],
                    ))
                    .into_any_element()
                });
                let mut field = div().child(label_row(t("notes.spanAi.pickLabel"), count));
                if sections.is_empty() {
                    field = field.child(small_hint(t("notes.spanAi.noSections")));
                } else {
                    let rows = sections.iter().enumerate().map(|(i, s)| {
                        let on = picked.contains(&i);
                        let hover = k.surface_2;
                        div()
                            .id(SharedString::from(format!("span-pick-{i}")))
                            .tag(format!("span-pick-{i}"))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .w_full()
                            .pr(px(12.))
                            .py(px(8.))
                            .pl(px(match s.level {
                                2 => 28.,
                                3 => 44.,
                                _ => 12.,
                            }))
                            .when(i > 0, |d| d.border_t_1().border_color(k.border))
                            .cursor_pointer()
                            .when(on, |d| d.bg(k.surface_3))
                            .when(!on, |d| d.hover(move |st| st.bg(hover)))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .size(px(14.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(on, |d| d.bg(k.text).border_color(k.text))
                                    .when(!on, |d| d.border_color(k.border_strong))
                                    .when(on, |d| {
                                        d.child(icon("check", 11.).text_color(k.surface))
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .text_color(k.text)
                                    .when(s.level == 1, |d| d.font_weight(FontWeight::SEMIBOLD))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(s.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(crate::markdown::MONO)
                                    .text_size(px(10.5))
                                    .text_color(k.text_3)
                                    .child(t_with(
                                        "notes.spanAi.chars",
                                        &[("n", &fmt_num(chars(&body, s.start, s.end)))],
                                    )),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(m) = this.notes.span_modal.as_mut() {
                                    if let Some(x) = m.picked.iter().position(|p| *p == i) {
                                        m.picked.remove(x);
                                    } else {
                                        m.picked.push(i);
                                    }
                                }
                                cx.notify();
                            }))
                    });
                    field = field
                        .child(
                            div()
                                .id("span-pick")
                                .max_h(px(340.))
                                .overflow_y_scroll()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.border)
                                .children(rows),
                        )
                        .child(hint(t("notes.spanAi.pickHint"), k));
                }
                out = out.child(field);
            }
            Step::Prompt | Step::Loading if !runs.is_empty() => {
                let total: usize = runs.iter().map(|r| chars(&body, r.start, r.end)).sum();
                let label = if runs.len() == 1 {
                    runs[0].label.clone()
                } else {
                    t_with("notes.spanAi.runsLabel", &[("n", &runs.len().to_string())])
                };
                let mut field = div().mb(px(16.)).child(label_row(
                    label,
                    Some(
                        small_hint(t_with("notes.spanAi.chars", &[("n", &fmt_num(total))]))
                            .into_any_element(),
                    ),
                ));
                for (i, r) in runs.iter().enumerate() {
                    let mut block = div().when(i > 0, |d| d.mt(px(12.)));
                    if runs.len() > 1 {
                        block = block.child(run_head(r, Some(chars(&body, r.start, r.end))));
                    }
                    block = block.child(
                        div()
                            .id(("span-source", i))
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .px(px(12.))
                            .py(px(10.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(k.border)
                            .bg(k.surface_2)
                            .font_family(crate::markdown::MONO)
                            .text_size(px(12.))
                            .line_height(relative(1.6))
                            .text_color(k.text_2)
                            .child(body.get(r.start..r.end).unwrap_or_default().to_string()),
                    );
                    if step == Step::Loading && i == run_at {
                        let label = if runs.len() > 1 {
                            t_with(
                                "notes.spanAi.progress",
                                &[
                                    ("i", &(run_at + 1).to_string()),
                                    ("n", &runs.len().to_string()),
                                ],
                            )
                        } else {
                            t("notes.spanAi.editing")
                        };
                        let wait = span_run.as_ref().map(run_wait_line);
                        block = block.child(
                            div()
                                .mt(px(10.))
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(ai_thinking("span-bar", label, wait, true, k, reduce))
                                .when(!stream.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .id("span-stream")
                                            .tag("span-stream")
                                            .max_h(px(420.))
                                            .overflow_y_scroll()
                                            .track_scroll(&m.stream_scroll)
                                            .px(px(16.))
                                            .py(px(14.))
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(k.border)
                                            .bg(k.surface_2)
                                            .font_family(crate::markdown::MONO)
                                            .text_size(px(12.5))
                                            .line_height(relative(1.65))
                                            .child(stream.clone()),
                                    )
                                }),
                        );
                    }
                    field = field.child(block);
                }
                if runs.len() > 1 && step == Step::Prompt {
                    field = field.child(hint(
                        t_with("notes.spanAi.runsHint", &[("n", &runs.len().to_string())]),
                        k,
                    ));
                }
                if step == Step::Loading {
                    field = field.child(hint(t("notes.ai.bg.hint"), k));
                }
                out = out.child(field);
            }
            _ => {}
        }
        if step == Step::Prompt {
            let input = m.input.as_ref().map(|i| {
                div()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .px(px(3.))
                    .py(px(7.))
                    .child(
                        Textarea::new(i)
                            .appearance(false)
                            .small()
                            .text_size(px(13.))
                            .line_height(relative(1.6)),
                    )
            });
            let dirs = std::iter::once(
                button("span-ref-add", k, false, false)
                    .child(icon("folder-plus", 13.).text_color(k.text_2))
                    .child(t("notes.ai.refDirs.add"))
                    .on_click(cx.listener(|this, _, _, cx| this.pick_ref_dir(cx))),
            )
            .chain(m.recent.iter().map(|d| {
                let dir = d.clone();
                choice_chip(
                    SharedString::from(format!("span-ref-{d}")),
                    ref_dir_name(d),
                    m.ref_on.contains(d),
                    "folder",
                    k,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(m) = this.notes.span_modal.as_mut() {
                        if let Some(i) = m.ref_on.iter().position(|x| *x == dir) {
                            m.ref_on.remove(i);
                        } else {
                            m.ref_on.push(dir.clone());
                        }
                    }
                    cx.notify();
                }))
            }))
            .collect::<Vec<_>>();
            out = out
                .child(
                    div()
                        .mb(px(16.))
                        .child(field_label(t("notes.spanAi.instructionLabel"), k))
                        .children(input)
                        .child(hint(t("notes.spanAi.hint"), k)),
                )
                .child(
                    div()
                        .child(field_label(t("notes.ai.refDirs.label"), k))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(6.))
                                .children(dirs),
                        )
                        .child(hint(t("notes.ai.refDirs.hint"), k)),
                );
        }
        if step == Step::Preview {
            let r = span_run.as_ref()?;
            if r.result.is_some() && r.base != body {
                out = out.child(
                    note_box(Note::Warn, k)
                        .mb(px(12.))
                        .child(t("notes.spanAi.baseChanged")),
                );
            }
            if r.truncated {
                out = out.child(
                    note_box(Note::Warn, k)
                        .mb(px(12.))
                        .child(t("notes.ai.truncated")),
                );
            }
            let tab = |id: &'static str, label: String, src: bool| {
                let active = source_view == src;
                let hover = crate::note_ai_view::glass(k.glass_fill);
                let t2 = k.text;
                div()
                    .id(id)
                    .tag(id)
                    .px(px(12.))
                    .py(px(5.))
                    .rounded(px(4.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .when(active, |d| d.bg(k.glass_raise).text_color(k.text))
                    .when(!active, |d| {
                        d.text_color(k.text_2)
                            .hover(move |s| s.bg(hover).text_color(t2))
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = this.notes.span_modal.as_mut() {
                            m.source_view = src;
                        }
                        cx.notify();
                    }))
            };
            let segmented = div()
                .flex()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(7.))
                .border_1()
                .border_color(k.border)
                .bg(k.surface_2)
                .child(tab("span-tab-diff", t("notes.ai.tabDiff"), false))
                .child(tab("span-tab-source", t("notes.ai.tabSource"), true));
            let mut field = div().child(label_row(
                t("notes.spanAi.resultLabel"),
                Some(segmented.into_any_element()),
            ));
            for (i, r) in runs.iter().enumerate() {
                let mut block = div().when(i > 0, |d| d.mt(px(12.)));
                if runs.len() > 1 {
                    block = block.child(run_head(r, None));
                }
                block = if source_view {
                    block.children(m.sources.get(i).map(|s| {
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
                    }))
                } else {
                    block.children(diffs.get(i).cloned().flatten().map(|d| diff_view(&d, k)))
                };
                field = field.child(block);
            }
            out = out.child(field.child(hint(t("notes.spanAi.applyHint"), k)));
        }

        let modal = crate::dialog::shell_split(
            k,
            820.,
            t("notes.spanAi.title"),
            out.into_any_element(),
            left,
            right,
            cx.listener(|this, _, _, cx| {
                this.notes.span_modal = None;
                cx.notify();
            }),
        );
        let p = path.clone();
        Some(
            div()
                .absolute()
                .inset_0()
                .child(modal)
                .when(confirm_discard, |d| {
                    d.child(crate::dialog::shell(
                        k,
                        440.,
                        t("common.aiDiscard.title"),
                        div()
                            .child(t("common.aiDiscard.bodyDiscard"))
                            .into_any_element(),
                        vec![
                            button("span-discard-keep", k, false, false)
                                .child(t("common.aiDiscard.keep"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(m) = this.notes.span_modal.as_mut() {
                                        m.confirm_discard = false;
                                    }
                                    cx.notify();
                                })),
                            crate::day_card::danger_button("span-discard-yes", k)
                                .child(t("common.aiDiscard.discardOnly"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.dismiss_note_ai(&p);
                                    if let Some(m) = this.notes.span_modal.as_mut() {
                                        m.confirm_discard = false;
                                        m.revise = false;
                                        m.sources.clear();
                                    }
                                    cx.notify();
                                })),
                        ],
                        cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.notes.span_modal.as_mut() {
                                m.confirm_discard = false;
                            }
                            cx.notify();
                        }),
                    ))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{fmt_num, runs_from_picked};

    #[test]
    fn numbers_and_runs() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(1234), "1,234");
        assert_eq!(fmt_num(1234567), "1,234,567");
        let md = "# A\na\n# B\nb\n# C\nc\n";
        let r = runs_from_picked(md, &[2, 0, 1]);
        assert_eq!(r.len(), 1, "붙은 절은 한 덩어리");
        assert_eq!(r[0].label, "A \u{2026} C");
        assert_eq!((r[0].start, r[0].end), (0, md.len()));
        let r = runs_from_picked(md, &[0, 2]);
        assert_eq!(r.len(), 2, "떨어진 절은 따로");
        assert_eq!(r[1].label, "C");
    }
}
