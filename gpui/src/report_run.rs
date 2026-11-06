// 리포트 생성 실행 스토어 — 지금 앱 lib/reportRun.ts. 날짜(주간은 `w:<주 시작>`)마다 실행 하나.
// 탭이나 날짜를 바꿔도 생성은 끝까지 돌아 파일과 행을 남긴다 — 패널은 들여다보기만 한다.
//
// 상태 기계(Runs)는 화면과 떨어진 보통 구조체다. 수집과 생성은 tokio(crate::ai)에서 돌고, 수집 진행과 글 조각은
// 공유 상자(Live)에 쌓인다. 화면 쪽 작업이 100ms 마다 상자를 옮긴다(note_ai 와 같은 이유).
// 세대(seq): 지우기(clear) 뒤 늦게 오는 결과는 세대가 달라 버린다.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amber_core::Channel;
use amber_core::ai::{self as core_ai, AiError};
use amber_core::report::{
    CollectProgress, GithubCfg, McpSource, SessionsCfg, SourceDigest, WeeklyDay,
};
use amber_gpui::db::{Db, Row};
use amber_gpui::report;
use gpui_kit::*;
use serde_json::{Value as Json, json};

use crate::Spike;

/// 주간 실행 키 — 같은 스토어를 접두어로 나눈다. 달력 점은 일간만 본다
pub fn weekly_key(monday: &str) -> String {
    format!("w:{monday}")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Collecting,
    Streaming,
    Done,
    Empty,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChipStatus {
    Ok,
    Pending,
    Error,
    Mcp,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Chip {
    pub id: String,
    pub status: ChipStatus,
    pub items: u32,
    /// 실패 사유 — 백엔드가 준 유일한 실행 가능한 안내("gh auth login …")라 말풍선으로 남긴다
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub phase: Phase,
    /// 중단 단추가 쓰는 취소 키 — 생성(흐름) 단계에서만 있다
    pub cancel_key: Option<String>,
    pub stream: String,
    pub chips: Vec<Chip>,
    pub error: Option<String>,
    /// 끝났을 때의 행(일간 daily_reports, 주간 weekly_reports)
    pub report: Option<Row>,
    pub body: String,
}

impl Run {
    fn fresh(phase: Phase) -> Self {
        Run {
            phase,
            cancel_key: None,
            stream: String::new(),
            chips: Vec::new(),
            error: None,
            report: None,
            body: String::new(),
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Collecting | Phase::Streaming)
    }
}

/// tokio 쪽이 쌓고 화면 쪽이 옮기는 상자
#[derive(Default)]
pub struct Live {
    pub progress: Vec<CollectProgress>,
    pub stream: String,
    /// 수집이 끝나 생성으로 넘어갔다
    pub streaming: bool,
    pub changed: bool,
}

type LiveBox = Arc<Mutex<Live>>;

#[derive(Default)]
pub struct Runs {
    runs: HashMap<String, Run>,
    seq: HashMap<String, u64>,
}

impl Runs {
    pub fn get(&self, key: &str) -> Option<&Run> {
        self.runs.get(key)
    }

    pub fn running(&self, key: &str) -> bool {
        self.runs.get(key).is_some_and(Run::running)
    }

    /// 레일의 생성 중 점 — 주간도 센다
    pub fn any_running(&self) -> bool {
        self.runs.values().any(Run::running)
    }

    /// 지금 생성 중인 날짜들(주간 빼고) — 달력이 막대를 깜빡인다
    pub fn running_dates(&self) -> Vec<String> {
        self.runs
            .iter()
            .filter(|(k, r)| r.running() && !k.starts_with("w:"))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// 새 실행 — 같은 키가 돌고 있으면 None. 이전 실행(끝났거나 실패한 것)은 통째로 갈아 끼운다
    pub fn begin(&mut self, key: &str, phase: Phase, chips: Vec<Chip>) -> Option<u64> {
        if self.running(key) {
            return None;
        }
        let seq = self.seq.entry(key.to_string()).or_default();
        *seq += 1;
        let mut run = Run::fresh(phase);
        run.chips = chips;
        self.runs.insert(key.to_string(), run);
        Some(*seq)
    }

    pub fn is_live(&self, key: &str, seq: u64) -> bool {
        self.seq.get(key) == Some(&seq) && self.runs.contains_key(key)
    }

    /// 상자의 진행(칩), 단계, 글을 옮긴다. 바뀌었으면 true
    pub fn pump(&mut self, key: &str, seq: u64, live: &Live, cancel_key: &str) -> bool {
        if !live.changed || !self.is_live(key, seq) {
            return false;
        }
        let Some(run) = self.runs.get_mut(key) else {
            return false;
        };
        for p in &live.progress {
            if let Some(c) = run.chips.iter_mut().find(|c| c.id == p.id) {
                *c = Chip {
                    id: c.id.clone(),
                    status: if p.ok {
                        ChipStatus::Ok
                    } else {
                        ChipStatus::Error
                    },
                    items: p.items,
                    error: p.error.clone(),
                };
            }
        }
        if live.streaming && run.phase == Phase::Collecting {
            run.phase = Phase::Streaming;
            run.cancel_key = Some(cancel_key.to_string());
        }
        run.stream.clone_from(&live.stream);
        true
    }

    /// 끝 — 성공(done), 재료 없음(empty), 실패(error). 세대가 다르면 버린다. 성공으로 넘어가는 순간이면 true(레일 점)
    pub fn finish(&mut self, key: &str, seq: u64, end: End) -> bool {
        if !self.is_live(key, seq) {
            return false;
        }
        let Some(run) = self.runs.get_mut(key) else {
            return false;
        };
        let was = run.phase;
        run.cancel_key = None;
        match end {
            End::Done { body, report } => {
                run.phase = Phase::Done;
                run.body = body;
                run.report = report;
            }
            End::Empty => run.phase = Phase::Empty,
            End::Error(e) => {
                run.phase = Phase::Error;
                run.error = Some(e);
            }
        }
        run.phase == Phase::Done && was != Phase::Done
    }

    /// 지우기, 실패 모달 닫기, 직접 고쳐 저장 — 디스크가 정본이 된다
    pub fn clear(&mut self, key: &str) -> bool {
        self.runs.remove(key).is_some()
    }

    /// 중단 단추 — 취소 키를 돌려준다. CLI 가 죽으면 생성이 에러로 빠져 실패로 마감된다(따로 상태를 바꾸지 않는다)
    pub fn cancel_key(&self, key: &str) -> Option<String> {
        self.runs.get(key)?.cancel_key.clone()
    }
}

pub enum End {
    Done { body: String, report: Option<Row> },
    Empty,
    Error(String),
}

/// 리포트 상태 — 실행 스토어, 레일 완료 점, 패널
#[derive(Default)]
pub struct Reports {
    pub runs: Runs,
    pub rail_done: bool,
    /// 패널(일간/주간) — 읽은 리포트, 확인 창, 편집, 카드 묶음
    pub panel: crate::report_view::Panel,
}

fn new_key() -> String {
    format!("report-{}", amber_gpui::comments::new_comment_id())
}

/// 그날 0시 ~ 다음 날 0시(로컬, ms) — dayRangeMs
fn day_range_ms(date: &str) -> Option<(i64, i64)> {
    use chrono::TimeZone;
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let at = |d: chrono::NaiveDate| {
        chrono::Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
            .earliest()
            .map(|t| t.timestamp_millis())
    };
    Some((at(d)?, at(d.succ_opt()?)?))
}

/// getTimezoneOffset — UTC 에서 로컬을 뺀 분(서울은 -540)
fn tz_offset_min() -> i32 {
    -chrono::Local::now().offset().local_minus_utc() / 60
}

/// 일간 생성의 재료 — 화면 스레드에서 DB 로 모은다(Db 는 tokio 로 못 넘긴다)
struct DailyPlan {
    todos_md: String,
    todos_n: u32,
    github: Option<GithubCfg>,
    sessions: Option<SessionsCfg>,
    mcp: Vec<(String, u8, String)>,
    context: String,
    chips: Vec<Chip>,
}

fn daily_plan(db: &Db, date: &str, provider: Option<&str>) -> Result<DailyPlan, String> {
    let rc = report::load_report_config(db).map_err(|e| e.to_string())?;
    let digest = report::build_todos_digest(db, date).map_err(|e| e.to_string())?;
    let todos_md = digest["md"].as_str().unwrap_or_default().to_string();
    let todos_n = digest["count"].as_u64().unwrap_or_default() as u32;
    let ranked = report::ranked_sources(&rc);
    let rank = |id: &str| ranked.iter().find(|(s, _)| s == id).map(|(_, r)| *r);
    let opt =
        |k: &str| Some(rc[k].as_str().unwrap_or_default().to_string()).filter(|s| !s.is_empty());
    let github = rank("github").map(|rank| GithubCfg {
        rank,
        path: opt("githubPath"),
        repos: rc["githubRepos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect(),
        account: opt("githubAccount"),
    });
    let sessions = rank("ai_sessions").map(|rank| SessionsCfg {
        rank,
        claude: rc["sessionsClaude"].as_bool().unwrap_or(true),
        codex: rc["sessionsCodex"].as_bool().unwrap_or(true),
    });
    // MCP 위임은 claude 경로 전용
    let mcp = if provider == Some("claude") {
        report::mcp_sources_from(&rc)
    } else {
        Vec::new()
    };
    let mut chips = vec![Chip {
        id: "todos".into(),
        status: ChipStatus::Ok,
        items: todos_n,
        error: None,
    }];
    for (id, _) in &ranked {
        if id == "github" || id == "ai_sessions" {
            chips.push(Chip {
                id: id.clone(),
                status: ChipStatus::Pending,
                items: 0,
                error: None,
            });
        }
    }
    for (id, _, _) in &mcp {
        chips.push(Chip {
            id: id.clone(),
            status: ChipStatus::Mcp,
            items: 0,
            error: None,
        });
    }
    Ok(DailyPlan {
        todos_md,
        todos_n,
        github,
        sessions,
        mcp,
        context: rc["context"].as_str().unwrap_or_default().to_string(),
        chips,
    })
}

/// tokio 에서 돌아온 것 — 일간은 수집 결과까지, 주간은 묶은 날짜
enum Outcome {
    Daily {
        digests: Vec<SourceDigest>,
        markdown: String,
        model: String,
        duration_ms: Option<i64>,
    },
    Weekly {
        dates: Vec<String>,
        markdown: String,
        model: String,
        duration_ms: Option<i64>,
    },
    Empty,
}

/// 일간 sources_json — 수집 소스 다음 MCP(앱이 결과를 모르니 ok, 0건, mcp 표시)
fn sources_json(digests: &[SourceDigest], mcp: &[(String, u8, String)]) -> String {
    let mut all: Vec<Json> = digests
        .iter()
        .map(|d| json!({ "id": d.id, "rank": d.rank, "ok": d.ok, "items": d.items, "error": d.error }))
        .collect();
    all.extend(mcp.iter().map(
        |(id, rank, _)| json!({ "id": id, "rank": rank, "ok": true, "items": 0, "mcp": true, "error": null }),
    ));
    Json::from(all).to_string()
}

impl Spike {
    /// 일간 리포트 생성 — 게이트(연결, 첫 설정)는 패널이 통과시킨 뒤 부른다
    pub(crate) fn start_report(&mut self, date: &str, cx: &mut Context<Self>) {
        let Some((model, cli, provider, lang)) = crate::note_ai::ai_args(&self.db) else {
            return;
        };
        if self.reports.runs.running(date) {
            return;
        }
        let plan = match daily_plan(&self.db, date, provider.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                if let Some(seq) = self.reports.runs.begin(date, Phase::Collecting, Vec::new()) {
                    self.reports.runs.finish(date, seq, End::Error(e));
                }
                cx.notify();
                return;
            }
        };
        let Some((start, end)) = day_range_ms(date) else {
            return;
        };
        let Some(seq) = self
            .reports
            .runs
            .begin(date, Phase::Collecting, plan.chips.clone())
        else {
            return;
        };
        let live: LiveBox = Arc::default();
        let key = new_key();
        let (p, d, s) = (live.clone(), live.clone(), live.clone());
        let on_progress = Channel::new(move |x: CollectProgress| {
            if let Ok(mut l) = p.lock() {
                l.progress.push(x);
                l.changed = true;
            }
        });
        let on_delta = Channel::new(move |t: String| {
            if let Ok(mut l) = d.lock() {
                l.stream.push_str(&t);
                l.changed = true;
            }
        });
        let date_s = date.to_string();
        let cancel = key.clone();
        let mcp = plan.mcp.clone();
        let rx = crate::ai::spawn(async move {
            let todos = Some(plan.todos_md.clone()).filter(|t| !t.is_empty());
            let digests = amber_core::report::report_collect(
                date_s.clone(),
                start,
                end,
                tz_offset_min(),
                plan.github,
                plan.sessions,
                todos,
                on_progress,
            )
            .await?;
            let active =
                plan.todos_n > 0 || digests.iter().any(|d| d.ok && d.items > 0) || !mcp.is_empty();
            if !active {
                return Ok(Outcome::Empty);
            }
            if let Ok(mut l) = s.lock() {
                l.streaming = true;
                l.changed = true;
            }
            let r = amber_core::report::report_generate(
                date_s,
                plan.todos_md,
                digests.clone(),
                mcp.iter()
                    .map(|(id, rank, server)| McpSource {
                        id: id.clone(),
                        rank: *rank,
                        server: server.clone(),
                    })
                    .collect(),
                model,
                cli,
                provider,
                None,
                Some(lang),
                Some(plan.context),
                on_delta,
                Some(cancel),
            )
            .await?;
            Ok(Outcome::Daily {
                digests,
                markdown: r.markdown,
                model: r.meta.model,
                duration_ms: r.meta.duration_ms,
            })
        });
        self.pump_report(date.to_string(), seq, key, live, plan.mcp, rx, cx);
    }

    /// 주간 리포트 생성 — 재료는 그 주의 일간 본문이라 수집 단계 없이 바로 흐름으로 간다
    pub(crate) fn start_weekly_report(
        &mut self,
        week_start: &str,
        week_end: &str,
        cx: &mut Context<Self>,
    ) {
        let Some((model, cli, provider, lang)) = crate::note_ai::ai_args(&self.db) else {
            return;
        };
        let wkey = weekly_key(week_start);
        let Some(seq) = self.reports.runs.begin(&wkey, Phase::Streaming, Vec::new()) else {
            return;
        };
        let days: Vec<WeeklyDay> = report::load_week_sources(&self.db, week_start)
            .into_iter()
            .filter_map(|d| {
                let body = d
                    .body
                    .filter(|b| !amber_gpui::vault::js_trim(b).is_empty())?;
                Some(WeeklyDay {
                    date: d.date,
                    weekday: d.weekday,
                    body,
                })
            })
            .collect();
        if days.is_empty() {
            self.reports.runs.finish(&wkey, seq, End::Empty);
            cx.notify();
            return;
        }
        let rc = report::load_report_config(&self.db).unwrap_or_default();
        let text = |k: &str| rc[k].as_str().unwrap_or_default().to_string();
        let (display, context) = (text("displayName"), text("context"));
        let key = new_key();
        // 수집이 없으니 취소 키는 처음부터 — 상자도 바로 '흐름'
        if let Some(run) = self.reports.runs.runs.get_mut(&wkey) {
            run.cancel_key = Some(key.clone());
        }
        let live: LiveBox = Arc::new(Mutex::new(Live {
            streaming: true,
            ..Live::default()
        }));
        let d = live.clone();
        let on_delta = Channel::new(move |t: String| {
            if let Ok(mut l) = d.lock() {
                l.stream.push_str(&t);
                l.changed = true;
            }
        });
        let (ws, we, cancel) = (week_start.to_string(), week_end.to_string(), key.clone());
        let rx = crate::ai::spawn(async move {
            let dates = days.iter().map(|d| d.date.clone()).collect();
            let r = amber_core::report::report_generate_weekly(
                ws,
                we,
                days,
                Some(display),
                model,
                cli,
                provider,
                None,
                Some(lang),
                Some(context),
                on_delta,
                Some(cancel),
            )
            .await?;
            Ok(Outcome::Weekly {
                dates,
                markdown: r.markdown,
                model: r.meta.model,
                duration_ms: r.meta.duration_ms,
            })
        });
        self.pump_report(wkey, seq, key, live, Vec::new(), rx, cx);
    }

    /// 100ms 마다 상자를 옮기고, 끝나면 파일과 행을 쓰고 마감한다
    #[allow(clippy::too_many_arguments)]
    fn pump_report(
        &mut self,
        run_key: String,
        seq: u64,
        cancel_key: String,
        live: LiveBox,
        mcp: Vec<(String, u8, String)>,
        mut rx: futures::channel::oneshot::Receiver<Result<Outcome, AiError>>,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
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
                    if let Ok(mut l) = live.lock()
                        && s.reports.runs.pump(&run_key, seq, &l, &cancel_key)
                    {
                        l.changed = false;
                        cx.notify();
                    }
                    let Some(done) = done else {
                        return true;
                    };
                    let end = match done {
                        Some(Ok(out)) => s.save_report(&run_key, out, &mcp),
                        Some(Err(e)) => End::Error(crate::ai::err_text(&e)),
                        None => return false,
                    };
                    if s.reports.runs.finish(&run_key, seq, end)
                        && s.section != crate::Section::Todo
                    {
                        s.reports.rail_done = true;
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

    /// 다 만들었다 — 본문 파일과 메타 행을 쓰고 행을 다시 읽는다
    fn save_report(&mut self, run_key: &str, out: Outcome, mcp: &[(String, u8, String)]) -> End {
        let provider = crate::note_ai::ai_args(&self.db).and_then(|a| a.2);
        let res = match out {
            Outcome::Empty => return End::Empty,
            Outcome::Daily {
                digests,
                markdown,
                model,
                duration_ms,
            } => report::write_report_file(&self.db, run_key, &markdown).and_then(|file_path| {
                let row = report::ReportRow {
                    date: run_key.to_string(),
                    file_path,
                    sources_json: sources_json(&digests, mcp),
                    provider,
                    model: Some(model),
                    duration_ms: duration_ms.map(|v| v as f64),
                };
                report::upsert_report(&self.db, &row).map_err(|e| e.to_string())?;
                let report = report::get_report(&self.db, run_key).map_err(|e| e.to_string())?;
                Ok((markdown, report))
            }),
            Outcome::Weekly {
                dates,
                markdown,
                model,
                duration_ms,
            } => {
                let week = run_key.trim_start_matches("w:");
                report::write_weekly_report_file(&self.db, week, &markdown).and_then(|file_path| {
                    let row = report::ReportRow {
                        date: week.to_string(),
                        file_path,
                        sources_json: Json::from(dates).to_string(),
                        provider,
                        model: Some(model),
                        duration_ms: duration_ms.map(|v| v as f64),
                    };
                    report::upsert_weekly_report(&self.db, &row).map_err(|e| e.to_string())?;
                    let report =
                        report::get_weekly_report(&self.db, week).map_err(|e| e.to_string())?;
                    Ok((markdown, report))
                })
            }
        };
        match res {
            Ok((body, report)) => End::Done { body, report },
            Err(e) => End::Error(e),
        }
    }

    /// 중단 — CLI 를 죽인다. 생성이 에러로 빠져 실패로 마감된다
    pub(crate) fn cancel_report(&mut self, run_key: &str) {
        if let Some(key) = self.reports.runs.cancel_key(run_key) {
            core_ai::ai_cancel(key);
        }
    }
}

#[cfg(test)]
mod tests {
    // super::* 는 gpui_kit 의 test 매크로까지 끌어와 #[test] 를 가린다
    use super::{Chip, ChipStatus, End, Live, Phase, Runs, day_range_ms, sources_json, weekly_key};
    use amber_core::report::{CollectProgress, SourceDigest};

    fn chip(id: &str, status: ChipStatus) -> Chip {
        Chip {
            id: id.into(),
            status,
            items: 0,
            error: None,
        }
    }

    #[test]
    fn one_run_per_key_and_late_results_are_dropped() {
        let mut r = Runs::default();
        let a = r.begin("2026-03-02", Phase::Collecting, vec![]).unwrap();
        assert!(r.begin("2026-03-02", Phase::Collecting, vec![]).is_none());
        assert!(r.any_running());
        assert_eq!(r.running_dates(), ["2026-03-02"]);
        // 주간은 레일만 센다
        let w = r
            .begin(&weekly_key("2026-03-02"), Phase::Streaming, vec![])
            .unwrap();
        assert_eq!(r.running_dates(), ["2026-03-02"]);
        assert!(!r.finish(&weekly_key("2026-03-02"), w, End::Empty));
        // 지운 뒤 늦게 온 결과는 버린다
        r.clear("2026-03-02");
        assert!(!r.finish("2026-03-02", a, End::Error("x".into())));
        assert!(r.get("2026-03-02").is_none());
        assert!(!r.any_running());
    }

    #[test]
    fn progress_fills_chips_and_streaming_brings_the_cancel_key() {
        let mut r = Runs::default();
        let seq = r
            .begin(
                "d",
                Phase::Collecting,
                vec![
                    chip("todos", ChipStatus::Ok),
                    chip("github", ChipStatus::Pending),
                    chip("ai_sessions", ChipStatus::Pending),
                    chip("slack", ChipStatus::Mcp),
                ],
            )
            .unwrap();
        let mut live = Live {
            progress: vec![
                CollectProgress {
                    id: "github".into(),
                    ok: false,
                    items: 0,
                    error: Some("gh auth login".into()),
                },
                CollectProgress {
                    id: "ai_sessions".into(),
                    ok: true,
                    items: 4,
                    error: None,
                },
            ],
            changed: true,
            ..Live::default()
        };
        assert!(r.pump("d", seq, &live, "k"));
        let run = r.get("d").unwrap();
        assert_eq!(run.chips[1].status, ChipStatus::Error);
        assert_eq!(run.chips[1].error.as_deref(), Some("gh auth login"));
        assert_eq!(
            (run.chips[2].status, run.chips[2].items),
            (ChipStatus::Ok, 4)
        );
        assert_eq!(run.chips[3].status, ChipStatus::Mcp);
        assert_eq!(
            (run.phase, run.cancel_key.as_deref()),
            (Phase::Collecting, None)
        );
        live.streaming = true;
        live.stream = "# 오".into();
        assert!(r.pump("d", seq, &live, "k"));
        let run = r.get("d").unwrap();
        assert_eq!(
            (run.phase, run.cancel_key.as_deref()),
            (Phase::Streaming, Some("k"))
        );
        assert_eq!(run.stream, "# 오");
        // 성공으로 넘어가는 순간만 레일 점
        assert!(r.finish(
            "d",
            seq,
            End::Done {
                body: "# 오늘".into(),
                report: None
            }
        ));
        let run = r.get("d").unwrap();
        assert_eq!((run.phase, run.cancel_key.as_deref()), (Phase::Done, None));
        // 끝난 뒤엔 다시 시작할 수 있다(이전 실행을 갈아 끼운다)
        assert!(r.begin("d", Phase::Collecting, vec![]).is_some());
        assert_eq!(r.get("d").unwrap().body, "");
    }

    #[test]
    fn sources_json_matches_the_web_order() {
        let d = SourceDigest {
            id: "github".into(),
            rank: 1,
            ok: false,
            items: 0,
            digest_md: "x".into(),
            error: Some("e".into()),
        };
        assert_eq!(
            sources_json(&[d], &[("slack".into(), 2, "slack".into())]),
            r#"[{"id":"github","rank":1,"ok":false,"items":0,"error":"e"},{"id":"slack","rank":2,"ok":true,"items":0,"mcp":true,"error":null}]"#
        );
    }

    #[test]
    fn day_range_is_local_midnight_to_midnight() {
        let (a, b) = day_range_ms("2026-03-02").unwrap();
        assert!(b - a == 86_400_000 || (b - a - 86_400_000).abs() == 3_600_000);
        assert!(day_range_ms("x").is_none());
    }
}
