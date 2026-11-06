// 리포트 패널 — 지금 앱 DailyReportPanel.tsx, WeeklyReportPanel.tsx. 할 일 카드 아래에 붙는 카드 한 장.
// 일간: ① 미생성(버튼) → ② 수집(소스 칩) → 흐르는 글 → ③ 완성(마크다운 + 메타) / ④ 빈 날, 오류.
// 주간: ① 재료(요일 칩) → ② 흐르는 글 → ③ 완성(평문 — 들여쓰기가 곧 계층이라 마크다운으로 태우지 않는다) / ④.
// 생성은 report_run 스토어가 백그라운드로 돌린다 — 여기는 들여다보기만 한다.
//
// 실패한 다시 생성은 표시에서 뺀다: 멀쩡한 리포트가 사용량 한도 같은 남 탓으로 화면에서 사라지면 안 된다.
// 갈아 끼우는 때는 새 문서가 완성된 때 하나뿐이고, 실패는 모달로 알린다(닫으면 실행을 비운다).
//
// 카드 묶음(useStackCollapse): 리포트를 굴리면 할 일 카드가 묶음 위로 밀려 올라가고 리포트가 커진다.
// 할 일 카드가 다 올라간 뒤에 리포트 안이 구른다. 위로는 거꾸로. 묶음은 카드와 같은 둥근 틀로 자른다.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use amber_gpui::dates;
use amber_gpui::db::Row;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::report::{self, WeekDaySource};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::assets::icon;
use crate::calendar::Unit;
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::notes_view::Block;
use crate::report_run::{ChipStatus, Phase, Run, weekly_key};
use crate::theme::Tokens;
use crate::tip::tip;
use crate::{Spike, hex};

/// 휠 한 번의 입력 — 아래가 +
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelInput {
    pub dy: f32,
    /// 지금 접힌 양
    pub collapse: f32,
    /// 최대로 접을 수 있는 양 — 할 일 카드 높이 + 카드 사이 틈
    pub max: f32,
    /// 커서가 할 일 카드 위인가
    pub on_card: bool,
    /// 커서 밑에서 가장 안쪽의 세로 스크롤 칸: 위로 남은 양, 아래로 남은 양
    pub inner_top: f32,
    pub inner_below: f32,
    /// 리포트 본문이 아래로 더 숨기고 있는 양
    pub report_below: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelPlan {
    pub collapse: f32,
    /// 이 휠을 먹었는가(먹으면 기본 스크롤을 막는다)
    pub consumed: bool,
    /// 먹고 남은 아래 이동 — 리포트 본문을 그만큼 굴린다
    pub spill: f32,
}

/// planWheel — 휠 한 번을 어디에 쓸지(useStackCollapse.ts 와 같은 규칙)
pub fn plan_wheel(s: WheelInput) -> WheelPlan {
    let c = s.collapse.max(0.).min(s.max.max(0.));
    let native = WheelPlan {
        collapse: c,
        consumed: false,
        spill: 0.,
    };
    if s.dy > 0. {
        // 할 일 카드 위에서는 목록이 먼저 구른다 — 목록 끝에 닿아야 판이 움직인다
        if s.on_card && s.inner_below > 0. {
            return native;
        }
        // 리포트가 더 보여 줄 게 있을 때만 접는다
        let room = (s.max - c).min(s.report_below.max(0.));
        if room <= 0. {
            return native;
        }
        let take = s.dy.min(room);
        return WheelPlan {
            collapse: c + take,
            consumed: true,
            spill: if s.on_card { 0. } else { s.dy - take },
        };
    }
    if s.dy < 0. {
        // 위로는 안쪽이 먼저다 — 리포트(또는 목록)가 맨 위에 닿은 뒤에 할 일 카드가 내려온다
        if s.inner_top > 0. || c <= 0. {
            return native;
        }
        return WheelPlan {
            collapse: (c + s.dy).max(0.),
            consumed: true,
            spill: 0.,
        };
    }
    native
}

/// 묻는 것 — 다시 생성, 지우기, 고친 초안 버리기
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    Regen,
    Delete,
    Discard,
}

pub struct ReportEdit {
    pub state: Entity<TextareaState>,
    pub preview: Vec<Block>,
    pub dirty: bool,
}

/// 패널 상태 — 키는 일간이 날짜, 주간이 `w:<주 시작>`
#[derive(Default)]
pub struct Panel {
    /// 디스크에서 읽은 리포트(행, 본문) — 실행이 없을 때의 정본
    pub loaded: Option<(Row, String)>,
    /// 읽은 키와 그때 실행이 있었나 — 키가 바뀌거나 실행이 비면 다시 읽는다
    pub loaded_for: Option<(String, bool)>,
    /// 주간 재료(요일 7개) — 키와 실행 단계가 바뀌면 다시 읽는다
    pub sources: Option<Vec<WeekDaySource>>,
    pub sources_for: Option<(String, Option<Phase>)>,
    /// 그려 둔 본문(마크다운 덩어리) — 본문이 바뀔 때만 다시 짓는다
    pub blocks: Option<(String, Vec<Block>)>,
    pub ask: Option<Ask>,
    pub copied: bool,
    pub saved: bool,
    pub op_error: Option<String>,
    pub edit: Option<ReportEdit>,
    pub stack: Stack,
}

/// 카드 묶음의 자리와 스크롤
#[derive(Default)]
pub struct Stack {
    pub collapse: f32,
    pub key: String,
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    pub card: Rc<Cell<Bounds<Pixels>>>,
    pub list: ScrollHandle,
    pub report: ScrollHandle,
    pub stream: ScrollHandle,
    pub preview: ScrollHandle,
    pub split: Rc<Cell<Bounds<Pixels>>>,
    /// 흐르는 글의 길이 — 새 글이 오면 맨 아래로 굴린다
    pub stream_len: usize,
}

/// 카드 사이 틈(--board-gap)
const GAP: f32 = 8.;

fn src_label(id: &str) -> String {
    match id {
        "todos" => t("report.source.todos"),
        "github" => "GitHub".into(),
        "ai_sessions" => t("report.source.aiSessions"),
        "slack" => "Slack".into(),
        "notion" => "Notion".into(),
        _ => id.to_string(),
    }
}

/// "14:05" — 24시간
fn hhmm(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn glass(k: &Tokens) -> Background {
    linear_gradient(
        150.,
        linear_color_stop(k.glass_fill.0, 0.),
        linear_color_stop(k.glass_fill.1, 1.),
    )
}

/// 스크롤 칸의 위로 남은 양, 아래로 남은 양
fn room(h: &ScrollHandle) -> (f32, f32) {
    let top = -f32::from(h.offset().y);
    let max = f32::from(h.max_offset().y);
    (top.max(0.), (max - top).max(0.))
}

/// .chip — 유리 칩(근거 소스, 요일)
fn chip(k: &Tokens) -> Div {
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
        .bg(glass(k))
        .shadow(k.glass_shadow(false))
}

/// .note-box 의 오류/성공 한 줄
fn note(kind: crate::note_ai_view::Note, text: String, k: &Tokens) -> Div {
    crate::note_ai_view::note_box(kind, k).child(text)
}

/// .icon-sm 버튼 — 머리 띠의 동작(연필, 다시, 복사, 지우기)
fn icon_button(id: &'static str, name: &'static str, k: &Tokens, danger: bool) -> Stateful<Div> {
    let (fill, edge, shadow) = (glass(k), k.glass_edge, k.glass_shadow(false));
    let hover = linear_gradient(
        150.,
        linear_color_stop(k.glass_fill_hover.0, 0.),
        linear_color_stop(k.glass_fill_hover.1, 1.),
    );
    let (soft, red) = (k.danger_soft, k.danger);
    div()
        .id(id)
        .tag(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .shadow(shadow)
        .cursor_pointer()
        .child(icon(name, 13.).text_color(if danger { k.danger } else { k.text_2 }))
        .when(!danger, |d| d.hover(move |s| s.bg(hover)))
        .when(danger, |d| d.hover(move |s| s.bg(soft).border_color(red)))
}

impl Spike {
    /// 지금 보이는 리포트의 키 — 일 모드면 날짜, 주 모드면 w:<주 시작>
    pub(crate) fn report_key(&self) -> String {
        if self.cal.unit == Unit::Week {
            weekly_key(&dates::week_start_of(&self.date))
        } else {
            self.date.clone()
        }
    }

    /// 그리기 전에: 디스크에서 읽기(키가 바뀌었거나 실행이 비었을 때), 주 재료, 본문 덩어리, 묶음 접힘 되돌리기
    pub(crate) fn prepare_report(&mut self, cx: &mut Context<Self>) {
        if self.section != crate::Section::Todo {
            return;
        }
        let key = self.report_key();
        let run = self.reports.runs.get(&key);
        let has_run = run.is_some();
        let phase = run.map(|r| r.phase);
        let p = &mut self.reports.panel;
        let key_changed = p.loaded_for.as_ref().is_none_or(|(k, _)| *k != key);
        let run_cleared = p
            .loaded_for
            .as_ref()
            .is_some_and(|(k, had)| *k == key && *had && !has_run);
        if key_changed {
            // 다른 날로 가면 새로 띄운 패널이다(지금 앱은 key 로 패널을 새로 만든다) — 확인 창, 편집, 알림을 닫는다
            p.loaded = None;
            p.ask = None;
            p.edit = None;
            p.op_error = None;
            p.copied = false;
            p.saved = false;
        }
        // 실행이 있으면 그게 정본이라 읽지 않는다. 실행이 비면(지우기, 실패 닫기, 직접 저장) 디스크로 갈아탄다
        if (key_changed || run_cleared) && !has_run {
            p.loaded = if let Some(week) = key.strip_prefix("w:") {
                report::get_weekly_report(&self.db, week)
                    .ok()
                    .flatten()
                    .and_then(|r| {
                        let path = r
                            .get("file_path")
                            .and_then(Json::as_str)
                            .map(str::to_string);
                        let md = report::read_weekly_report_file(&self.db, week, path.as_deref())
                            .ok()
                            .flatten()?;
                        Some((r, md))
                    })
            } else {
                report::get_report(&self.db, &key)
                    .ok()
                    .flatten()
                    .and_then(|r| {
                        Some((r, report::read_report_file(&self.db, &key).ok().flatten()?))
                    })
            };
        }
        p.loaded_for = Some((key.clone(), has_run));
        if let Some(week) = key.strip_prefix("w:") {
            let sig = (key.clone(), phase);
            if p.sources_for.as_ref() != Some(&sig) {
                p.sources = Some(report::load_week_sources(&self.db, week));
                p.sources_for = Some(sig);
            }
        }
        // 일간 완성 본문 → 마크다운 덩어리(주간은 평문이라 짓지 않는다)
        if !key.starts_with("w:") {
            let body = self.report_shown(&key).2;
            if let Some(body) = body
                && self
                    .reports
                    .panel
                    .blocks
                    .as_ref()
                    .is_none_or(|(b, _)| *b != body)
            {
                let text = hex(Tokens::get(cx).text);
                let blocks = crate::notes_view::build_blocks(&body, &text, cx).0;
                self.reports.panel.blocks = Some((body, blocks));
            }
        }
        // 날짜, 단위가 바뀌면 묶음을 펼친다 — 새 목록은 위에서부터 본다
        let stack_key = format!("{}:{}", self.cal.unit == Unit::Week, self.date);
        let st = &mut self.reports.panel.stack;
        if st.key != stack_key {
            st.key = stack_key;
            st.collapse = 0.;
        }
        // 할 일이 줄어 카드가 낮아졌는데 예전만큼 접혀 있으면 리포트 윗부분까지 밀린다
        let max = f32::from(st.card.get().size.height) + GAP;
        if st.collapse > max {
            st.collapse = max;
        }
    }

    /// 표시값 — (단계, 실행(실패를 뺀 것), 완성 본문, 행, 직전 리포트가 있는 실패의 문구, 인라인 오류)
    #[allow(clippy::type_complexity)]
    fn report_shown(
        &self,
        key: &str,
    ) -> (
        Option<Phase>,
        Option<&Run>,
        Option<String>,
        Option<Row>,
        Option<String>,
        Option<String>,
    ) {
        let run = self.reports.runs.get(key);
        let loaded = self.reports.panel.loaded.as_ref();
        let failed = run.is_some_and(|r| r.phase == Phase::Error);
        let shown = if failed && loaded.is_some() {
            None
        } else {
            run
        };
        let phase = match shown {
            Some(r) => Some(r.phase),
            None if loaded.is_some() => Some(Phase::Done),
            None => None,
        };
        let (body, row) = match (shown, loaded) {
            (Some(r), _) if r.phase == Phase::Done => (Some(r.body.clone()), r.report.clone()),
            (Some(_), _) => (None, None),
            (None, Some((row, body))) => (Some(body.clone()), Some(row.clone())),
            (None, None) => (None, None),
        };
        let failed_prev = (failed && loaded.is_some())
            .then(|| run.and_then(|r| r.error.clone()))
            .flatten();
        let error = if failed_prev.is_some() {
            None
        } else {
            run.and_then(|r| r.error.clone())
        };
        (phase, shown, body, row, failed_prev, error)
    }

    /// 생성 — 연결이 없거나 첫 설정 전이면 설정을 연다
    fn report_generate(&mut self, regen: bool, cx: &mut Context<Self>) {
        let key = self.report_key();
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        if self.reports.runs.running(&key) {
            return;
        }
        if crate::note_ai::ai_args(&self.db).is_none() {
            self.open_settings(cx);
            return;
        }
        let week = key.strip_prefix("w:").map(str::to_string);
        if !regen {
            let future = match &week {
                Some(w) => *w > today,
                None => self.date > today,
            };
            if future {
                return;
            }
            let rc = report::load_report_config(&self.db).unwrap_or_default();
            if rc["onboarded"].as_bool() != Some(true) {
                self.open_settings(cx);
                return;
            }
        }
        self.reports.panel.op_error = None;
        match week {
            Some(w) => {
                let end = dates::week_days(&w)[6].clone();
                self.start_weekly_report(&w, &end, cx);
            }
            None => {
                let date = self.date.clone();
                self.start_report(&date, cx);
            }
        }
    }

    /// Esc — 묻는 창이 있으면 닫는다(초안 버리기는 '계속 편집'), 실패 알림이면 실행을 비운다
    pub(crate) fn report_escape(&mut self) -> bool {
        if self.reports.panel.ask.take().is_some() {
            return true;
        }
        let key = self.report_key();
        if self.report_shown(&key).4.is_some() {
            self.reports.runs.clear(&key);
            return true;
        }
        false
    }

    fn report_remove(&mut self, cx: &mut Context<Self>) {
        let key = self.report_key();
        self.reports.panel.ask = None;
        let res = match key.strip_prefix("w:") {
            Some(w) => report::delete_weekly_report(&self.db, w),
            None => report::delete_report(&self.db, &key),
        };
        match res {
            Ok(()) => {
                self.reports.runs.clear(&key);
                self.reports.panel.loaded = None;
                self.reports.panel.loaded_for = Some((key, false));
            }
            Err(e) => self.reports.panel.op_error = Some(e),
        }
        cx.notify();
    }

    fn report_copy(&mut self, body: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(body));
        self.reports.panel.copied = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |s, cx| {
                s.reports.panel.copied = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn report_start_edit(
        &mut self,
        body: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx);
            s.set_value(body.clone(), window, cx);
            s
        });
        cx.subscribe_in(&state, window, |this, input, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let draft = input.read(cx).value().to_string();
                let key = this.report_key();
                let base = this.report_shown(&key).2.unwrap_or_default();
                let text = hex(Tokens::get(cx).text);
                let preview = crate::notes_view::build_blocks(&draft, &text, cx).0;
                if let Some(e) = this.reports.panel.edit.as_mut() {
                    e.dirty = draft != base;
                    e.preview = preview;
                }
                cx.notify();
            }
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        let text = hex(Tokens::get(cx).text);
        let preview = crate::notes_view::build_blocks(&body, &text, cx).0;
        self.reports.panel.op_error = None;
        self.reports.panel.edit = Some(ReportEdit {
            state,
            preview,
            dirty: false,
        });
        cx.notify();
    }

    /// 편집 끝 — 고친 게 있으면 묻는다(초안은 파일에 없으니 닫으면 사라진다)
    fn report_close_edit(&mut self, cx: &mut Context<Self>) {
        if self.reports.panel.edit.as_ref().is_some_and(|e| e.dirty) {
            self.reports.panel.ask = Some(Ask::Discard);
        } else {
            self.reports.panel.edit = None;
        }
        cx.notify();
    }

    /// ⌘S — 본문 파일만 다시 쓴다(생성 시각, 모델 같은 메타는 그대로). 저장 뒤로는 파일이 정본이라 실행을 비운다
    pub(crate) fn report_save_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.reports.panel.edit.as_ref() else {
            return;
        };
        let md = edit.state.read(cx).value().to_string();
        let date = self.report_key();
        match report::write_report_file(&self.db, &date, &md) {
            Ok(_) => {
                if let Ok(Some(meta)) = report::get_report(&self.db, &date) {
                    self.reports.panel.loaded = Some((meta, md));
                }
                self.reports.runs.clear(&date);
                self.reports.panel.loaded_for = Some((date, false));
                self.reports.panel.edit = None;
                self.reports.panel.saved = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        s.reports.panel.saved = false;
                        cx.notify();
                    });
                })
                .detach();
            }
            Err(e) => self.reports.panel.op_error = Some(e),
        }
        cx.notify();
    }

    /// 묶음 휠 — 먹었으면 true(기본 스크롤을 막는다)
    fn stack_wheel(&mut self, e: &ScrollEvent, line: Pixels, cx: &mut Context<Self>) -> bool {
        if e.modifiers.control || self.reports.panel.ask.is_some() || self.confirm.is_some() {
            return false;
        }
        let d = e.delta.pixel_delta(line);
        if d.x.abs() > d.y.abs() {
            return false;
        }
        let st = &self.reports.panel.stack;
        let pos = e.position;
        let on_card = st.card.get().contains(&pos);
        let editing = self.reports.panel.edit.is_some();
        // 커서 밑에서 실제로 구르는 가장 안쪽 칸
        let mut inner: Vec<&ScrollHandle> = Vec::new();
        if on_card {
            inner.push(&st.list);
        } else {
            inner.push(&st.stream);
            if editing && st.split.get().contains(&pos) {
                inner.push(&st.preview);
            }
            inner.push(&st.report);
        }
        let (inner_top, inner_below) = inner
            .into_iter()
            .find(|h| h.bounds().contains(&pos) && f32::from(h.max_offset().y) > 1.)
            .map(room)
            .unwrap_or((0., 0.));
        let plan = plan_wheel(WheelInput {
            dy: -f32::from(d.y),
            collapse: st.collapse,
            max: f32::from(st.card.get().size.height) + GAP,
            on_card,
            inner_top,
            inner_below,
            report_below: room(&st.report).1,
        });
        let st = &mut self.reports.panel.stack;
        if plan.collapse != st.collapse {
            st.collapse = plan.collapse;
            cx.notify();
        }
        if plan.consumed && plan.spill > 0. {
            let o = st.report.offset();
            st.report.set_offset(point(o.x, o.y - px(plan.spill)));
        }
        plan.consumed
    }

    /// 할 일 카드 + 리포트 카드 묶음(.todo-stack)
    pub(crate) fn todo_stack(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let st = &self.reports.panel.stack;
        let (bounds, card_b) = (st.bounds.clone(), st.card.clone());
        let collapse = st.collapse;
        let this = cx.entity().downgrade();
        let card = self
            .day_card(k, cx)
            .relative()
            .flex_shrink(1.)
            .min_h(px(160.))
            .mt(px(-collapse))
            .child(
                canvas(move |b, _, _| card_b.set(b), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            );
        let report = self.report_card(k, cx);
        div()
            .id("todo-stack")
            .tag("todo-stack")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.))
            .child(card)
            .child(report)
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    move |b, _, window, _| {
                        let this = this.clone();
                        window.on_mouse_event(move |e: &ScrollEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || !b.contains(&e.position) {
                                return;
                            }
                            let line = window.line_height();
                            let eaten = this
                                .update(cx, |s, cx| s.stack_wheel(e, line, cx))
                                .unwrap_or(false);
                            if eaten {
                                cx.stop_propagation();
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// 리포트 카드 — 머리 띠(제목 + 동작)는 서고 아래만 카드 안에서 구른다
    fn report_card(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let key = self.report_key();
        let weekly = key.starts_with("w:");
        let (phase, shown, body, row, failed_prev, error) = self.report_shown(&key);
        let phase_v = phase;
        let editing = self.reports.panel.edit.is_some();
        let stream = shown.map(|r| r.stream.clone()).unwrap_or_default();
        let chips = shown.map(|r| r.chips.clone()).unwrap_or_default();
        let streaming = matches!(phase, Some(Phase::Collecting | Phase::Streaming));
        // 본문이 생기면(작성 중 포함) 할 일보다 먼저 줄어든다 — 가중치를 크게 줘 리포트가 거의 전부를 떠안는다
        let has_body = (phase == Some(Phase::Done) && body.is_some())
            || (streaming && !stream.is_empty())
            || editing;
        let stack_h = f32::from(self.reports.panel.stack.bounds.get().size.height);
        let head = if weekly {
            self.weekly_head(k, &key, phase, body.clone(), cx)
        } else {
            self.daily_head(k, phase, body.clone(), cx)
        };
        // 새 글이 오면 흐르는 상자를 맨 아래로
        let st = &mut self.reports.panel.stack;
        if stream.len() != st.stream_len {
            st.stream_len = stream.len();
            st.stream.set_offset(point(px(0.), px(-1e7)));
        }
        let mut scroll = div()
            .id("report-scroll")
            .tag("report-scroll")
            .flex()
            .flex_col()
            .flex_shrink(1.)
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.reports.panel.stack.report)
            .pt(px(12.))
            .px(px(16.))
            .pb(px(16.));
        scroll = if weekly {
            self.weekly_body(
                k,
                &key,
                phase_v,
                &stream,
                body.clone(),
                row.clone(),
                scroll,
                cx,
            )
        } else {
            self.daily_body(k, phase_v, &chips, &stream, row.clone(), scroll, cx)
        };
        if let (Some(Phase::Error), Some(e)) = (phase, error) {
            scroll = scroll.child(note(crate::note_ai_view::Note::Error, e, k).mt(px(8.)));
        }
        if let Some(e) = self.reports.panel.op_error.clone() {
            scroll = scroll.child(note(crate::note_ai_view::Note::Error, e, k).mt(px(8.)));
        }
        if self.reports.panel.saved {
            scroll = scroll.child(
                note(crate::note_ai_view::Note::Ok, t("report.editSaved"), k)
                    .mt(px(8.))
                    .tag("report-saved"),
            );
        }
        let mut card = div()
            .tag(if weekly { "report-weekly" } else { "report" })
            .mt(px(GAP))
            .flex()
            .flex_col()
            .overflow_hidden()
            .min_h_0()
            .border_1()
            .border_color(k.glass_edge)
            .rounded(px(12.))
            .bg(glass(k))
            .shadow(k.glass_shadow(false))
            .flex_grow(1.)
            .child(head)
            .child(scroll);
        if has_body {
            card = card.flex_shrink(1000.).min_h(px((stack_h * 0.4).min(300.)));
        } else {
            card = card.flex_shrink_0();
        }
        let _ = failed_prev;
        card
    }

    /// 일간 머리 띠 — 편집 중엔 되돌릴 수 없는 동작(다시 생성, 지우기)을 감춘다
    fn daily_head(
        &self,
        k: &Tokens,
        phase: Option<Phase>,
        body: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let editing = self.reports.panel.edit.as_ref();
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let future = self.date > today;
        let connected = crate::note_ai::ai_args(&self.db).is_some();
        let actions: AnyElement = match (phase, editing) {
            (Some(Phase::Done), Some(e)) => div()
                .flex()
                .gap(px(2.))
                .child(
                    button("report-edit-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| this.report_close_edit(cx))),
                )
                .child({
                    let dirty = e.dirty;
                    primary_button("report-edit-save", k, !dirty)
                        .child(t("common.save"))
                        .when(dirty, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.report_save_edit(cx)))
                        })
                })
                .into_any_element(),
            (Some(Phase::Done), None) => {
                let body = body.unwrap_or_default();
                let copied = self.reports.panel.copied;
                div()
                    .flex()
                    .gap(px(2.))
                    .child(
                        icon_button("report-edit", "pencil", k, false)
                            .tooltip(tip(t("report.edit")))
                            .on_click({
                                let body = body.clone();
                                cx.listener(move |this, _, window, cx| {
                                    this.report_start_edit(body.clone(), window, cx)
                                })
                            }),
                    )
                    .child(
                        icon_button("report-regen", "refresh", k, false)
                            .tooltip(tip(t("report.regen")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = Some(Ask::Regen);
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_button(
                            "report-copy",
                            if copied { "check" } else { "copy" },
                            k,
                            false,
                        )
                        .tooltip(tip(t("report.copy")))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.report_copy(body.clone(), cx)),
                        ),
                    )
                    .child(
                        icon_button("report-delete", "trash", k, true)
                            .tooltip(tip(t("common.delete")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = Some(Ask::Delete);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }
            (None | Some(Phase::Empty | Phase::Error), _) => {
                let b = primary_button("report-generate", k, future)
                    .child(icon("sparkles", 13.).text_color(k.text))
                    .child(t(if connected {
                        "report.generate"
                    } else {
                        "report.connectGenerate"
                    }));
                if future {
                    b.tooltip(tip(t("report.futureNo"))).into_any_element()
                } else {
                    b.on_click(cx.listener(|this, _, _, cx| this.report_generate(false, cx)))
                        .into_any_element()
                }
            }
            _ => div().into_any_element(),
        };
        report_head(k)
            .child(eyebrow(k, "sparkles", t("report.title")))
            .child(div().flex_1())
            .child(actions)
    }

    #[allow(clippy::too_many_arguments)]
    fn daily_body(
        &self,
        k: &Tokens,
        phase: Option<Phase>,
        chips: &[crate::report_run::Chip],
        stream: &str,
        row: Option<Row>,
        mut d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let reduce = cx.reduce_motion();
        match phase {
            None => {
                d = d.child(hint(
                    k,
                    t(if self.date > today {
                        "report.hintFuture"
                    } else {
                        "report.hintIdle"
                    }),
                ))
            }
            Some(Phase::Collecting | Phase::Streaming) => {
                let collecting = phase == Some(Phase::Collecting);
                let mut row_el = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .mb(px(12.));
                for c in chips {
                    row_el = row_el.child(run_chip(c, k));
                }
                let key = self.report_key();
                row_el = row_el.child(
                    stop_button(k, collecting)
                        .ml_auto()
                        .tooltip(tip(t("report.stopHint")))
                        .when(!collecting, |b| {
                            b.on_click(cx.listener(move |this, _, _, _| this.cancel_report(&key)))
                        }),
                );
                d = d.child(row_el);
                d = if collecting {
                    d.child(thinking(k, t("report.collecting"), reduce))
                } else if !stream.is_empty() {
                    d.child(self.stream_box(k, stream, reduce))
                } else {
                    d.child(thinking(k, t("report.summarizing"), reduce))
                };
            }
            Some(Phase::Done) => {
                d = d.child(self.daily_meta(k, row));
                d = if let Some(edit) = self.reports.panel.edit.as_ref() {
                    d.child(self.report_edit_split(k, edit, cx))
                        .child(hint(k, t("report.editHint")))
                } else {
                    d.child(self.report_markdown(k, cx))
                };
            }
            Some(Phase::Empty) => d = d.child(empty_note(k, t("report.empty"))),
            Some(Phase::Error) => {}
        }
        d
    }

    /// 완성 메타 — 근거 소스 태그, 실패한 소스, 시각과 모델
    fn daily_meta(&self, k: &Tokens, row: Option<Row>) -> Div {
        let sources: Vec<Json> = row
            .as_ref()
            .and_then(|r| r.get("sources_json")?.as_str())
            .and_then(|s| serde_json::from_str::<Json>(s).ok())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let id = |s: &Json| s["id"].as_str().unwrap_or_default().to_string();
        let mut meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .mb(px(12.));
        for s in sources.iter().filter(|s| {
            s["ok"].as_bool() == Some(true)
                && (s["items"].as_f64().unwrap_or(0.) > 0. || s["mcp"].as_bool() == Some(true))
        }) {
            meta = meta.child(chip(k).child(format!("#{}", src_label(&id(s)))));
        }
        for (i, s) in sources
            .iter()
            .filter(|s| s["ok"].as_bool() != Some(true))
            .enumerate()
        {
            meta = meta.child(
                chip(k)
                    .id(ElementId::Name(format!("report-src-failed-{i}").into()))
                    .border_dashed()
                    .text_color(k.text_3)
                    .tooltip(tip(t("report.srcFailedTitle")))
                    .child(t_with("report.srcFailed", &[("name", &src_label(&id(s)))])),
            );
        }
        meta.when_some(row, |d, r| d.child(stamp(k, &r)))
    }

    /// 완성 본문 — 노트와 같은 마크다운(13.5px)
    fn report_markdown(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let on_link = open_link();
        let mut d = div().tag("report-body").flex().flex_col();
        if let Some((_, blocks)) = self.reports.panel.blocks.as_ref() {
            for (i, b) in blocks.iter().enumerate() {
                d = d.child(match b {
                    Block::Text(state) => crate::markdown::view(state, on_link.clone(), k, None)
                        .text_size(px(13.5))
                        .into_any_element(),
                    other => self.note_block(other, 2_000_000 + i as u64, &on_link, k, cx),
                });
            }
        }
        d
    }

    /// 편집 — 왼쪽 원문 | 오른쪽 미리보기(노트 편집과 같은 문법), 높이 440 고정, 둘 다 제 안에서 구른다
    fn report_edit_split(&self, k: &Tokens, edit: &ReportEdit, cx: &mut Context<Self>) -> Div {
        let on_link = open_link();
        let split = self.reports.panel.stack.split.clone();
        div()
            .relative()
            .flex()
            .gap(px(12.))
            .h(px(440.))
            .mb(px(8.))
            .child(
                canvas(move |b, _, _| split.set(b), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .py(px(9.))
                    .px(px(11.))
                    .font_family(crate::markdown::MONO)
                    .text_size(px(13.))
                    .line_height(relative(1.7))
                    .child(Textarea::new(&edit.state).appearance(false).h(relative(1.))),
            )
            .child(
                div()
                    .id("report-preview")
                    .tag("report-preview")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.reports.panel.stack.preview)
                    .rounded(px(8.))
                    .border_1()
                    .border_color(k.border)
                    .bg(k.surface)
                    .py(px(9.))
                    .px(px(18.))
                    .flex()
                    .flex_col()
                    .children(edit.preview.iter().enumerate().map(|(i, b)| {
                        match b {
                            Block::Text(state) => {
                                crate::markdown::view(state, on_link.clone(), k, None)
                                    .text_size(px(13.5))
                                    .into_any_element()
                            }
                            other => self.note_block(other, 3_000_000 + i as u64, &on_link, k, cx),
                        }
                    })),
            )
    }

    /// 흐르는 글 — 고정폭 평문, 최대 420 에서 구르고 깜빡이는 캐럿
    fn stream_box(&self, k: &Tokens, stream: &str, reduce: bool) -> Stateful<Div> {
        let caret = div().w(px(7.)).h(px(15.)).rounded(px(1.)).bg(k.text);
        let caret = if reduce {
            caret.into_any_element()
        } else {
            caret
                .with_animation(
                    "report-caret",
                    Animation::new(Duration::from_millis(1000)).repeat(),
                    |d, x| d.opacity(if x < 0.5 { 1. } else { 0. }),
                )
                .into_any_element()
        };
        div()
            .id("report-stream")
            .tag("report-stream")
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.reports.panel.stack.stream)
            .px(px(16.))
            .py(px(14.))
            .border_1()
            .border_color(k.border)
            .rounded(px(8.))
            .bg(k.surface_2)
            .font_family(crate::markdown::MONO)
            .text_size(px(12.5))
            .line_height(relative(1.65))
            .text_color(k.text)
            .child({
                // 캐럿은 마지막 줄 끝에 붙는다 — 앞줄들은 한 덩어리, 마지막 줄만 캐럿과 한 줄로
                let (head, last) = match stream.rfind('\n') {
                    Some(i) => (Some(stream[..i].to_string()), stream[i + 1..].to_string()),
                    None => (None, stream.to_string()),
                };
                div()
                    .flex()
                    .flex_col()
                    .when_some(head, |d, h| d.child(h))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(1.))
                            .when(!last.is_empty(), |d| d.child(last))
                            .child(caret),
                    )
            })
    }

    /// 주간 머리 띠 — 제목 + 범위(이 본문은 앱 밖으로 복사돼 나가 어느 주인지가 함께 가야 한다)
    fn weekly_head(
        &self,
        k: &Tokens,
        key: &str,
        phase: Option<Phase>,
        body: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let week = key.trim_start_matches("w:").to_string();
        let days = dates::week_days(&week);
        let range = format!(
            "{} \u{2013} {}",
            dates::format_day_short(&week),
            dates::format_day_short(&days[6])
        );
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let future = week > today;
        let have = self.reports.panel.sources.as_ref().map(|s| {
            s.iter()
                .filter(|d| {
                    d.body
                        .as_deref()
                        .is_some_and(|b| !amber_gpui::vault::js_trim(b).is_empty())
                })
                .count()
        });
        let busy = phase == Some(Phase::Streaming);
        let connected = crate::note_ai::ai_args(&self.db).is_some();
        let reduce = cx.reduce_motion();
        let label = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(eyebrow(k, "calendar-check", t("report.weekly.title")))
            .child(div().text_size(px(11.)).text_color(k.text_3).child(range))
            .when(busy, |d| d.child(busy_dot(k, reduce)));
        let actions: AnyElement = match phase {
            Some(Phase::Done) => {
                let body = body.unwrap_or_default();
                let copied = self.reports.panel.copied;
                div()
                    .flex()
                    .gap(px(2.))
                    .child(
                        icon_button("report-regen", "refresh", k, false)
                            .tooltip(tip(t("report.regen")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = Some(Ask::Regen);
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_button(
                            "report-copy",
                            if copied { "check" } else { "copy" },
                            k,
                            false,
                        )
                        .tooltip(tip(t("report.weekly.copy")))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.report_copy(body.clone(), cx)),
                        ),
                    )
                    .child(
                        icon_button("report-delete", "trash", k, true)
                            .tooltip(tip(t("common.delete")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = Some(Ask::Delete);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }
            None | Some(Phase::Empty | Phase::Error) => {
                let off = future || have == Some(0);
                let b = primary_button("report-generate", k, off)
                    .child(icon("sparkles", 13.).text_color(k.text))
                    .child(t(if connected {
                        "report.weekly.generate"
                    } else {
                        "report.connectGenerate"
                    }));
                if future {
                    b.tooltip(tip(t("report.weekly.futureNo")))
                        .into_any_element()
                } else if off {
                    b.into_any_element()
                } else {
                    b.on_click(cx.listener(|this, _, _, cx| this.report_generate(false, cx)))
                        .into_any_element()
                }
            }
            _ => div().into_any_element(),
        };
        report_head(k)
            .child(label)
            .child(div().flex_1())
            .child(actions)
    }

    #[allow(clippy::too_many_arguments)]
    fn weekly_body(
        &self,
        k: &Tokens,
        key: &str,
        phase: Option<Phase>,
        stream: &str,
        body: Option<String>,
        row: Option<Row>,
        mut d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let week = key.trim_start_matches("w:").to_string();
        let end = dates::week_days(&week)[6].clone();
        let today = dates::today_str(chrono::Local::now().timestamp_millis());
        let future = week > today;
        let current = !future && end >= today;
        let reduce = cx.reduce_motion();
        let streaming = phase == Some(Phase::Streaming);
        // ① 커버리지 — 어느 날이 재료로 들어가고 어느 날이 빠지는지 먼저 밝힌다
        if !streaming && let Some(sources) = self.reports.panel.sources.as_ref() {
            let has = |s: &WeekDaySource| {
                s.body
                    .as_deref()
                    .is_some_and(|b| !amber_gpui::vault::js_trim(b).is_empty())
            };
            let have = sources.iter().filter(|s| has(s)).count();
            let missing = sources.len() - have;
            let mut row_el = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.))
                .mb(px(8.));
            for s in sources {
                let on = has(s);
                let c = chip(k).min_w(px(22.)).justify_center();
                row_el = row_el.child(if on {
                    c.bg(k.surface_2)
                        .text_color(k.text)
                        .child(s.weekday.clone())
                } else {
                    c.bg(transparent_black())
                        .shadow(Vec::new())
                        .border_dashed()
                        .border_color(k.border_strong)
                        .text_color(k.text_3)
                        .child(s.weekday.clone())
                });
            }
            let count = if have > 0 {
                let mut c = t_with("report.weekly.coverage", &[("n", &have.to_string())]);
                if missing > 0 {
                    c.push(' ');
                    c.push_str(&t_with(
                        "report.weekly.missingDays",
                        &[("n", &missing.to_string())],
                    ));
                }
                c
            } else {
                t("report.weekly.noSources")
            };
            row_el = row_el.child(
                div()
                    .ml(px(4.))
                    .text_size(px(11.))
                    .text_color(k.text_3)
                    .child(count),
            );
            d = d.child(row_el);
        }
        if current && !matches!(phase, Some(Phase::Done | Phase::Streaming)) {
            d = d.child(hint(k, t("report.weekly.currentWeek")));
        }
        match phase {
            Some(Phase::Streaming) => {
                let wk = week.clone();
                d = d.child(
                    div().flex().justify_end().mt(px(6.)).mb(px(2.)).child(
                        stop_button(k, false)
                            .tooltip(tip(t("report.stopHint")))
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.cancel_report(&weekly_key(&wk))
                            })),
                    ),
                );
                d = if !stream.is_empty() {
                    d.child(self.stream_box(k, stream, reduce))
                } else {
                    d.child(thinking(k, t("report.weekly.summarizing"), reduce))
                };
            }
            Some(Phase::Done) => {
                let mut meta = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .mb(px(12.));
                if let Some(r) = row.as_ref() {
                    let n = r
                        .get("sources_json")
                        .and_then(Json::as_str)
                        .and_then(|s| serde_json::from_str::<Json>(s).ok())
                        .and_then(|v| v.as_array().map(Vec::len))
                        .unwrap_or(0);
                    meta = meta
                        .child(
                            chip(k)
                                .child(t_with("report.weekly.builtFrom", &[("n", &n.to_string())])),
                        )
                        .child(stamp(k, r));
                }
                d = d.child(meta).child(
                    div()
                        .tag("report-week-body")
                        .text_size(px(13.))
                        .line_height(relative(1.7))
                        .text_color(k.text)
                        .child(body.unwrap_or_default()),
                );
            }
            Some(Phase::Empty) => d = d.child(empty_note(k, t("report.weekly.empty"))),
            _ => {}
        }
        d
    }

    /// 묻는 창들 — 다시 생성, 지우기, 초안 버리기, 다시 생성 실패(직전 리포트는 그대로)
    pub(crate) fn report_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if self.section != crate::Section::Todo {
            return None;
        }
        let key = self.report_key();
        let weekly = key.starts_with("w:");
        let close = || {
            cx.listener(|this, _, _, cx| {
                this.reports.panel.ask = None;
                cx.notify();
            })
        };
        let cancel = |label: String| {
            button("report-ask-cancel", k, false, false)
                .child(label)
                .on_click(close())
        };
        if let Some(ask) = self.reports.panel.ask {
            let w = |a: &'static str, b: &'static str| t(if weekly { a } else { b });
            return Some(match ask {
                Ask::Regen => crate::dialog::shell(
                    k,
                    440.,
                    w("report.weekly.regenTitle", "report.regenTitle"),
                    div()
                        .child(w("report.weekly.regenBody", "report.regenBody"))
                        .into_any_element(),
                    vec![
                        cancel(t("common.cancel")),
                        danger_button("report-ask-regen", k)
                            .child(t("report.regen"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = None;
                                this.report_generate(true, cx);
                            })),
                    ],
                    close(),
                ),
                Ask::Delete => crate::dialog::shell(
                    k,
                    440.,
                    w("report.weekly.deleteTitle", "report.deleteTitle"),
                    div()
                        .child(w("report.weekly.deleteBody", "report.deleteBody"))
                        .into_any_element(),
                    vec![
                        cancel(t("common.cancel")),
                        danger_button("report-ask-delete", k)
                            .child(t("common.delete"))
                            .on_click(cx.listener(|this, _, _, cx| this.report_remove(cx))),
                    ],
                    close(),
                ),
                Ask::Discard => crate::dialog::shell(
                    k,
                    440.,
                    t("common.unsaved.title"),
                    div().child(t("common.unsaved.body")).into_any_element(),
                    vec![
                        cancel(t("common.unsaved.keep")),
                        danger_button("report-ask-discard", k)
                            .child(t("common.unsaved.discard"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reports.panel.ask = None;
                                this.reports.panel.edit = None;
                                cx.notify();
                            })),
                    ],
                    close(),
                ),
            });
        }
        let failed = self.report_shown(&key).4?;
        let (k1, k2) = (key.clone(), key);
        Some(crate::dialog::shell(
            k,
            440.,
            t("report.failedTitle"),
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(failed)
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(k.text_3)
                        .child(t("report.failedKept")),
                )
                .into_any_element(),
            vec![
                button("report-failed-close", k, false, false)
                    .child(t("common.close"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reports.runs.clear(&k1);
                        cx.notify();
                    })),
            ],
            cx.listener(move |this, _, _, cx| {
                this.reports.runs.clear(&k2);
                cx.notify();
            }),
        ))
    }
}

/// 리포트 링크는 브라우저로
fn open_link() -> crate::markdown::OnLink {
    Arc::new(|url, _, _, cx| cx.open_url(url))
}

fn report_head(k: &Tokens) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(8.))
        .min_h(px(26.))
        .pt(px(14.))
        .px(px(16.))
        .pb(px(10.))
        .border_b_1()
        .border_color(k.glass_edge)
}

/// .report-eyebrow — 작은 대문자 제목
fn eyebrow(k: &Tokens, name: &'static str, label: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(k.text_3)
        .child(icon(name, 12.).text_color(k.text_3))
        .child(label.to_uppercase())
}

fn hint(k: &Tokens, text: String) -> Div {
    div()
        .text_size(px(12.5))
        .line_height(relative(1.6))
        .text_color(k.text_3)
        .child(text)
}

/// 활동 없음 — 왼쪽 바 + 옅은 바탕(색으로 칠하지 않는다)
fn empty_note(k: &Tokens, text: String) -> Div {
    hint(k, text)
        .px(px(14.))
        .py(px(10.))
        .bg(k.surface_2)
        .border_l(px(2.))
        .border_color(k.text)
        .rounded_r(px(6.))
        .tag("report-empty")
}

/// 시각 · 모델 — 오른쪽 끝
fn stamp(k: &Tokens, r: &Row) -> Div {
    let at = r.get("created_at").and_then(Json::as_f64).unwrap_or(0.) as i64;
    let model = r.get("model").and_then(Json::as_str).unwrap_or_default();
    let mut s = hhmm(at);
    if !model.is_empty() {
        // 지금 앱과 같은 구분자(U+00B7)
        s.push_str(" \u{b7} ");
        s.push_str(model);
    }
    div()
        .ml_auto()
        .text_size(px(11.))
        .text_color(k.text_3)
        .child(s)
}

/// 수집 칩 — 완료는 채움 + 체크, 대기는 아웃라인, 실패는 점선(사유는 말풍선), MCP 는 도구 표시
fn run_chip(c: &crate::report_run::Chip, k: &Tokens) -> AnyElement {
    let mut label = src_label(&c.id);
    if c.status == ChipStatus::Ok {
        label.push_str(&format!(" {}", c.items));
    }
    let base = div()
        .id(ElementId::Name(format!("report-chip-{}", c.id).into()))
        .tag(format!("report-chip-{}", c.id))
        .flex()
        .items_center()
        .gap(px(5.))
        .text_size(px(12.))
        .px(px(10.))
        .py(px(3.))
        .rounded_full()
        .border_1();
    let el = match c.status {
        ChipStatus::Ok => base
            .bg(k.primary_bg)
            .text_color(k.primary_fg)
            .border_color(k.primary_bg)
            .child(icon("check", 12.).text_color(k.primary_fg)),
        ChipStatus::Pending => base.text_color(k.text_3).border_color(k.border_strong),
        ChipStatus::Error => base
            .text_color(k.text_3)
            .border_color(k.border_strong)
            .border_dashed()
            .child(icon("x", 12.).text_color(k.text_3)),
        ChipStatus::Mcp => base
            .text_color(k.text)
            .border_color(transparent_black())
            .child(icon("workflow", 12.).text_color(k.text)),
    }
    .child(label);
    match &c.error {
        Some(e) => el.tooltip(tip(e.clone())).into_any_element(),
        None => el.into_any_element(),
    }
}

/// .btn-danger.btn-sm '중단' — 수집 중엔 꺼진다(생성 단계에서만 멈출 수 있다)
fn stop_button(k: &Tokens, disabled: bool) -> Stateful<Div> {
    danger_button("report-stop", k)
        .child(icon("x", 13.).text_color(k.danger))
        .child(t("report.stop"))
        .when(disabled, |d| d.opacity(0.5).cursor_default())
}

/// .ai-thinking.ring — 도는 원호 하나와 라벨(한 대기 상태에 모션은 하나)
fn thinking(k: &Tokens, label: String, reduce: bool) -> Div {
    div()
        .tag("report-thinking")
        .flex()
        .items_center()
        .gap(px(10.))
        .pt(px(14.))
        .pb(px(24.))
        .child(crate::spinner::ai_ring(k, reduce))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.text_2)
                .child(label),
        )
}

/// .rail-busy — 주간 머리의 생성 중 맥동 점
fn busy_dot(k: &Tokens, reduce: bool) -> AnyElement {
    let dot = div().size(px(6.)).rounded_full().bg(k.text);
    if reduce {
        dot.opacity(0.85).into_any_element()
    } else {
        dot.with_animation(
            "report-weekly-busy",
            Animation::new(Duration::from_millis(1100)).repeat(),
            |d, x| d.opacity(0.3 + 0.7 * (1. - (std::f32::consts::PI * 2. * x).cos()) / 2.),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{WheelInput, WheelPlan, plan_wheel};

    // useStackCollapse.test.ts 의 갈래를 그대로 — "리포트를 굴리면 리포트가 커진다"
    const BASE: WheelInput = WheelInput {
        dy: 0.,
        collapse: 0.,
        max: 300.,
        on_card: false,
        inner_top: 0.,
        inner_below: 0.,
        report_below: 1000.,
    };

    fn plan(f: impl FnOnce(&mut WheelInput)) -> WheelPlan {
        let mut s = BASE;
        f(&mut s);
        plan_wheel(s)
    }

    fn p(collapse: f32, consumed: bool, spill: f32) -> WheelPlan {
        WheelPlan {
            collapse,
            consumed,
            spill,
        }
    }

    #[test]
    fn scrolling_down_grows_the_report_first() {
        assert_eq!(
            plan(|s| {
                s.dy = 80.;
                s.inner_below = 900.;
            }),
            p(80., true, 0.)
        );
        assert_eq!(
            plan(|s| {
                s.dy = 120.;
                s.collapse = 250.;
            }),
            p(300., true, 70.)
        );
        let full = plan(|s| {
            s.dy = 80.;
            s.collapse = 300.;
            s.inner_below = 500.;
        });
        assert!(!full.consumed && full.collapse == 300.);
        assert!(
            !plan(|s| {
                s.dy = 80.;
                s.report_below = 0.;
            })
            .consumed
        );
        assert_eq!(
            plan(|s| {
                s.dy = 80.;
                s.report_below = 30.;
            })
            .collapse,
            30.
        );
        assert!(
            !plan(|s| {
                s.dy = 80.;
                s.on_card = true;
                s.inner_below = 200.;
            })
            .consumed
        );
        let card = plan(|s| {
            s.dy = 80.;
            s.on_card = true;
        });
        assert_eq!((card.collapse, card.spill), (80., 0.));
    }

    #[test]
    fn scrolling_up_brings_the_card_back_after_the_inner_top() {
        let inner = plan(|s| {
            s.dy = -80.;
            s.collapse = 300.;
            s.inner_top = 400.;
        });
        assert!(!inner.consumed && inner.collapse == 300.);
        assert_eq!(
            plan(|s| {
                s.dy = -80.;
                s.collapse = 300.;
            }),
            p(220., true, 0.)
        );
        assert_eq!(
            plan(|s| {
                s.dy = -500.;
                s.collapse = 100.;
            })
            .collapse,
            0.
        );
        assert!(
            !plan(|s| {
                s.dy = -80.;
            })
            .consumed
        );
    }

    #[test]
    fn edges_clamp_and_no_report_means_no_collapse() {
        assert_eq!(
            plan(|s| {
                s.dy = 10.;
                s.collapse = 500.;
                s.max = 200.;
            })
            .collapse,
            200.
        );
        assert!(
            !plan(|s| {
                s.dy = 80.;
                s.max = 0.;
            })
            .consumed
        );
    }
}
