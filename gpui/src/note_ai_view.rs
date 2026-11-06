// 노트 AI 작성 화면 — 지금 앱 NoteAiModal.tsx(지시 → 백그라운드 실행 → 검토 → 편집 초안으로 적용), NoteAiBanner.tsx
// (노트 위 배너), 트리 행의 점, 레일의 점. 실행 상태는 스토어(note_ai.rs)에 있다 — 닫기는 중단이 아니라 숨기기라
// 묻지 않는다. 파일에 바로 쓰지 않는다: 적용하면 편집 초안이 되고 사용자가 ⌘S 로 저장한다(AI 출력은 초안).
// 아직: 저장 프롬프트 내용 보기(칩의 눈 단추, PromptPeekModal).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::line_diff::{Row, diff_rows};
use amber_gpui::note_ai_text::{compose_instruction, wait_line};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::note_ai::{Kind, Phase, Run, StartCompose};
use crate::theme::Tokens;

const PRESETS: [&str; 5] = [
    "notes.ai.preset1",
    "notes.ai.preset2",
    "notes.ai.preset3",
    "notes.ai.preset4",
    "notes.ai.preset5",
];
/// 최근 참고 폴더(lib/refDirs.ts) — 같은 키, 여섯 개까지
const REF_KEY: &str = "amber.ai.refDirs";
const REF_MAX: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiView {
    Diff,
    Preview,
    Source,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub id: String,
    pub label: String,
    pub text: String,
}

pub struct AiModal {
    pub path: String,
    pub input: Option<Entity<TextareaState>>,
    /// 입력칸을 이 값으로 (다시) 만든다 — 창이 있어야 만들어진다(render)
    pub seed: Option<String>,
    /// 켠 저장 프롬프트(`s:<id>`)와 빠른 지시(`p:<번호>`)
    pub chosen: Vec<String>,
    pub recent: Vec<String>,
    pub ref_on: Vec<String>,
    /// '다시 지시' — 결과가 있어도 지시 화면을 보인다
    pub revise: bool,
    pub confirm_discard: bool,
    pub view: AiView,
    pub saved: Vec<Prompt>,
    pub source: Option<Entity<TextareaState>>,
    /// 마지막으로 본 완료 시각 — 결과가 새로 오면 변경점(기존 노트)이나 미리보기로 돌린다
    seen_finish: Option<i64>,
    stream_scroll: ScrollHandle,
    stream_len: usize,
    diff: Option<(u64, Rc<(Vec<Row>, usize, usize)>)>,
}

/// lib/prompts.ts loadPrompts — settings 의 note_ai_prompts(JSON 배열). 깨진 값은 거른다
pub fn load_prompts(db: &amber_gpui::db::Db) -> Vec<Prompt> {
    let Some(raw) = amber_gpui::prefs::get(db, "note_ai_prompts") else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|p| {
            let id = p.get("id")?.as_str()?;
            let text = p.get("text")?.as_str()?;
            let label = match p.get("label") {
                Some(serde_json::Value::String(s)) => s.clone(),
                None | Some(serde_json::Value::Null) => String::new(),
                Some(v) => v.to_string(),
            };
            Some(Prompt {
                id: id.into(),
                label,
                text: text.into(),
            })
        })
        .collect()
}

pub(crate) fn load_recent(db: &amber_gpui::db::Db) -> Vec<String> {
    amber_gpui::prefs::get(db, REF_KEY)
        .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(&raw).ok())
        .map(|v| {
            v.into_iter()
                .filter_map(|x| x.as_str().filter(|s| !s.is_empty()).map(str::to_string))
                .take(REF_MAX)
                .collect()
        })
        .unwrap_or_default()
}

/// refDirs.pushRecent — 맨 앞에 끼운다(있으면 앞으로)
pub(crate) fn push_recent(list: &[String], dir: &str) -> Vec<String> {
    let d = dir.trim();
    if d.is_empty() {
        return list.to_vec();
    }
    std::iter::once(d.to_string())
        .chain(list.iter().filter(|x| *x != d).cloned())
        .take(REF_MAX)
        .collect()
}

/// refDirs.refDirName — 칩 라벨은 마지막 이름
pub(crate) fn ref_dir_name(path: &str) -> String {
    let p = path.trim_end_matches('/');
    match p.rsplit('/').next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => path.to_string(),
    }
}

pub(crate) fn hash2(a: &str, b: &str) -> u64 {
    let mut h = DefaultHasher::new();
    a.hash(&mut h);
    b.hash(&mut h);
    h.finish()
}

pub(crate) fn glass(pair: (Hsla, Hsla)) -> Background {
    linear_gradient(
        150.,
        linear_color_stop(pair.0, 0.),
        linear_color_stop(pair.1, 1.),
    )
}

/// 결과 알림(.error-note / .ok-note / .warn-note) — 왼쪽 바 + 옅은 바탕
#[derive(Clone, Copy)]
pub enum Note {
    Error,
    Ok,
    Warn,
    Info,
}

pub fn note_box(kind: Note, k: &Tokens) -> Div {
    let (bg, fg, bar) = match kind {
        Note::Error => (k.danger_soft, k.danger_text, k.danger),
        Note::Ok => (k.success_soft, k.success_text, k.success),
        Note::Warn => (k.surface_2, k.text, k.warning),
        Note::Info => (k.surface_2, k.text, k.text),
    };
    div()
        .px(px(12.))
        .py(px(10.))
        .rounded(px(4.))
        .border_l(px(3.))
        .border_color(bar)
        .bg(bg)
        .text_color(fg)
        .text_size(px(13.))
}

pub(crate) fn field_label(text: String, k: &Tokens) -> Div {
    div()
        .mb(px(5.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(k.text_2)
        .child(text)
}

pub(crate) fn hint(text: String, k: &Tokens) -> Div {
    div()
        .mt(px(6.))
        .text_size(px(12.))
        .text_color(k.text_3)
        .child(text)
}

/// .chip.chip-choice — 누르면 '요청에 포함'만 켠다. 켜짐 = 결과 색 채움 + 체크
pub(crate) fn choice_chip(
    id: SharedString,
    label: String,
    on: bool,
    icon_name: &'static str,
    k: &Tokens,
) -> Stateful<Div> {
    let (hover_fill, hover_edge) = (glass(k.glass_fill_hover), k.glass_edge_hover);
    div()
        .id(id.clone())
        .tag(id)
        .flex()
        .items_center()
        .gap(px(5.))
        .max_w_full()
        .pl(px(8.))
        .pr(px(10.))
        .py(px(4.))
        .rounded(px(4.))
        .border_1()
        .text_size(px(12.))
        .line_height(relative(1.4))
        .cursor_pointer()
        .when(on, |d| {
            d.bg(k.success_soft)
                .border_color(k.success)
                .text_color(k.success_text)
        })
        .when(!on, |d| {
            d.bg(glass(k.glass_fill))
                .border_color(k.glass_edge)
                .text_color(k.text)
                .hover(move |s| s.bg(hover_fill).border_color(hover_edge))
        })
        .child(
            icon(if on { "check" } else { icon_name }, 12.)
                .flex_shrink_0()
                .text_color(if on { k.success } else { k.text_3 }),
        )
        .child(
            div()
                .max_w(px(260.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

/// .ai-thinking(.compact) — 라벨, 쓸고 지나가는 막대 하나, 진행 한 줄. 모션은 막대 하나뿐
pub fn ai_thinking(
    id: &'static str,
    label: String,
    activity: Option<String>,
    compact: bool,
    k: &Tokens,
    reduce: bool,
) -> Div {
    let track = div()
        .relative()
        .w_full()
        .when(!compact, |d| d.max_w(px(240.)))
        .h(px(3.))
        .rounded_full()
        .bg(k.surface_3)
        .overflow_hidden();
    let bar = div()
        .absolute()
        .top_0()
        .bottom_0()
        .rounded_full()
        .bg(k.text);
    let track = if reduce {
        track.child(bar.left_0().w_full().opacity(0.4))
    } else {
        track.child(bar.w(relative(0.4)).with_animation(
            id,
            Animation::new(Duration::from_millis(1150)).repeat(),
            // translateX(-100%) → (250%) of a 40% bar = left -40% → +100%
            |b, d| b.left(relative(-0.4 + 1.4 * d)),
        ))
    };
    div()
        .flex()
        .flex_col()
        .when(compact, |d| {
            d.items_stretch().gap(px(6.)).pt(px(2.)).pb(px(4.))
        })
        .when(!compact, |d| {
            d.items_center().gap(px(12.)).py(px(36.)).px(px(24.))
        })
        .child(
            div()
                .text_size(px(if compact { 12.5 } else { 13. }))
                .font_weight(if compact {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::SEMIBOLD
                })
                .text_color(if compact { k.text_3 } else { k.text_2 })
                .child(label),
        )
        .child(track)
        .when_some(activity, |d, a| {
            d.child(
                div()
                    .max_w_full()
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(a),
            )
        })
}

pub(crate) fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

pub(crate) fn run_wait_line(run: &Run) -> String {
    let act = run
        .activity
        .as_ref()
        .map(|(tool, target)| (tool.as_str(), target.as_deref()));
    wait_line(
        run.started_at,
        act,
        run.activity_at,
        !run.ref_dirs.is_empty(),
        now_ms(),
    )
}

impl Spike {
    /// 지금 보이는 본문 — 편집 중이면 초안
    pub(crate) fn ai_current_body(&self, cx: &App) -> String {
        self.note_draft(cx)
            .unwrap_or_else(|| self.notes.body.clone())
    }

    pub(crate) fn ai_connected(&self) -> bool {
        amber_gpui::ai_config::load_config(&self.db)
            .ok()
            .is_some_and(|c| c.provider.is_some())
    }

    /// 열 때마다 지시 화면을 되살린다 — 이 노트에 실행이 있으면 그때 보낸 지시, 칩, 폴더를
    pub(crate) fn open_ai_modal(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.notes.selected.clone() else {
            return;
        };
        let cur = self.notes.ai.get(&path).cloned();
        let has_existing = !self.ai_current_body(cx).trim().is_empty();
        self.notes.ai_modal = Some(AiModal {
            path,
            input: None,
            seed: Some(cur.as_ref().map(|r| r.typed.clone()).unwrap_or_default()),
            chosen: cur.as_ref().map(|r| r.chosen.clone()).unwrap_or_default(),
            recent: load_recent(&self.db),
            ref_on: cur.as_ref().map(|r| r.ref_dirs.clone()).unwrap_or_default(),
            revise: false,
            confirm_discard: false,
            view: if cur.as_ref().is_some_and(|r| r.result.is_some()) && has_existing {
                AiView::Diff
            } else {
                AiView::Preview
            },
            saved: load_prompts(&self.db),
            source: None,
            seen_finish: cur.and_then(|r| r.finished_at),
            stream_scroll: ScrollHandle::new(),
            stream_len: 0,
            diff: None,
        });
        cx.notify();
    }

    /// render 에서 — 지시 입력칸, 원문 탭 입력칸을 만든다. 적용한 결과를 편집 초안으로
    pub(crate) fn apply_ai_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(md) = self.notes.pending_apply.take() {
            if self.notes.edit.is_none() {
                self.start_note_edit(window, cx);
            }
            if let Some(e) = self.notes.edit.as_ref() {
                let state = e.state.clone();
                state.update(cx, |s, cx| s.set_value(md.clone(), window, cx));
                let text = crate::hex(Tokens::get(cx).text);
                let (preview, _) = crate::notes_view::build_blocks(&md, &text, cx);
                let dirty = md != self.notes.body;
                if let Some(e) = self.notes.edit.as_mut() {
                    e.preview = preview;
                    e.dirty = dirty;
                }
            }
        }
        let Some(m) = self.notes.ai_modal.as_mut() else {
            return;
        };
        if let Some(v) = m.seed.take() {
            let input = cx.new(|cx| {
                let mut s = TextareaState::new(window, cx)
                    .auto_grow(4, 10)
                    .placeholder(t("notes.ai.instructionPh"));
                s.set_value(v, window, cx);
                s
            });
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            input.update(cx, |s, cx| s.focus(window, cx));
            m.input = Some(input);
        }
        // 원문 탭 — 손으로 고친 결과는 스토어에 남는다(setNoteAiResult)
        let want_source = m.view == AiView::Source;
        if want_source && m.source.is_none() {
            let md = self
                .notes
                .ai
                .get(&m.path)
                .and_then(|r| r.result.clone())
                .unwrap_or_default();
            let path = m.path.clone();
            let src = cx.new(|cx| {
                let mut s = TextareaState::new(window, cx).auto_grow(18, 18);
                s.set_value(md, window, cx);
                s
            });
            cx.observe(&src, move |this, src, cx| {
                let v = src.read(cx).value().to_string();
                if this.notes.ai.get(&path).and_then(|r| r.result.as_deref()) != Some(v.as_str()) {
                    this.notes.ai.set_result(&path, v);
                    cx.notify();
                }
            })
            .detach();
            if let Some(m) = self.notes.ai_modal.as_mut() {
                m.source = Some(src);
            }
        } else if !want_source {
            m.source = None;
        }
    }

    fn ai_instruction(&self, m: &AiModal, cx: &App) -> (String, String, usize) {
        let typed = m
            .input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        // 내가 친 말 → 저장 프롬프트 → 빠른 지시
        let mut extras: Vec<String> = m
            .saved
            .iter()
            .filter(|p| !p.text.trim().is_empty() && m.chosen.contains(&format!("s:{}", p.id)))
            .map(|p| p.text.clone())
            .collect();
        extras.extend(
            PRESETS
                .iter()
                .enumerate()
                .filter(|(i, _)| m.chosen.contains(&format!("p:{i}")))
                .map(|(_, k)| t(k)),
        );
        let n = extras.len();
        (compose_instruction(&typed, &extras), typed, n)
    }

    fn run_compose(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.notes.ai_modal.as_ref() else {
            return;
        };
        let (instruction, typed, _) = self.ai_instruction(m, cx);
        if instruction.encode_utf16().count() < 2 {
            return;
        }
        let ref_dirs: Vec<String> = m
            .recent
            .iter()
            .filter(|d| m.ref_on.contains(d))
            .cloned()
            .collect();
        let p = StartCompose {
            path: m.path.clone(),
            title: crate::notes_view::file_name(&m.path).to_string(),
            markdown: self.ai_current_body(cx),
            typed,
            chosen: m.chosen.clone(),
            ref_dirs,
            instruction,
        };
        if let Some(m) = self.notes.ai_modal.as_mut() {
            m.revise = false;
        }
        self.start_note_ai(p, cx);
    }

    fn apply_ai(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.notes.ai_modal.as_ref().map(|m| m.path.clone()) else {
            return;
        };
        let Some(md) = self.notes.ai.get(&path).and_then(|r| r.result.clone()) else {
            return;
        };
        self.notes.pending_apply = Some(md);
        // 편집기로 들어갔으니 기다리던 결과는 끝
        self.dismiss_note_ai(&path);
        self.notes.ai_modal = None;
        cx.notify();
    }

    /// 폴더 첨부 — 작업 폴더 고르기와 같은 macOS 폴더 창
    pub(crate) fn pick_ref_dir(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t("notes.ai.refDirs.dialogTitle").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |s, cx| s.add_ref_dir(&dir.to_string_lossy(), cx));
        })
        .detach();
    }

    pub(crate) fn add_ref_dir(&mut self, dir: &str, cx: &mut Context<Self>) {
        let next = push_recent(&load_recent(&self.db), dir);
        amber_gpui::prefs::set(
            &self.db,
            REF_KEY,
            &serde_json::to_string(&next).unwrap_or_default(),
        );
        let dir = dir.trim().to_string();
        if let Some(m) = self.notes.ai_modal.as_mut() {
            m.recent = next.clone();
            if !m.ref_on.contains(&dir) {
                m.ref_on.push(dir.clone());
            }
        }
        if let Some(m) = self.notes.span_modal.as_mut() {
            m.recent = next;
            if !m.ref_on.contains(&dir) {
                m.ref_on.push(dir);
            }
        }
        cx.notify();
    }

    /// 작성 모달
    pub(crate) fn ai_modal_view(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let path = self.notes.ai_modal.as_ref()?.path.clone();
        let run = self.notes.ai.get(&path).cloned();
        let current = self.ai_current_body(cx);
        let has_existing = !current.trim().is_empty();
        let connected = self.ai_connected();
        let reduce = cx.reduce_motion();
        let m = self.notes.ai_modal.as_mut()?;
        // 결과가 새로 왔다 — 기존 노트면 변경점부터, 새로 쓴 것이면 미리보기
        if let Some(r) = run.as_ref().filter(|r| r.phase == Phase::Done)
            && m.seen_finish != r.finished_at
        {
            m.seen_finish = r.finished_at;
            m.view = if has_existing {
                AiView::Diff
            } else {
                AiView::Preview
            };
        }
        #[derive(PartialEq)]
        enum Step {
            Prompt,
            Loading,
            Preview,
        }
        let step = match &run {
            None => Step::Prompt,
            Some(_) if m.revise => Step::Prompt,
            Some(r) if r.phase == Phase::Running => Step::Loading,
            Some(r) if r.phase == Phase::Done || r.result.is_some() => Step::Preview,
            _ => Step::Prompt,
        };
        let error = run
            .as_ref()
            .filter(|r| !m.revise && r.phase == Phase::Error)
            .and_then(|r| r.error.clone());
        let span_busy = run
            .as_ref()
            .is_some_and(|r| r.kind == Kind::Span && r.phase == Phase::Running);
        let result = run
            .as_ref()
            .and_then(|r| r.result.clone())
            .unwrap_or_default();
        let base_changed = run
            .as_ref()
            .is_some_and(|r| r.result.is_some() && r.base.trim() != current.trim());
        // 실시간 글이 늘면 맨 아래로
        let stream = run.as_ref().map(|r| r.stream.clone()).unwrap_or_default();
        if step == Step::Loading && stream.len() != m.stream_len {
            m.stream_len = stream.len();
            m.stream_scroll.scroll_to_bottom();
        }
        // 변경점은 비싸다 — 두 글이 그대로면 다시 계산하지 않는다
        let diff = if step == Step::Preview && m.view == AiView::Diff {
            let h = hash2(&current, &result);
            if m.diff.as_ref().is_none_or(|(x, _)| *x != h) {
                m.diff = Some((h, Rc::new(diff_rows(&current, &result))));
            }
            m.diff.as_ref().map(|(_, d)| d.clone())
        } else {
            None
        };
        let (view, revise, confirm_discard) = (m.view, m.revise, m.confirm_discard);
        let m = self.notes.ai_modal.as_ref()?;
        let (instruction, _, n_extra) = self.ai_instruction(m, cx);
        let too_short = instruction.encode_utf16().count() < 2;

        let mut left: Vec<Stateful<Div>> = Vec::new();
        let mut right: Vec<Stateful<Div>> = Vec::new();
        let close = cx.listener(|this, _, _, cx| {
            this.notes.ai_modal = None;
            cx.notify();
        });
        match step {
            Step::Prompt => {
                if revise && run.as_ref().is_some_and(|r| r.result.is_some()) {
                    left.push(
                        button("ai-back-result", k, false, false)
                            .child(icon("chevron-left", 14.).text_color(k.text_2))
                            .child(t("notes.ai.backToResult"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(m) = this.notes.ai_modal.as_mut() {
                                    m.revise = false;
                                }
                                cx.notify();
                            })),
                    );
                }
                right.push(
                    button("ai-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(close),
                );
                right.push(
                    primary_button("ai-run", k, too_short || !connected || span_busy)
                        .child(icon("sparkles", 15.).text_color(k.text))
                        .child(t("notes.ai.run"))
                        .when(!too_short && connected && !span_busy, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| {
                                this.run_compose(cx);
                                cx.notify();
                            }))
                        }),
                );
            }
            Step::Loading => {
                let p = path.clone();
                left.push(
                    crate::day_card::danger_button("ai-stop", k)
                        .child(icon("x", 14.).text_color(k.danger))
                        .child(t("notes.ai.stop"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.stop_note_ai(&p);
                            cx.notify();
                        })),
                );
                right.push(
                    button("ai-keep", k, false, false)
                        .child(t("notes.ai.bg.keep"))
                        .on_click(close),
                );
            }
            Step::Preview => {
                left.push(
                    button("ai-revise", k, false, false)
                        .child(icon("chevron-left", 14.).text_color(k.text_2))
                        .child(t("notes.ai.back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.notes.ai_modal.as_mut() {
                                m.revise = true;
                            }
                            cx.notify();
                        })),
                );
                left.push(
                    crate::day_card::danger_button("ai-discard", k)
                        .child(t("notes.ai.discard"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.notes.ai_modal.as_mut() {
                                m.confirm_discard = true;
                            }
                            cx.notify();
                        })),
                );
                right.push(
                    button("ai-close", k, false, false)
                        .child(t("common.close"))
                        .on_click(close),
                );
                right.push(
                    primary_button("ai-apply", k, result.is_empty())
                        .child(icon("check", 15.).text_color(k.text))
                        .child(t(if has_existing {
                            "notes.ai.applyDiff"
                        } else {
                            "notes.ai.applyNew"
                        }))
                        .on_click(cx.listener(|this, _, _, cx| this.apply_ai(cx))),
                );
            }
        }

        let mut body = div().flex().flex_col();
        if let Some(e) = error {
            body = body.child(note_box(Note::Error, k).mb(px(12.)).child(e));
        }
        if span_busy {
            body = body.child(
                note_box(Note::Warn, k)
                    .mb(px(12.))
                    .child(t("notes.ai.spanBusy")),
            );
        }
        match step {
            Step::Prompt => body = body.children(self.ai_prompt_fields(m, n_extra, k, cx)),
            Step::Loading => {
                let r = run.as_ref()?;
                let label = t(if r.continuing {
                    "notes.ai.continuing"
                } else {
                    "notes.ai.writing"
                });
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(ai_thinking(
                            "ai-modal-bar",
                            label,
                            Some(run_wait_line(r)),
                            !stream.is_empty(),
                            k,
                            reduce,
                        ))
                        .when(!stream.is_empty(), |d| {
                            d.child(
                                div()
                                    .id("ai-stream")
                                    .tag("ai-stream")
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
                                    .text_color(k.text)
                                    .child(stream.clone()),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(k.text_3)
                                .child(t("notes.ai.bg.hint")),
                        ),
                );
            }
            Step::Preview => {
                let r = run.as_ref()?;
                if r.truncated {
                    let p = path.clone();
                    body = body.child(
                        note_box(Note::Warn, k)
                            .mb(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(div().flex_1().child(t("notes.ai.truncated")))
                            .child(
                                button("ai-continue", k, false, !connected)
                                    .child(icon("sparkles", 13.).text_color(k.text_2))
                                    .child(t("notes.ai.continue"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.continue_note_ai(&p, cx);
                                        cx.notify();
                                    })),
                            ),
                    );
                } else if r.continued {
                    body = body.child(
                        note_box(Note::Warn, k)
                            .mb(px(12.))
                            .child(t("notes.ai.continued")),
                    );
                }
                if base_changed {
                    body = body.child(
                        note_box(Note::Warn, k)
                            .mb(px(12.))
                            .child(t("notes.ai.baseChanged")),
                    );
                }
                body =
                    body.child(self.ai_result_field(m, view, has_existing, &result, diff, k, cx));
            }
        }

        let modal = crate::dialog::shell_split(
            k,
            820.,
            t("notes.ai.title"),
            body.into_any_element(),
            left,
            right,
            cx.listener(|this, _, _, cx| {
                this.notes.ai_modal = None;
                cx.notify();
            }),
        );
        Some(
            div()
                .absolute()
                .inset_0()
                .child(modal)
                .when(confirm_discard, |d| {
                    d.child(self.ai_discard_confirm(&path, k, cx))
                }),
        )
    }

    fn ai_prompt_fields(
        &self,
        m: &AiModal,
        n_extra: usize,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let mut chips: Vec<Stateful<Div>> = Vec::new();
        // 저장 프롬프트가 앞, 기본 지시가 뒤 — 같은 물건이라 한 줄에
        for p in m.saved.iter().filter(|p| !p.text.trim().is_empty()) {
            let key = format!("s:{}", p.id);
            let label = if p.label.trim().is_empty() {
                p.text.chars().take(20).collect()
            } else {
                p.label.trim().to_string()
            };
            chips.push(self.chip_toggle(key, label, "sparkles", m, k, cx));
        }
        for (i, key) in PRESETS.iter().enumerate() {
            chips.push(self.chip_toggle(format!("p:{i}"), t(key), "plus", m, k, cx));
        }
        let mut dirs: Vec<Stateful<Div>> = vec![
            button("ai-ref-add", k, false, false)
                .child(icon("folder-plus", 13.).text_color(k.text_2))
                .child(t("notes.ai.refDirs.add"))
                .on_click(cx.listener(|this, _, _, cx| this.pick_ref_dir(cx))),
        ];
        for d in &m.recent {
            let on = m.ref_on.contains(d);
            let dir = d.clone();
            dirs.push(
                choice_chip(
                    SharedString::from(format!("ai-ref-{d}")),
                    ref_dir_name(d),
                    on,
                    "folder",
                    k,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(m) = this.notes.ai_modal.as_mut() {
                        if let Some(i) = m.ref_on.iter().position(|x| *x == dir) {
                            m.ref_on.remove(i);
                        } else {
                            m.ref_on.push(dir.clone());
                        }
                    }
                    cx.notify();
                })),
            );
        }
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
        vec![
            div()
                .mb(px(16.))
                .child(field_label(t("notes.ai.instructionLabel"), k))
                .children(input)
                .child(hint(t("notes.ai.hint"), k)),
            div()
                .mb(px(16.))
                .child(field_label(t("notes.ai.addLabel"), k))
                .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                .when(n_extra > 0, |d| {
                    d.child(hint(
                        t_with("common.ai.chosenCount", &[("n", &n_extra.to_string())]),
                        k,
                    ))
                }),
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
        ]
    }

    fn chip_toggle(
        &self,
        key: String,
        label: String,
        icon_name: &'static str,
        m: &AiModal,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let on = m.chosen.contains(&key);
        choice_chip(
            SharedString::from(format!("ai-chip-{key}")),
            label,
            on,
            icon_name,
            k,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if let Some(m) = this.notes.ai_modal.as_mut() {
                if let Some(i) = m.chosen.iter().position(|x| *x == key) {
                    m.chosen.remove(i);
                } else {
                    m.chosen.push(key.clone());
                }
            }
            cx.notify();
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn ai_result_field(
        &self,
        m: &AiModal,
        view: AiView,
        has_existing: bool,
        result: &str,
        diff: Option<Rc<(Vec<Row>, usize, usize)>>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let tab = |id: &'static str, label: String, v: AiView| {
            seg_tab(id, label, view == v, k).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(m) = this.notes.ai_modal.as_mut() {
                    m.view = v;
                }
                cx.notify();
            }))
        };
        let segmented = segmented(k)
            .when(has_existing, |d| {
                d.child(tab("ai-tab-diff", t("notes.ai.tabDiff"), AiView::Diff))
            })
            .child(tab(
                "ai-tab-preview",
                t("notes.ai.tabPreview"),
                AiView::Preview,
            ))
            .child(tab(
                "ai-tab-source",
                t("notes.ai.tabSource"),
                AiView::Source,
            ));
        let content = match view {
            AiView::Source => m
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
            AiView::Diff => diff
                .map(|d| diff_view(&d, k).into_any_element())
                .unwrap_or_else(|| div().into_any_element()),
            AiView::Preview => div()
                .id("ai-preview")
                .tag("ai-preview")
                .max_h(px(460.))
                .overflow_y_scroll()
                .child(crate::markdown::text(
                    "ai-preview-md",
                    result.to_string(),
                    self.on_link(cx),
                    k,
                    None,
                ))
                .into_any_element(),
        };
        // diffHint — 언어마다 어순이 달라 "{apply}" 자리에 굵은 버튼 이름을 끼운다(한 문장으로 흐르게 강조 구간으로)
        let dh = t("notes.ai.diffHint");
        let apply = t("notes.ai.applyDiff");
        let (hint_text, bold) = match dh.split_once("{apply}") {
            Some((a, b)) => (format!("{a}{apply}{b}"), a.len()..a.len() + apply.len()),
            None => (dh.clone(), 0..0),
        };
        let hint_el = StyledText::new(hint_text).with_highlights([(
            bold,
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
                            .child(t(if has_existing {
                                "notes.ai.resultEdited"
                            } else {
                                "notes.ai.resultNew"
                            })),
                    )
                    .child(segmented),
            )
            .child(content)
            .when(has_existing, |d| {
                d.child(
                    div()
                        .mt(px(6.))
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(hint_el),
                )
            })
    }

    /// 버리기만 확인한다(DiscardAiModal mode=discard) — 닫기는 아무것도 잃지 않으니 묻지 않는다
    fn ai_discard_confirm(&self, path: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let p = path.to_string();
        crate::dialog::shell(
            k,
            440.,
            t("common.aiDiscard.title"),
            div()
                .child(t("common.aiDiscard.bodyDiscard"))
                .into_any_element(),
            vec![
                button("ai-discard-keep", k, false, false)
                    .child(t("common.aiDiscard.keep"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(m) = this.notes.ai_modal.as_mut() {
                            m.confirm_discard = false;
                        }
                        cx.notify();
                    })),
                crate::day_card::danger_button("ai-discard-yes", k)
                    .child(t("common.aiDiscard.discardOnly"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dismiss_note_ai(&p);
                        if let Some(m) = this.notes.ai_modal.as_mut() {
                            m.confirm_discard = false;
                            m.revise = false;
                        }
                        cx.notify();
                    })),
            ],
            cx.listener(|this, _, _, cx| {
                if let Some(m) = this.notes.ai_modal.as_mut() {
                    m.confirm_discard = false;
                }
                cx.notify();
            }),
        )
    }

    /// 노트 위 배너(.ai-bg-bar) — 모달을 닫은 뒤에도 쓰는 중 / 초안 준비됨 / 실패 를 노트 자리에서 보이고 되돌아갈 길을 준다
    pub(crate) fn ai_banner(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let path = self.notes.selected.clone()?;
        let run = self.notes.ai.get(&path)?.clone();
        let span = run.kind == Kind::Span;
        // 되돌아갈 곳은 그 실행을 시작한 모달
        let open = cx.listener(|this, _, _, cx| {
            this.reopen_note_ai(cx);
        });
        let bar = |d: Div| d.flex().items_center().gap(px(14.)).mt(px(-6.)).mb(px(18.));
        let actions = div().flex().flex_shrink_0().items_center().gap(px(8.));
        match run.phase {
            Phase::Running => {
                let label = t(if run.continuing {
                    "notes.ai.continuing"
                } else if span {
                    "notes.ai.bg.editing"
                } else {
                    "notes.ai.bg.running"
                });
                let p = path.clone();
                Some(
                    bar(div())
                        .tag("ai-banner")
                        .px(px(12.))
                        .py(px(6.))
                        .rounded(px(4.))
                        .bg(k.surface_2)
                        .child(div().flex_1().min_w_0().child(ai_thinking(
                            "ai-banner-bar",
                            label,
                            Some(run_wait_line(&run)),
                            true,
                            k,
                            cx.reduce_motion(),
                        )))
                        .child(
                            actions
                                .child(
                                    button("ai-bg-open", k, false, false)
                                        .child(icon("eye", 13.).text_color(k.text_2))
                                        .child(t("notes.ai.bg.open"))
                                        .on_click(open),
                                )
                                .child(
                                    crate::day_card::danger_button("ai-bg-stop", k)
                                        .child(icon("x", 13.).text_color(k.danger))
                                        .child(t("notes.ai.stop"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.stop_note_ai(&p);
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            }
            Phase::Error => {
                let p = path.clone();
                Some(
                    bar(note_box(Note::Error, k))
                        .tag("ai-banner")
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_wrap()
                                .gap(px(4.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .child(t("notes.ai.bg.failed")),
                                )
                                .child(run.error.clone().unwrap_or_default()),
                        )
                        .child(
                            actions
                                .child(
                                    button("ai-bg-retry", k, false, false)
                                        .child(icon("refresh", 13.).text_color(k.text_2))
                                        .child(t("notes.ai.bg.retry"))
                                        .on_click(open),
                                )
                                .child(
                                    button("ai-bg-close", k, false, false)
                                        .child(t("common.close"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.notes.ai.dismiss_error(&p);
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            }
            Phase::Done => {
                let when = run.finished_at.map(|ms| {
                    t_with(
                        "notes.ai.bg.readyAt",
                        &[("when", &crate::notes_view::time_ago(ms, now_ms()))],
                    )
                });
                Some(
                    bar(note_box(Note::Ok, k))
                        .tag("ai-banner")
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(icon("sparkles", 14.).text_color(k.success_text))
                                .child(div().font_weight(FontWeight::BOLD).child(t(if span {
                                    "notes.ai.bg.editReady"
                                } else {
                                    "notes.ai.bg.ready"
                                })))
                                .when_some(when, |d, w| {
                                    d.child(div().opacity(0.75).text_size(px(12.)).child(w))
                                }),
                        )
                        .child(
                            actions
                                .child(
                                    primary_button("ai-bg-review", k, false)
                                        .child(t("notes.ai.bg.review"))
                                        .on_click(open),
                                )
                                .child(
                                    crate::day_card::danger_button("ai-bg-discard", k)
                                        .child(t("notes.ai.discard"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.reopen_note_ai(cx);
                                            if let Some(m) = this.notes.ai_modal.as_mut() {
                                                m.confirm_discard = true;
                                            }
                                            if let Some(m) = this.notes.span_modal.as_mut() {
                                                m.confirm_discard = true;
                                            }
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            }
        }
    }

    /// 트리 행의 실행 점(.ai-dot) — 쓰는 중 = 맥동, 준비됨 = 멈춘 점, 실패 = 빨간 링
    pub(crate) fn ai_tree_dot(&self, path: &str, k: &Tokens, reduce: bool) -> Option<AnyElement> {
        let phase = self.notes.ai.get(path)?.phase;
        let dot = div()
            .flex_shrink_0()
            .size(px(7.))
            .mx(px(4.))
            .ml(px(6.))
            .rounded_full();
        Some(match phase {
            Phase::Error => dot
                .border(px(1.5))
                .border_color(k.danger)
                .into_any_element(),
            Phase::Done => dot.bg(k.text).into_any_element(),
            Phase::Running if reduce => dot.bg(k.text).opacity(0.85).into_any_element(),
            Phase::Running => dot
                .bg(k.text)
                .with_animation(
                    SharedString::from(format!("ai-dot-{path}")),
                    Animation::new(Duration::from_millis(1100)).repeat(),
                    // 0.3 → 1 → 0.3 (ease-in-out)
                    |d, x| d.opacity(0.3 + 0.7 * (1. - (std::f32::consts::PI * 2. * x).cos()) / 2.),
                )
                .into_any_element(),
        })
    }
}

/// DiffView — 요약(+n −m 줄)과 줄 단위 변경점. 긴 무변경 구간은 접혀 있다
/// .segmented — 결과 보기 탭 묶음
pub(crate) fn segmented(k: &Tokens) -> Div {
    div()
        .flex()
        .gap(px(2.))
        .p(px(3.))
        .rounded(px(7.))
        .border_1()
        .border_color(k.border)
        .bg(k.surface_2)
}

/// .segmented .tab — 고른 탭은 떠 있는 유리
pub(crate) fn seg_tab(id: &'static str, label: String, active: bool, k: &Tokens) -> Stateful<Div> {
    let hover = glass(k.glass_fill);
    div()
        .id(id)
        .tag(id)
        .px(px(12.))
        .py(px(5.))
        .rounded(px(4.))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(active, |d| {
            d.bg(k.glass_raise)
                .text_color(k.text)
                .shadow(vec![BoxShadow {
                    color: k.shadow_sm,
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: false,
                }])
        })
        .when(!active, |d| {
            let t2 = k.text;
            d.text_color(k.text_2)
                .hover(move |s| s.bg(hover).text_color(t2))
        })
        .child(label)
}

pub(crate) fn diff_view(d: &(Vec<Row>, usize, usize), k: &Tokens) -> Div {
    let (rows, add, del) = d;
    let summary = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .mb(px(8.))
        .text_size(px(12.));
    let summary = if *add == 0 && *del == 0 {
        summary.child(div().text_color(k.text_3).child(t("diagrams.diff.none")))
    } else {
        summary
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.diff_add.2)
                    .child(format!("+{add}")),
            )
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.diff_del.2)
                    .child(format!("\u{2212}{del}")),
            )
            .child(div().text_color(k.text_3).child(t("diagrams.diff.lines")))
    };
    let line = |gutter: &str, text: String, bg: Option<Hsla>, gbg: Option<Hsla>, fg: Hsla| {
        div()
            .flex()
            .when_some(bg, |d, c| d.bg(c))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(22.))
                    .flex()
                    .justify_center()
                    .border_r_1()
                    .border_color(k.border)
                    .text_color(fg)
                    .when_some(gbg, |d, c| d.bg(c))
                    .child(gutter.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(10.))
                    .child(if text.is_empty() {
                        " ".to_string()
                    } else {
                        text
                    }),
            )
    };
    div().tag("ai-diff").child(summary).child(
        div()
            .id("ai-diff-view")
            .max_h(px(460.))
            .overflow_y_scroll()
            .rounded(px(6.))
            .border_1()
            .border_color(k.border)
            .bg(k.surface)
            .font_family(crate::markdown::MONO)
            .text_size(px(12.5))
            .line_height(relative(1.55))
            .text_color(k.text)
            .children(rows.iter().map(|r| {
                match r {
                    Row::Add(s) => line(
                        "+",
                        s.clone(),
                        Some(k.diff_add.0),
                        Some(k.diff_add.1),
                        k.diff_add.2,
                    ),
                    Row::Del(s) => line(
                        "\u{2212}",
                        s.clone(),
                        Some(k.diff_del.0),
                        Some(k.diff_del.1),
                        k.diff_del.2,
                    ),
                    Row::Ctx(s) => line("", s.clone(), None, None, k.text_3),
                    Row::Fold(n) => line(
                        "",
                        t_with("diagrams.diff.fold", &[("n", &n.to_string())]),
                        Some(k.surface_2),
                        None,
                        k.text_3,
                    )
                    .text_color(k.text_3)
                    .italic()
                    .text_size(px(11.5)),
                }
            })),
    )
}

#[cfg(test)]
mod tests {
    use super::{push_recent, ref_dir_name};

    #[test]
    fn recent_folders_like_ref_dirs() {
        let l = vec!["/a".to_string(), "/b".to_string()];
        assert_eq!(push_recent(&l, " /b "), vec!["/b", "/a"]);
        assert_eq!(push_recent(&l, "  "), l);
        let many: Vec<String> = (0..6).map(|i| format!("/{i}")).collect();
        assert_eq!(push_recent(&many, "/x").len(), 6);
        assert_eq!(ref_dir_name("/Users/me/repo/"), "repo");
        assert_eq!(ref_dir_name("/"), "/");
    }
}
