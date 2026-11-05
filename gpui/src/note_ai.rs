// 노트 AI 실행 스토어 — 지금 앱 lib/noteAiRun.ts. 모달을 닫거나 다른 노트·탭으로 가도 CLI 는 끝까지 돌고,
// 결과는 노트 경로에 매달려 검토를 기다린다(배너, 트리 점, 레일 점이 되돌아갈 길). 노트 하나에 실행 하나 —
// 전문 작성(compose)과 부분 수정(span)이 같은 자리를 쓴다.
//
// 상태 기계(Runs)는 화면과 떨어진 보통 구조체다. CLI 는 tokio(crate::ai)에서 돌고, 글 조각·진행·초안 스냅샷은
// 공유 상자(Live)에 쌓인다. 화면 쪽 작업이 100ms 마다 상자를 들여다보고 옮긴다 — tokio 스레드가 화면을 직접
// 깨우지 않는다(crate::ai::wait 와 같은 이유).
//
// 세대(seq): 중단·재실행 뒤 늦게 오는 조각·결과·에러는 세대가 다르면 버린다. 취소는 비동기라 프로세스가 죽기 전
// 조각이 더 오고, 그게 새 실행 글과 섞이면 한 글자씩 뒤엉킨 결과가 된다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amber_core::Channel;
use amber_core::ai::{self as core_ai, Activity, AiError};
use amber_gpui::note_ai_text::{CONTINUE_INSTRUCTION, TAIL_CHARS, splice_spans, tail_span};
use gpui_kit::*;

use crate::Spike;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Running,
    Done,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Compose,
    Span,
}

/// 한 번의 호출로 고칠 덩어리 — 소스 구간(바이트)과 보일 이름. section = 절 전체(제목 줄을 지킨다)
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub section: bool,
    pub start: usize,
    pub end: usize,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub path: String,
    pub title: String,
    pub kind: Kind,
    pub phase: Phase,
    /// 내가 친 지시와 체크한 칩, 참고 폴더 — '다시 지시'가 되살린다
    pub typed: String,
    pub chosen: Vec<String>,
    pub ref_dirs: Vec<String>,
    /// 시작한 때의 본문 — 검토 때 그 사이 노트가 바뀌었는지 가른다
    pub base: String,
    /// 생성 중 실시간 글(파일 모드에선 초안 폴더 스냅샷)
    pub stream: String,
    pub activity: Option<(String, Option<String>)>,
    pub activity_at: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// 고칠 구간들(부분 수정만), 구간별 새 글, 지금 처리 중인 구간
    pub spans: Vec<Span>,
    pub span_results: Vec<String>,
    pub span_at: usize,
    /// 마지막으로 성공한 결과 = 적용하면 되는 노트 전문. 다시 실행해도 새 결과가 올 때까지 남는다
    pub result: Option<String>,
    pub model: Option<String>,
    pub truncated: bool,
    pub continued: bool,
    /// 잘린 결과를 이어 쓰는 중(phase 는 Running, result 는 그대로)
    pub continuing: bool,
    pub error: Option<String>,
    /// 진행 중인 실행의 취소 키 — 끝나면 None
    pub cancel_key: Option<String>,
}

fn base_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_lowercase();
    if lower.ends_with(".md") {
        name[..name.len() - 3].to_string()
    } else {
        name.to_string()
    }
}

/// 실행 시작 인자 — 전문 작성
pub struct StartCompose {
    pub path: String,
    pub title: String,
    /// 지금 본문(편집 중이면 초안)
    pub markdown: String,
    pub typed: String,
    pub chosen: Vec<String>,
    pub ref_dirs: Vec<String>,
    /// 보낼 최종 지시(내가 친 말 + 체크한 지시)
    pub instruction: String,
}

/// 실행 시작 인자 — 부분 수정. 붙어 있는 절은 호출부가 이미 하나로 묶어서 준다
pub struct StartSpan {
    pub path: String,
    pub title: String,
    pub markdown: String,
    pub spans: Vec<Span>,
    pub instruction: String,
    pub ref_dirs: Vec<String>,
}

#[derive(Default)]
pub struct Runs {
    pub map: HashMap<String, Run>,
    seqs: HashMap<String, u64>,
}

impl Runs {
    pub fn get(&self, path: &str) -> Option<&Run> {
        self.map.get(path)
    }

    pub fn running(&self, path: &str) -> bool {
        self.map
            .get(path)
            .is_some_and(|r| r.phase == Phase::Running)
    }

    /// 어느 노트라도 쓰는 중인지 — 레일의 맥동 점
    pub fn any_running(&self) -> bool {
        self.map.values().any(|r| r.phase == Phase::Running)
    }

    fn bump(&mut self, path: &str) -> u64 {
        let n = self.seqs.get(path).copied().unwrap_or(0) + 1;
        self.seqs.insert(path.to_string(), n);
        n
    }

    pub fn is_live(&self, path: &str, seq: u64) -> bool {
        self.seqs.get(path) == Some(&seq)
    }

    /// 전문 작성을 시작한다 — 같은 노트가 이미 쓰는 중이면 None. 이전 전문 작성 결과는 새 결과가 올 때까지 남긴다
    pub fn begin_compose(&mut self, p: &StartCompose, key: &str, now: i64) -> Option<u64> {
        if self.running(&p.path) {
            return None;
        }
        let seq = self.bump(&p.path);
        let prev = self.map.get(&p.path).filter(|r| r.kind == Kind::Compose);
        let run = Run {
            path: p.path.clone(),
            title: p.title.clone(),
            kind: Kind::Compose,
            phase: Phase::Running,
            typed: p.typed.clone(),
            chosen: p.chosen.clone(),
            ref_dirs: p.ref_dirs.clone(),
            base: p.markdown.clone(),
            stream: String::new(),
            activity: None,
            activity_at: 0,
            started_at: now,
            finished_at: None,
            spans: Vec::new(),
            span_results: Vec::new(),
            span_at: 0,
            result: prev.and_then(|r| r.result.clone()),
            model: prev.and_then(|r| r.model.clone()),
            truncated: prev.is_some_and(|r| r.truncated),
            continued: prev.is_some_and(|r| r.continued),
            continuing: false,
            error: None,
            cancel_key: Some(key.to_string()),
        };
        self.map.insert(p.path.clone(), run);
        Some(seq)
    }

    /// 부분 수정을 시작한다. 같은 구간을 다시 고치는 중이면 이전 결과를 남긴다(재시도가 실패해도 잃지 않게)
    pub fn begin_span(&mut self, p: &StartSpan, key: &str, now: i64) -> Option<u64> {
        if self.running(&p.path) || p.spans.is_empty() {
            return None;
        }
        let seq = self.bump(&p.path);
        let prev = self
            .map
            .get(&p.path)
            .filter(|r| r.kind == Kind::Span && r.spans == p.spans);
        let run = Run {
            path: p.path.clone(),
            title: p.title.clone(),
            kind: Kind::Span,
            phase: Phase::Running,
            typed: p.instruction.clone(),
            chosen: Vec::new(),
            ref_dirs: p.ref_dirs.clone(),
            base: p.markdown.clone(),
            stream: String::new(),
            activity: None,
            activity_at: 0,
            started_at: now,
            finished_at: None,
            spans: p.spans.clone(),
            span_results: prev.map(|r| r.span_results.clone()).unwrap_or_default(),
            span_at: 0,
            result: prev.and_then(|r| r.result.clone()),
            model: None,
            truncated: false,
            continued: false,
            continuing: false,
            error: None,
            cancel_key: Some(key.to_string()),
        };
        self.map.insert(p.path.clone(), run);
        Some(seq)
    }

    /// 잘린 결과 이어 쓰기 — (세대, 받아 둔 결과, 끝 조각)
    pub fn begin_continue(
        &mut self,
        path: &str,
        key: &str,
        now: i64,
    ) -> Option<(u64, String, String)> {
        let base = self
            .map
            .get(path)
            .filter(|r| r.phase != Phase::Running)
            .and_then(|r| r.result.clone())?;
        let mut tail = tail_span(&base, TAIL_CHARS);
        // 끝 조각은 받아 둔 결과의 꼬리여야 되끼울 수 있다 — 서로게이트 한가운데서 잘렸으면 다음 글자부터
        if !base.ends_with(&tail) {
            tail = tail.trim_start_matches('\u{fffd}').to_string();
        }
        if !base.ends_with(&tail) {
            tail = base.clone();
        }
        let seq = self.bump(path);
        let r = self.map.get_mut(path)?;
        r.phase = Phase::Running;
        r.continuing = true;
        r.stream.clear();
        r.activity = None;
        r.activity_at = 0;
        r.started_at = now;
        r.error = None;
        r.cancel_key = Some(key.to_string());
        Some((seq, base, tail))
    }

    /// 상자에 쌓인 것을 옮긴다 — 세대가 다르면(버려진 실행) 아무것도 안 한다. 바뀐 게 있으면 true
    pub fn pump(&mut self, path: &str, seq: u64, live: &Live) -> bool {
        if !self.is_live(path, seq) || !live.changed {
            return false;
        }
        let Some(r) = self.map.get_mut(path) else {
            return false;
        };
        r.stream = live.draft.clone().unwrap_or_else(|| live.stream.clone());
        if let Some((a, at)) = &live.activity {
            r.activity = Some(a.clone());
            r.activity_at = *at;
        }
        r.span_at = live.span_at;
        true
    }

    fn finish(&mut self, path: &str, seq: u64, now: i64, f: impl FnOnce(&mut Run)) -> bool {
        if !self.is_live(path, seq) {
            return false;
        }
        let Some(r) = self.map.get_mut(path) else {
            return false;
        };
        f(r);
        r.finished_at = Some(now);
        r.stream.clear();
        r.cancel_key = None;
        r.continuing = false;
        true
    }

    /// 성공 — 레일 점을 켤지(성공으로 넘어갔나) 돌려준다
    pub fn finish_ok(&mut self, path: &str, seq: u64, out: Outcome, now: i64) -> bool {
        self.finish(path, seq, now, |r| {
            match out {
                Outcome::Compose {
                    markdown,
                    truncated,
                    continued,
                    model,
                } => {
                    r.result = Some(markdown);
                    r.truncated = truncated;
                    r.continued = continued;
                    r.model = Some(model);
                }
                Outcome::Span { texts, truncated } => {
                    let spans: Vec<(usize, usize)> =
                        r.spans.iter().map(|s| (s.start, s.end)).collect();
                    let opt: Vec<Option<String>> = texts.iter().cloned().map(Some).collect();
                    r.result = Some(splice_spans(&r.base, &spans, &opt));
                    r.span_results = texts;
                    r.truncated = truncated;
                }
                Outcome::Continue {
                    text,
                    tail_len,
                    truncated,
                } => {
                    let base = r.result.clone().unwrap_or_default();
                    r.result = Some(format!("{}{text}", &base[..base.len() - tail_len]));
                    r.truncated = truncated;
                }
            }
            r.phase = Phase::Done;
            r.error = None;
        })
    }

    /// 실패 — 잘린 결과라도 남겨 둔다
    pub fn finish_err(&mut self, path: &str, seq: u64, error: String, now: i64) {
        self.finish(path, seq, now, |r| {
            r.phase = Phase::Error;
            r.error = Some(error);
        });
    }

    /// 중단 — 바로 정리하고 죽일 키를 돌려준다. 받아 둔 결과가 있으면 그리로, 없으면 실행을 지운다(지시 화면으로)
    pub fn stop(&mut self, path: &str) -> Option<String> {
        let r = self.map.get(path).filter(|r| r.phase == Phase::Running)?;
        let key = r.cancel_key.clone();
        let has = r.result.is_some();
        self.bump(path);
        if has {
            let r = self.map.get_mut(path)?;
            r.phase = Phase::Done;
            r.continuing = false;
            r.stream.clear();
            r.error = None;
            r.cancel_key = None;
        } else {
            self.map.remove(path);
        }
        key
    }

    /// 통째로 버린다(결과 포함) — 진행 중이면 죽일 키를 돌려준다
    pub fn dismiss(&mut self, path: &str) -> Option<String> {
        let r = self.map.remove(path)?;
        if r.phase == Phase::Running {
            self.bump(path);
            return r.cancel_key;
        }
        None
    }

    /// 실패 알림만 치운다 — 이전 결과가 있으면 그 결과로, 없으면 실행을 지운다
    pub fn dismiss_error(&mut self, path: &str) {
        let Some(r) = self.map.get_mut(path).filter(|r| r.phase == Phase::Error) else {
            return;
        };
        if r.result.is_some() {
            r.phase = Phase::Done;
            r.error = None;
        } else {
            self.map.remove(path);
        }
    }

    /// 소스 탭에서 결과를 손으로 고쳤다
    pub fn set_result(&mut self, path: &str, md: String) {
        if let Some(r) = self.map.get_mut(path).filter(|r| r.phase != Phase::Running) {
            r.result = Some(md);
        }
    }

    /// 부분 수정 검토에서 조각 하나를 손으로 고쳤다 — 전문도 다시 끼운다
    pub fn set_span_result(&mut self, path: &str, index: usize, text: String) {
        let Some(r) = self
            .map
            .get_mut(path)
            .filter(|r| r.phase != Phase::Running && r.kind == Kind::Span)
        else {
            return;
        };
        if index >= r.span_results.len() {
            r.span_results.resize(index + 1, String::new());
        }
        r.span_results[index] = text;
        let spans: Vec<(usize, usize)> = r.spans.iter().map(|s| (s.start, s.end)).collect();
        let opt: Vec<Option<String>> = r.span_results.iter().cloned().map(Some).collect();
        r.result = Some(splice_spans(&r.base, &spans, &opt));
    }

    /// 노트·폴더 이름 바꾸기/옮기기를 따라간다 — 키가 경로라 안 옮기면 결과가 고아가 된다
    pub fn remap(&mut self, old: &str, new: &str, is_dir: bool) {
        let prefix = format!("{old}/");
        let moves: Vec<(String, String)> = self
            .map
            .keys()
            .filter_map(|p| {
                if p == old {
                    Some((p.clone(), new.to_string()))
                } else if is_dir && p.starts_with(&prefix) {
                    Some((p.clone(), format!("{new}{}", &p[old.len()..])))
                } else {
                    None
                }
            })
            .collect();
        for (from, to) in moves {
            if let Some(mut r) = self.map.remove(&from) {
                r.title = base_name(&to);
                r.path = to.clone();
                self.map.insert(to.clone(), r);
            }
            if let Some(s) = self.seqs.remove(&from) {
                self.seqs.insert(to, s);
            }
        }
    }

    /// 노트·폴더 지우기 — 그 아래 실행은 끊고 지운다. 죽일 키들을 돌려준다
    pub fn drop_under(&mut self, path: &str) -> Vec<String> {
        let prefix = format!("{path}/");
        let gone: Vec<String> = self
            .map
            .keys()
            .filter(|p| *p == path || p.starts_with(&prefix))
            .cloned()
            .collect();
        let mut keys = Vec::new();
        for p in gone {
            self.bump(&p);
            if let Some(r) = self.map.remove(&p)
                && r.phase == Phase::Running
                && let Some(k) = r.cancel_key
            {
                keys.push(k);
            }
        }
        keys
    }
}

/// 끝난 실행의 결과
pub enum Outcome {
    Compose {
        markdown: String,
        truncated: bool,
        continued: bool,
        model: String,
    },
    Span {
        texts: Vec<String>,
        truncated: bool,
    },
    Continue {
        text: String,
        /// 받아 둔 결과에서 떼어 낼 끝 조각의 길이(바이트)
        tail_len: usize,
        truncated: bool,
    },
}

/// tokio 쪽이 쌓고 화면 쪽이 옮기는 상자
#[derive(Default)]
pub struct Live {
    pub stream: String,
    /// 초안 폴더 스냅샷 — 한 번 오면 글 조각은 무시한다(파일 모드의 조각은 "DONE 5" 같은 마무리 한 줄뿐)
    pub draft: Option<String>,
    pub activity: Option<((String, Option<String>), i64)>,
    pub span_at: usize,
    pub changed: bool,
}

type LiveBox = Arc<Mutex<Live>>;

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn channels(live: &LiveBox) -> (Channel<String>, Channel<Activity>) {
    let (a, b) = (live.clone(), live.clone());
    let delta = Channel::new(move |d: String| {
        if let Ok(mut l) = a.lock()
            && l.draft.is_none()
        {
            l.stream.push_str(&d);
            l.changed = true;
        }
    });
    let activity = Channel::new(move |x: Activity| {
        if let Ok(mut l) = b.lock() {
            l.activity = Some(((x.tool, x.target), now_ms()));
            l.changed = true;
        }
    });
    (delta, activity)
}

fn new_key() -> String {
    format!("note-ai-{}", amber_gpui::comments::new_comment_id())
}

/// 연결 설정 — (모델, CLI 경로, 공급자, 출력 언어). 연결이 없으면 None
fn ai_args(
    db: &amber_gpui::db::Db,
) -> Option<(Option<String>, Option<String>, Option<String>, String)> {
    let c = amber_gpui::ai_config::load_config(db)
        .ok()
        .filter(|c| c.provider.is_some())?;
    Some((
        Some(c.model.clone()).filter(|m| !m.is_empty()),
        Some(c.cli_path.clone()).filter(|p| !p.is_empty()),
        c.provider.clone(),
        amber_gpui::ai_config::output_lang(&c),
    ))
}

impl Spike {
    /// 전문 작성 시작 — 화면과 무관하게 끝까지 돌아 결과를 스토어에 남긴다
    pub(crate) fn start_note_ai(&mut self, p: StartCompose, cx: &mut Context<Self>) {
        let Some((model, cli, provider, lang)) = ai_args(&self.db) else {
            return;
        };
        let key = new_key();
        let Some(seq) = self.notes.ai.begin_compose(&p, &key, now_ms()) else {
            return;
        };
        let live: LiveBox = Arc::default();
        let (on_delta, on_activity) = channels(&live);
        let d = live.clone();
        let on_draft = Channel::new(move |full: String| {
            if let Ok(mut l) = d.lock() {
                l.draft = Some(full);
                l.changed = true;
            }
        });
        let rx = crate::ai::spawn(async move {
            core_ai::ai_note_compose_stream(
                p.title,
                p.markdown,
                p.instruction,
                model,
                cli,
                provider,
                None,
                Some(lang),
                Some(p.ref_dirs),
                on_delta,
                on_activity,
                on_draft,
                Some(key),
            )
            .await
            .map(|r| Outcome::Compose {
                markdown: r.markdown,
                truncated: r.meta.truncated,
                continued: r.meta.continued,
                model: r.meta.model,
            })
        });
        self.pump_note_ai(p.path, seq, live, rx, cx);
    }

    /// 부분 수정 시작 — 구간마다 차례로 부르고, 받은 조각을 뒤에서부터 되끼워 전문으로 남긴다
    /// (차례로 도는 이유는 화면이다 — 실시간 글 상자가 하나라 동시에 흘리면 글자가 섞인다)
    pub(crate) fn start_span_ai(&mut self, p: StartSpan, cx: &mut Context<Self>) {
        let Some((model, cli, provider, lang)) = ai_args(&self.db) else {
            return;
        };
        let key = new_key();
        let Some(seq) = self.notes.ai.begin_span(&p, &key, now_ms()) else {
            return;
        };
        let live: LiveBox = Arc::default();
        let stopped = Arc::new(AtomicBool::new(false));
        self.notes.ai_stops.insert(key.clone(), stopped.clone());
        let l = live.clone();
        let rx = crate::ai::spawn(async move {
            let mut texts = Vec::new();
            let mut truncated = false;
            for (i, s) in p.spans.iter().enumerate() {
                if stopped.load(Ordering::SeqCst) {
                    return Err(AiError::new("CANCELLED", ""));
                }
                if let Ok(mut x) = l.lock() {
                    x.span_at = i;
                    x.stream.clear();
                    x.changed = true;
                }
                let (on_delta, on_activity) = channels(&l);
                let r = core_ai::ai_note_edit_span(
                    p.title.clone(),
                    p.markdown.clone(),
                    p.markdown[s.start..s.end].to_string(),
                    p.instruction.clone(),
                    Some(if s.section { "section" } else { "selection" }.to_string()),
                    model.clone(),
                    cli.clone(),
                    provider.clone(),
                    None,
                    Some(lang.clone()),
                    Some(p.ref_dirs.clone()),
                    on_delta,
                    on_activity,
                    Some(key.clone()),
                )
                .await?;
                truncated |= r.meta.truncated;
                texts.push(r.text);
            }
            Ok(Outcome::Span { texts, truncated })
        });
        self.pump_note_ai(p.path, seq, live, rx, cx);
    }

    /// 잘린 결과 이어 쓰기 — 끝 조각(~700자)을 넘겨 "조각 + 이어질 내용"을 받아 그 자리에 되끼운다.
    /// 전문을 다시 받지 않으니 상한에 다시 걸리지 않고, 앞부분은 한 글자도 바뀌지 않는다
    pub(crate) fn continue_note_ai(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some((model, cli, provider, lang)) = ai_args(&self.db) else {
            return;
        };
        let key = new_key();
        let Some((seq, base, tail)) = self.notes.ai.begin_continue(path, &key, now_ms()) else {
            return;
        };
        let (title, dirs) = self
            .notes
            .ai
            .get(path)
            .map(|r| (r.title.clone(), r.ref_dirs.clone()))
            .unwrap_or_default();
        let live: LiveBox = Arc::default();
        let (on_delta, on_activity) = channels(&live);
        let tail_len = tail.len();
        let rx = crate::ai::spawn(async move {
            core_ai::ai_note_edit_span(
                title,
                base,
                tail,
                CONTINUE_INSTRUCTION.to_string(),
                Some("selection".into()),
                model,
                cli,
                provider,
                None,
                Some(lang),
                Some(dirs),
                on_delta,
                on_activity,
                Some(key),
            )
            .await
            .map(|r| Outcome::Continue {
                text: r.text,
                tail_len,
                truncated: r.meta.truncated,
            })
        });
        self.pump_note_ai(path.to_string(), seq, live, rx, cx);
    }

    /// 100ms 마다 상자를 옮기고, 끝나면 결과를 남긴다
    fn pump_note_ai(
        &mut self,
        path: String,
        seq: u64,
        live: LiveBox,
        mut rx: futures::channel::oneshot::Receiver<Result<Outcome, AiError>>,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let mut tick = 0u32;
            loop {
                ex.timer(Duration::from_millis(100)).await;
                tick += 1;
                let done = match rx.try_recv() {
                    Ok(None) => None,
                    Ok(Some(r)) => Some(Some(r)),
                    Err(_) => Some(None),
                };
                let keep = this.update(cx, |s, cx| {
                    let path = path.clone();
                    if let Ok(mut l) = live.lock()
                        && s.notes.ai.pump(&path, seq, &l)
                    {
                        l.changed = false;
                        cx.notify();
                    }
                    // 대기 한 줄은 4초마다 바뀌고 1분, 3분에 꼬리가 붙는다 — 1초마다 다시 그린다(useAiWaitLine)
                    if tick.is_multiple_of(10) {
                        cx.notify();
                    }
                    let Some(done) = done else {
                        return true;
                    };
                    let now = now_ms();
                    match done {
                        Some(Ok(out)) => {
                            if s.notes.ai.finish_ok(&path, seq, out, now) {
                                s.mark_rail_done_notes();
                            }
                        }
                        Some(Err(e)) => {
                            s.notes
                                .ai
                                .finish_err(&path, seq, crate::ai::err_text(&e), now)
                        }
                        None => {}
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

    /// 중단 — CLI 를 죽이고 기다리지 않고 바로 정리한다(실제로 끝나길 기다리면 누른 뒤에도 몇 초간 글자가 흐른다)
    pub(crate) fn stop_note_ai(&mut self, path: &str) {
        if let Some(key) = self.notes.ai.stop(path) {
            self.kill_note_ai(&key);
        }
    }

    pub(crate) fn dismiss_note_ai(&mut self, path: &str) {
        if let Some(key) = self.notes.ai.dismiss(path) {
            self.kill_note_ai(&key);
        }
    }

    /// 노트·폴더를 지웠다 — 그 아래 실행은 끊는다
    pub(crate) fn drop_note_ai_under(&mut self, path: &str) {
        for key in self.notes.ai.drop_under(path) {
            self.kill_note_ai(&key);
        }
    }

    fn kill_note_ai(&mut self, key: &str) {
        if let Some(flag) = self.notes.ai_stops.remove(key) {
            flag.store(true, Ordering::SeqCst);
        }
        core_ai::ai_cancel(key.to_string());
    }

    /// 성공으로 끝났다 — 노트 탭을 보고 있지 않으면 레일에 완료 점(그 탭에 들어가면 꺼진다)
    fn mark_rail_done_notes(&mut self) {
        if self.section != crate::Section::Notes {
            self.notes.rail_done = true;
        }
    }
}

#[cfg(test)]
mod tests {
    // super::* 는 gpui_kit 의 test 매크로까지 끌어와 #[test] 를 가린다
    use super::{Outcome, Phase, Runs, Span, StartCompose, StartSpan, base_name};

    fn compose(path: &str) -> StartCompose {
        StartCompose {
            path: path.into(),
            title: base_name(path),
            markdown: "# old\n".into(),
            typed: "make it better".into(),
            chosen: vec!["p:0".into()],
            ref_dirs: vec![],
            instruction: "make it better".into(),
        }
    }

    fn done(md: &str) -> Outcome {
        Outcome::Compose {
            markdown: md.into(),
            truncated: false,
            continued: false,
            model: "m".into(),
        }
    }

    #[test]
    fn one_run_per_note_and_late_results_are_dropped() {
        let mut r = Runs::default();
        let s1 = r.begin_compose(&compose("a.md"), "k1", 1).unwrap();
        assert!(
            r.begin_compose(&compose("a.md"), "k2", 2).is_none(),
            "쓰는 중이면 무시"
        );
        // 중단 — 받은 결과가 없으니 실행이 사라진다
        assert_eq!(r.stop("a.md").as_deref(), Some("k1"));
        assert!(r.get("a.md").is_none());
        let s2 = r.begin_compose(&compose("a.md"), "k3", 3).unwrap();
        assert!(
            !r.finish_ok("a.md", s1, done("late"), 4),
            "버려진 세대의 결과"
        );
        assert!(r.finish_ok("a.md", s2, done("# new\n"), 5));
        let run = r.get("a.md").unwrap();
        assert_eq!(
            (run.phase, run.result.as_deref()),
            (Phase::Done, Some("# new\n"))
        );
        assert_eq!(run.cancel_key, None);
    }

    #[test]
    fn rerun_keeps_the_previous_result_until_a_new_one_lands() {
        let mut r = Runs::default();
        let s = r.begin_compose(&compose("a.md"), "k", 1).unwrap();
        r.finish_ok("a.md", s, done("first"), 2);
        let s = r.begin_compose(&compose("a.md"), "k2", 3).unwrap();
        assert_eq!(r.get("a.md").unwrap().result.as_deref(), Some("first"));
        r.finish_err("a.md", s, "boom".into(), 4);
        let run = r.get("a.md").unwrap();
        assert_eq!(
            (run.phase, run.result.as_deref()),
            (Phase::Error, Some("first"))
        );
        r.dismiss_error("a.md");
        assert_eq!(
            r.get("a.md").unwrap().phase,
            Phase::Done,
            "이전 결과로 돌아간다"
        );
        // 중단해도 받아 둔 결과로 돌아간다
        r.begin_compose(&compose("a.md"), "k3", 5).unwrap();
        r.stop("a.md");
        assert_eq!(r.get("a.md").unwrap().result.as_deref(), Some("first"));
    }

    #[test]
    fn spans_splice_from_the_back_and_continue_replaces_the_tail() {
        let mut r = Runs::default();
        let md = "# A\nold a\n# B\nold b\n";
        let p = StartSpan {
            path: "n.md".into(),
            title: "n".into(),
            markdown: md.into(),
            spans: vec![
                Span {
                    section: true,
                    start: 0,
                    end: 10,
                    label: "A".into(),
                },
                Span {
                    section: true,
                    start: 10,
                    end: md.len(),
                    label: "B".into(),
                },
            ],
            instruction: "x".into(),
            ref_dirs: vec![],
        };
        let s = r.begin_span(&p, "k", 1).unwrap();
        r.finish_ok(
            "n.md",
            s,
            Outcome::Span {
                texts: vec!["# A\nnew a".into(), "# B\nnew b".into()],
                truncated: false,
            },
            2,
        );
        assert_eq!(
            r.get("n.md").unwrap().result.as_deref(),
            Some("# A\nnew a\n# B\nnew b\n")
        );
        r.set_span_result("n.md", 1, "# B\nhand".into());
        assert_eq!(
            r.get("n.md").unwrap().result.as_deref(),
            Some("# A\nnew a\n# B\nhand\n")
        );
        // 이어 쓰기 — 끝 조각을 떼고 받은 글로
        let (s, base, tail) = r.begin_continue("n.md", "k2", 3).unwrap();
        assert_eq!(base, tail, "짧으면 전부가 끝 조각");
        r.finish_ok(
            "n.md",
            s,
            Outcome::Continue {
                text: "WHOLE".into(),
                tail_len: tail.len(),
                truncated: false,
            },
            4,
        );
        assert_eq!(r.get("n.md").unwrap().result.as_deref(), Some("WHOLE"));
    }

    #[test]
    fn renames_and_deletes_follow_the_tree() {
        let mut r = Runs::default();
        r.begin_compose(&compose("CS/a.md"), "k1", 1);
        r.begin_compose(&compose("CS수업.md"), "k2", 1);
        r.remap("CS", "IT", true);
        assert!(r.get("IT/a.md").is_some());
        assert_eq!(r.get("IT/a.md").unwrap().title, "a");
        assert!(r.get("CS수업.md").is_some(), "이름이 겹치기만 한 건 그대로");
        r.remap("IT/a.md", "IT/b.md", false);
        assert_eq!(r.get("IT/b.md").unwrap().title, "b");
        assert_eq!(r.drop_under("IT"), vec!["k1".to_string()]);
        assert!(r.get("IT/b.md").is_none());
        assert!(r.running("CS수업.md"));
    }
}
