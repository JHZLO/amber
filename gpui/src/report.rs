// src/lib/report.ts 의 DB 함수(리포트 설정, MCP 캐시, 일간/주간 리포트 행, 할 일 digest)를 옮긴 것.
// SQL 은 그대로다($n → ?n 만). 본문 파일(vault/reports)도 여기, AI 실행은 없다.
// 설정값은 JS 의 JSON.stringify 와 글자까지 같아야 해서 serde_json 의 preserve_order 를 쓴다(Cargo.toml).

use std::collections::HashSet;

use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::types::Value;
use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::concepts::{get_setting, set_setting};
use crate::db::{Db, Result, Row, get_i64, text};
use crate::todos::{list_overdue_open, list_todos};

const MCP_CACHE_KEY: &str = "report_mcp_cache";

fn opt_text(s: Option<&str>) -> Value {
    s.map_or(Value::Null, text)
}

fn is_mcp_server(v: &Json) -> bool {
    v["name"].is_string()
        && v["connected"].is_boolean()
        && v["status"].is_string()
        && v["transport"].is_string()
}

/// 마지막으로 감지한 MCP 서버 목록. 없거나 깨졌으면 null
pub fn load_mcp_cache(db: &Db) -> Result<Json> {
    let raw = get_setting(db, MCP_CACHE_KEY)?.unwrap_or_default();
    let clean: Vec<Json> = match serde_json::from_str::<Json>(&raw) {
        Ok(Json::Array(arr)) => arr.into_iter().filter(is_mcp_server).collect(),
        _ => Vec::new(),
    };
    Ok(if clean.is_empty() {
        Json::Null
    } else {
        Json::from(clean)
    })
}

/// 빈 결과(감지 실패, 타임아웃)로 멀쩡한 캐시를 덮지 않는다
pub fn save_mcp_cache(db: &Db, servers: &[Json]) -> Result<()> {
    if servers.is_empty() {
        return Ok(());
    }
    set_setting(db, MCP_CACHE_KEY, &Json::from(servers).to_string())
}

/// 앞뒤 공백을 떼면 비지 않은 글자만, 처음 나온 순서로 하나씩 (JS 의 `new Set(...)`)
fn unique_nonblank<'a>(items: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .filter(|s| !s.trim().is_empty() && seen.insert(*s))
        .map(str::to_string)
        .collect()
}

/// 켜 둔 MCP 서버 목록. 키가 없거나 깨졌으면 예전 두 칸(slack, notion)에서 옮겨 온다
pub fn parse_mcp_picked(
    raw: Option<&str>,
    slack: Option<&str>,
    notion: Option<&str>,
) -> Vec<String> {
    if let Some(Ok(Json::Array(arr))) = raw
        .filter(|r| !r.is_empty())
        .map(serde_json::from_str::<Json>)
    {
        return unique_nonblank(arr.iter().filter_map(Json::as_str));
    }
    unique_nonblank([slack.unwrap_or(""), notion.unwrap_or("")].into_iter())
}

fn default_sources() -> Vec<Json> {
    vec![
        json!({ "id": "github", "enabled": true }),
        json!({ "id": "ai_sessions", "enabled": true }),
    ]
}

/// 예전 설정의 "slack", "notion" 항목은 걸러지고, 빠진 기본 소스는 뒤에 보충된다
fn parse_sources(raw: Option<&str>) -> Vec<Json> {
    let Some(Ok(Json::Array(arr))) = raw
        .filter(|r| !r.is_empty())
        .map(serde_json::from_str::<Json>)
    else {
        return default_sources();
    };
    let mut clean: Vec<Json> = arr
        .into_iter()
        .filter(|s| {
            matches!(s["id"].as_str(), Some("github" | "ai_sessions")) && s["enabled"].is_boolean()
        })
        .collect();
    for d in default_sources() {
        if !clean.iter().any(|s| s["id"] == d["id"]) {
            clean.push(d);
        }
    }
    clean
}

pub fn load_report_config(db: &Db) -> Result<Json> {
    let get = |k: &str| get_setting(db, k);
    let sources = get("report_sources")?;
    let repos = get("report_github_repos")?.unwrap_or_default();
    let mcp = get("report_mcp_servers")?;
    let (slack, notion) = (get("report_slack_server")?, get("report_notion_server")?);
    Ok(json!({
        "onboarded": get("report_onboarded")?.as_deref() == Some("1"),
        "sources": parse_sources(sources.as_deref()),
        "githubPath": get("report_github_path")?.unwrap_or_default(),
        "githubRepos": repos.split(',').map(str::trim).filter(|r| !r.is_empty()).collect::<Vec<_>>(),
        "githubAccount": get("report_github_account")?.unwrap_or_default(),
        // 기본 on. 명시적으로 "0" 을 저장했을 때만 off
        "sessionsClaude": get("report_sessions_claude")?.as_deref() != Some("0"),
        "sessionsCodex": get("report_sessions_codex")?.as_deref() != Some("0"),
        "mcpPicked": parse_mcp_picked(mcp.as_deref(), slack.as_deref(), notion.as_deref()),
        "displayName": get("report_display_name")?.unwrap_or_default(),
        "context": get("report_context")?.unwrap_or_default(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportConfig {
    pub onboarded: bool,
    pub sources: Json,
    pub github_path: String,
    pub github_repos: Vec<String>,
    pub github_account: String,
    pub sessions_claude: bool,
    pub sessions_codex: bool,
    pub mcp_picked: Json,
    pub display_name: String,
    pub context: String,
}

pub fn save_report_config(db: &Db, c: &ReportConfig) -> Result<()> {
    let flag = |b: bool| if b { "1" } else { "0" };
    for (key, value) in [
        ("report_onboarded", flag(c.onboarded).to_string()),
        ("report_sources", c.sources.to_string()),
        ("report_github_path", c.github_path.trim().to_string()),
        ("report_github_repos", c.github_repos.join(",")),
        ("report_github_account", c.github_account.trim().to_string()),
        (
            "report_sessions_claude",
            flag(c.sessions_claude).to_string(),
        ),
        ("report_sessions_codex", flag(c.sessions_codex).to_string()),
        ("report_mcp_servers", c.mcp_picked.to_string()),
        ("report_display_name", c.display_name.trim().to_string()),
        ("report_context", c.context.trim().to_string()),
    ] {
        set_setting(db, key, &value)?;
    }
    Ok(())
}

pub fn get_report(db: &Db, date: &str) -> Result<Option<Row>> {
    let rows = db.select(
        "SELECT * FROM daily_reports WHERE report_date = ?1",
        &[text(date)],
    )?;
    Ok(rows.into_iter().next())
}

/// upsertReport, upsertWeeklyReport 의 입력. 키는 일간이 date, 주간이 weekStart
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRow {
    #[serde(alias = "weekStart")]
    pub date: String,
    pub file_path: String,
    pub sources_json: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub duration_ms: Option<f64>,
}

fn upsert(db: &Db, table: &str, key: &str, input: &ReportRow) -> Result<()> {
    db.execute(
        &format!(
            "INSERT INTO {table} ({key}, file_path, sources_json, provider, model, duration_ms, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
     ON CONFLICT({key}) DO UPDATE SET
       file_path = excluded.file_path,
       sources_json = excluded.sources_json,
       provider = excluded.provider,
       model = excluded.model,
       duration_ms = excluded.duration_ms,
       updated_at = excluded.updated_at"
        ),
        &[
            text(&input.date),
            text(&input.file_path),
            text(&input.sources_json),
            opt_text(input.provider.as_deref()),
            opt_text(input.model.as_deref()),
            input.duration_ms.map_or(Value::Null, Value::Real),
            // tauri-plugin-sql 은 JS 숫자를 REAL 로 묶는다(정수 칸이라 SQLite 가 정수로 되돌린다)
            Value::Real(db.now() as f64),
        ],
    )?;
    Ok(())
}

pub fn upsert_report(db: &Db, input: &ReportRow) -> Result<()> {
    upsert(db, "daily_reports", "report_date", input)
}

/// 최근 리포트 날짜 몇 개 (최신순)
pub fn recent_report_dates(db: &Db, before: &str, limit: f64) -> Result<Vec<String>> {
    let rows = db.select(
        "SELECT report_date FROM daily_reports
      WHERE report_date < ?1
      ORDER BY report_date DESC
      LIMIT ?2",
        &[text(before), Value::Real(limit)],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| r.get("report_date")?.as_str().map(str::to_string))
        .collect())
}

pub fn get_weekly_report(db: &Db, week_start: &str) -> Result<Option<Row>> {
    let rows = db.select(
        "SELECT * FROM weekly_reports WHERE week_start = ?1",
        &[text(week_start)],
    )?;
    Ok(rows.into_iter().next())
}

pub fn upsert_weekly_report(db: &Db, input: &ReportRow) -> Result<()> {
    upsert(db, "weekly_reports", "week_start", input)
}

// ---- vault/reports 파일 (본문 정본, frontmatter 없음) ----

const VAULT: &str = "vault";

/// DB 에 저장할 상대경로
pub fn report_path_for(date: &str) -> String {
    format!("reports/{date}.md")
}

/// 일간과 같은 reports/ 폴더 — '-week' 접미어로 구분한다
pub fn weekly_report_path_for(week_start: &str) -> String {
    format!("reports/{week_start}-week.md")
}

fn write_body(db: &Db, rel: String, md: &str) -> std::result::Result<String, String> {
    std::fs::create_dir_all(db.at(&format!("{VAULT}/reports"))).map_err(|e| e.to_string())?;
    crate::vault::write_atomic(db, &format!("{VAULT}/{rel}"), md)?;
    Ok(rel)
}

fn read_body(db: &Db, rel: &str) -> std::result::Result<Option<String>, String> {
    let p = db.at(&format!("{VAULT}/{rel}"));
    if !p.exists() {
        return Ok(None);
    }
    crate::vault::read_text(&p).map(Some)
}

pub fn write_report_file(db: &Db, date: &str, md: &str) -> std::result::Result<String, String> {
    write_body(db, report_path_for(date), md)
}

pub fn read_report_file(db: &Db, date: &str) -> std::result::Result<Option<String>, String> {
    read_body(db, &report_path_for(date))
}

/// 행을 지우고 본문 파일은 휴지통으로(없으면 그대로)
pub fn delete_report(db: &Db, date: &str) -> std::result::Result<(), String> {
    db.execute(
        "DELETE FROM daily_reports WHERE report_date = ?1",
        &[text(date)],
    )
    .map_err(|e| e.to_string())?;
    db.trash(&format!("{VAULT}/{}", report_path_for(date)))
}

pub fn write_weekly_report_file(
    db: &Db,
    week_start: &str,
    md: &str,
) -> std::result::Result<String, String> {
    write_body(db, weekly_report_path_for(week_start), md)
}

/// 주간 본문. 행에 적힌 경로가 있으면 그걸로 — 주 시작 요일이 바뀌어 키가 옮겨진 행(0013)은 파일명이 옛 키다
pub fn read_weekly_report_file(
    db: &Db,
    week_start: &str,
    file_path: Option<&str>,
) -> std::result::Result<Option<String>, String> {
    match file_path.filter(|p| !p.is_empty()) {
        Some(p) => read_body(db, p),
        None => read_body(db, &weekly_report_path_for(week_start)),
    }
}

/// 행에 적힌 경로를 먼저 읽고 지운다(파일명이 키와 다를 수 있다)
pub fn delete_weekly_report(db: &Db, week_start: &str) -> std::result::Result<(), String> {
    let row = get_weekly_report(db, week_start).map_err(|e| e.to_string())?;
    db.execute(
        "DELETE FROM weekly_reports WHERE week_start = ?1",
        &[text(week_start)],
    )
    .map_err(|e| e.to_string())?;
    let rel = row
        .as_ref()
        .and_then(|r| r.get("file_path")?.as_str())
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| weekly_report_path_for(week_start));
    db.trash(&format!("{VAULT}/{rel}"))
}

/// 주간 재료 한 날 — 일간 리포트가 없는 날은 body 가 None 이다
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WeekDaySource {
    pub date: String,
    pub weekday: String,
    pub body: Option<String>,
}

/// 그 주 7일의 일간 리포트 본문. 못 읽은 날도 None 으로 남긴다 — 패널이 "N일 빠짐"을 말할 수 있게
pub fn load_week_sources(db: &Db, week_start: &str) -> Vec<WeekDaySource> {
    crate::dates::week_days(week_start)
        .into_iter()
        .map(|date| WeekDaySource {
            weekday: crate::dates::weekday_short(&date),
            body: read_report_file(db, &date).ok().flatten(),
            date,
        })
        .collect()
}

/// 선택 날짜의 할 일 + (오늘이면) 밀린 항목을 마크다운으로. 생성 프롬프트의 '계획' 재료.
/// 미체크 이월 고스트는 다른 날로 넘긴 것이라 빼고, 체크된 고스트는 그날 끝낸 일이라 남긴다. 지워진 흔적(gone)은 뺀다
pub fn build_todos_digest(db: &Db, date: &str) -> Result<Json> {
    let todos: Vec<Row> = list_todos(db, date)?
        .into_iter()
        .filter(|t| {
            (get_i64(t, "carried") != Some(1) || get_i64(t, "done") == Some(1))
                && get_i64(t, "gone") != Some(1)
        })
        .collect();
    let today = Local
        .timestamp_millis_opt(db.now())
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string());
    let overdue = if today.as_deref() == Some(date) {
        list_overdue_open(db, date)?
    } else {
        Vec::new()
    };
    if todos.is_empty() && overdue.is_empty() {
        return Ok(json!({ "md": "", "count": 0 }));
    }
    let lines = digest_lines(&todos, &overdue[..overdue.len().min(20)]);
    Ok(json!({ "md": lines.join("\n"), "count": todos.len() }))
}

/// reportTodos.ts 의 todoDigestLines. 자식 줄마다 상위 경로를 붙인다 — 들여쓰기만으로는 AI 가 엉뚱한 부모에 붙였다
fn digest_lines(todos: &[Row], overdue: &[Row]) -> Vec<String> {
    let id = |t: &Row| get_i64(t, "id").unwrap_or_default();
    let parent = |t: &Row| get_i64(t, "parent_id");
    let content = |t: &Row| {
        t.get("content")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let present: HashSet<i64> = todos.iter().map(id).collect();

    fn walk(
        t: &Row,
        depth: usize,
        trail: &[String],
        todos: &[Row],
        seen: &mut HashSet<i64>,
        lines: &mut Vec<String>,
    ) {
        let tid = get_i64(t, "id").unwrap_or_default();
        if !seen.insert(tid) {
            return;
        }
        let text = t.get("content").and_then(Json::as_str).unwrap_or_default();
        let path = if trail.is_empty() {
            String::new()
        } else {
            let clipped: Vec<String> = trail.iter().map(|s| clip(s)).collect();
            format!("  (상위: {})", clipped.join(" › "))
        };
        let mark = if get_i64(t, "done") == Some(1) {
            "[x]"
        } else {
            "[ ]"
        };
        lines.push(format!("{}- {mark} {text}{path}", "  ".repeat(depth)));
        let mut next = trail.to_vec();
        next.push(text.to_string());
        for c in todos
            .iter()
            .filter(|c| get_i64(c, "parent_id") == Some(tid))
        {
            walk(c, depth + 1, &next, todos, seen, lines);
        }
    }

    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    // visibleRoots: 부모가 이 목록에 없으면 루트로 그린다
    for t in todos
        .iter()
        .filter(|t| parent(t).is_none_or(|p| !present.contains(&p)))
    {
        walk(t, 0, &[], todos, &mut seen, &mut lines);
    }
    if !overdue.is_empty() {
        lines.push(String::new());
        lines.push("밀린(미완료) 항목:".into());
        for t in overdue.iter().take(20) {
            let when = t
                .get("due_date")
                .and_then(Json::as_str)
                .filter(|d| !d.is_empty())
                .map(|d| format!(" (원래 {})", day_short(d)))
                .unwrap_or_default();
            lines.push(format!("- [ ] {}{when}", content(t)));
        }
    }
    lines
}

/// 조상 제목은 50자(JS 문자열 길이 = UTF-16 단위)를 넘으면 자른다
fn clip(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() > 50 {
        format!("{}…", String::from_utf16_lossy(&units[..49]))
    } else {
        s.to_string()
    }
}

/// "7/14" (formatDayShort)
fn day_short(d: &str) -> String {
    NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .map(|d| d.format("%-m/%-d").to_string())
        .unwrap_or_else(|_| "NaN/NaN".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_picked_falls_back_to_legacy_slots() {
        assert_eq!(
            parse_mcp_picked(Some(r#"["a"," ","a","b"]"#), None, None),
            ["a", "b"]
        );
        assert_eq!(
            parse_mcp_picked(Some("{"), Some("slack"), Some(" ")),
            ["slack"]
        );
        assert_eq!(parse_mcp_picked(None, Some("s"), Some("s")), ["s"]);
    }

    #[test]
    fn clip_counts_utf16_units() {
        assert_eq!(clip(&"가".repeat(50)), "가".repeat(50));
        assert_eq!(clip(&"가".repeat(51)), format!("{}…", "가".repeat(49)));
        assert_eq!(day_short("2026-07-04"), "7/4");
    }
}
