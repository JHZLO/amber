// src/lib/db.ts 의 도메인 함수(개념, 태그, 설정, AI 호출 기록)를 옮긴 것. SQL 은 그대로다($n → ?n 만).
// 연결 자체(getDb)는 crate::db 가 맡는다.

use rusqlite::types::Value;
use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::db::{Db, Result, Row, int, text};

const TAGS_SUBQUERY: &str = "(SELECT group_concat(t.name, ',') FROM concept_tags ct \
     JOIN tags t ON t.id = ct.tag_id WHERE ct.concept_id = c.id) AS tags_csv";

/// tauri-plugin-sql 은 JS 숫자를 REAL 로 묶는다 — 정수 칸에서는 SQLite 가 정수로 되돌린다
fn real(n: f64) -> Value {
    Value::Real(n)
}

fn opt_text(s: Option<&str>) -> Value {
    s.map_or(Value::Null, text)
}

fn opt_real(n: Option<f64>) -> Value {
    n.map_or(Value::Null, real)
}

/// tags_csv 를 tags 배열로 (toWithTags)
fn with_tags(mut r: Row) -> Row {
    let tags: Vec<Json> = r
        .remove("tags_csv")
        .as_ref()
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(Json::from).collect())
        .unwrap_or_default();
    r.insert("tags".into(), Json::from(tags));
    r
}

#[derive(Deserialize, Default)]
pub struct ConceptFilter {
    pub status: String,
    pub search: Option<String>,
    pub tags: Option<Vec<String>>,
    pub sort: Option<String>,
    pub limit: Option<f64>,
}

/// 관리 창 리스트: 상태/검색/태그 필터 + 정렬
pub fn list_concepts(db: &Db, filter: &ConceptFilter) -> Result<Vec<Row>> {
    let mut wheres = Vec::new();
    let mut params = Vec::new();

    if filter.status != "all" {
        params.push(text(&filter.status));
        wheres.push(format!("c.status = ?{}", params.len()));
    }

    let search = filter.search.as_deref().map(str::trim).unwrap_or_default();
    if !search.is_empty() {
        let like = format!("%{search}%");
        let p = params.len() + 1;
        wheres.push(format!(
            "(c.title LIKE ?{p} OR c.summary LIKE ?{} OR EXISTS(\
             SELECT 1 FROM concept_tags ct JOIN tags t ON t.id = ct.tag_id \
             WHERE ct.concept_id = c.id AND t.name LIKE ?{}))",
            p + 1,
            p + 2
        ));
        params.extend([text(&like), text(&like), text(&like)]);
    }

    let tags: Vec<&String> = filter
        .tags
        .iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .collect();
    if !tags.is_empty() {
        let first = params.len() + 1;
        let holders: Vec<String> = (first..first + tags.len())
            .map(|i| format!("?{i}"))
            .collect();
        params.extend(tags.iter().map(|t| text(t)));
        params.push(real(tags.len() as f64));
        wheres.push(format!(
            "c.id IN (SELECT ct.concept_id FROM concept_tags ct \
             JOIN tags t ON t.id = ct.tag_id WHERE t.name IN ({}) \
             GROUP BY ct.concept_id HAVING COUNT(DISTINCT t.name) = ?{})",
            holders.join(","),
            params.len()
        ));
    }

    let order_by = match filter.sort.as_deref().unwrap_or("recent_updated") {
        "canonical" => "c.confidence ASC, c.last_seen_at ASC, c.id ASC",
        "recent_updated" => "c.updated_at DESC, c.id DESC",
        "recent_created" => "c.created_at DESC, c.id DESC",
        "title" => "c.title COLLATE NOCASE ASC",
        // TS 는 모르는 값이면 `ORDER BY undefined` 가 되어 SQL 오류로 끝난다
        _ => "undefined",
    };

    let mut sql = format!(
        "SELECT c.id, c.ulid, c.title, c.summary, c.detail_path, c.status, c.confidence,\
         c.seen_count, c.last_seen_at, c.learned_at, c.created_at, c.updated_at, c.source_kind,\
         CASE WHEN c.source_kind = 'file' THEN c.source END AS source, {TAGS_SUBQUERY} FROM concepts c{} ORDER BY {order_by}",
        if wheres.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", wheres.join(" AND "))
        }
    );
    if let Some(limit) = filter.limit {
        params.push(real(limit));
        sql.push_str(&format!(" LIMIT ?{}", params.len()));
    }
    Ok(db
        .select(&sql, &params)?
        .into_iter()
        .map(with_tags)
        .collect())
}

/// 위젯/학습중 큐: canonical 정렬된 학습중 개념
pub fn learning_queue(db: &Db) -> Result<Vec<Row>> {
    list_concepts(
        db,
        &ConceptFilter {
            status: "learning".into(),
            sort: Some("canonical".into()),
            ..Default::default()
        },
    )
}

pub fn get_concept(db: &Db, id: i64) -> Result<Option<Row>> {
    let rows = db.select(
        &format!("SELECT c.*, {TAGS_SUBQUERY} FROM concepts c WHERE c.id = ?1"),
        &[int(id)],
    )?;
    Ok(rows.into_iter().next().map(with_tags))
}

/// [start_ms, end_ms) 에 학습완료된 개념 (할 일 탭의 "이날 학습완료" 칩)
pub fn concepts_learned_on(db: &Db, start_ms: f64, end_ms: f64) -> Result<Vec<Row>> {
    db.select(
        "SELECT id, title FROM concepts
     WHERE status = 'learned' AND learned_at >= ?1 AND learned_at < ?2
     ORDER BY learned_at DESC",
        &[real(start_ms), real(end_ms)],
    )
}

/// 노트가 이름변경/이동됐을 때 개념의 출처 경로(source JSON 의 noteRel)를 따라 옮긴다
pub fn repoint_concept_source(db: &Db, old_rel: &str, new_rel: &str, is_dir: bool) -> Result<()> {
    let sql = if is_dir {
        "UPDATE concepts
          SET source = json_set(source, '$.noteRel',
                ?2 || substr(json_extract(source, '$.noteRel'), length(?1) + 1))
        WHERE source_kind = 'file'
          AND json_valid(source)
          AND json_extract(source, '$.noteRel') LIKE ?1 || '/%'"
    } else {
        "UPDATE concepts
        SET source = json_set(source, '$.noteRel', ?2)
      WHERE source_kind = 'file'
        AND json_valid(source)
        AND json_extract(source, '$.noteRel') = ?1"
    };
    db.execute(sql, &[text(old_rel), text(new_rel)])?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConceptInput {
    pub ulid: String,
    pub title: String,
    pub summary: String,
    pub detail_path: String,
    pub tags: Option<Vec<String>>,
    pub confidence: Option<f64>,
    pub source: Option<String>,
    pub source_kind: Option<String>,
}

/// 개념 생성 (status=learning 기본). 생성된 id
pub fn create_concept(db: &Db, input: &CreateConceptInput) -> Result<i64> {
    let (_, id) = db.execute(
        "INSERT INTO concepts (ulid, title, summary, detail_path, source, source_kind, confidence)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        &[
            text(&input.ulid),
            text(&input.title),
            text(&input.summary),
            text(&input.detail_path),
            opt_text(input.source.as_deref()),
            text(input.source_kind.as_deref().unwrap_or("paste")),
            real(input.confidence.unwrap_or(1.0)),
        ],
    )?;
    if let Some(tags) = input.tags.as_ref().filter(|t| !t.is_empty()) {
        set_concept_tags(db, id, tags)?;
    }
    Ok(id)
}

/// 제목/요약 수정 (updated_at 갱신). 둘 다 없으면 아무것도 안 한다
pub fn update_concept_content(
    db: &Db,
    id: i64,
    title: Option<&str>,
    summary: Option<&str>,
) -> Result<()> {
    let mut sets = Vec::new();
    let mut params = Vec::new();
    for (col, v) in [("title", title), ("summary", summary)] {
        if let Some(v) = v {
            params.push(text(v));
            sets.push(format!("{col} = ?{}", params.len()));
        }
    }
    if sets.is_empty() {
        return Ok(());
    }
    params.push(real(db.now() as f64));
    sets.push(format!("updated_at = ?{}", params.len()));
    params.push(int(id));
    db.execute(
        &format!(
            "UPDATE concepts SET {} WHERE id = ?{}",
            sets.join(", "),
            params.len()
        ),
        &params,
    )?;
    Ok(())
}

/// 상태 전이. 이미 그 상태면 그대로(멱등). learned_at 은 트리거가 관리
pub fn set_status(db: &Db, id: i64, status: &str) -> Result<()> {
    db.execute(
        "UPDATE concepts SET status = ?1, updated_at = ?2 WHERE id = ?3 AND status <> ?1",
        &[text(status), real(db.now() as f64), int(id)],
    )?;
    Ok(())
}

/// 자신감 ±1 (1~3 으로 자른다). updated_at 은 건드리지 않는다. 새 값
pub fn adjust_confidence(db: &Db, id: i64, delta: f64) -> Result<Json> {
    db.execute(
        "UPDATE concepts SET confidence = MIN(MAX(confidence + ?1, 1), 3) WHERE id = ?2",
        &[real(delta), int(id)],
    )?;
    let rows = db.select("SELECT confidence FROM concepts WHERE id = ?1", &[int(id)])?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|mut r| r.remove("confidence"))
        .filter(|v| !v.is_null())
        .unwrap_or(json!(1)))
}

/// 위젯 노출 기록. updated_at 은 건드리지 않는다
pub fn mark_seen(db: &Db, id: i64) -> Result<()> {
    db.execute(
        "UPDATE concepts SET seen_count = seen_count + 1, last_seen_at = ?1 WHERE id = ?2",
        &[real(db.now() as f64), int(id)],
    )?;
    Ok(())
}

/// concept_tags 는 FK CASCADE. vault 폴더 삭제는 호출부가 맡는다
pub fn delete_concept(db: &Db, id: i64) -> Result<()> {
    db.execute("DELETE FROM concepts WHERE id = ?1", &[int(id)])?;
    Ok(())
}

fn upsert_tag(db: &Db, name: &str) -> Result<i64> {
    db.execute(
        "INSERT INTO tags (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        &[text(name)],
    )?;
    let rows = db.select("SELECT id FROM tags WHERE name = ?1", &[text(name)])?;
    rows.first()
        .and_then(|r| r.get("id"))
        .and_then(Json::as_i64)
        .ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// 개념의 태그를 주어진 목록으로 교체 (앞뒤 공백을 떼고, 빈 것과 겹치는 것은 뺀다)
pub fn set_concept_tags(db: &Db, concept_id: i64, names: &[String]) -> Result<()> {
    db.execute(
        "DELETE FROM concept_tags WHERE concept_id = ?1",
        &[int(concept_id)],
    )?;
    let mut seen = Vec::new();
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if seen.contains(&name) {
            continue;
        }
        seen.push(name);
        let tag_id = upsert_tag(db, name)?;
        db.execute(
            "INSERT OR IGNORE INTO concept_tags (concept_id, tag_id) VALUES (?1, ?2)",
            &[int(concept_id), int(tag_id)],
        )?;
    }
    Ok(())
}

/// 전체 태그 (사용 횟수 desc)
pub fn all_tags(db: &Db) -> Result<Vec<Row>> {
    db.select(
        "SELECT t.name AS name, COUNT(ct.concept_id) AS count FROM tags t
     LEFT JOIN concept_tags ct ON ct.tag_id = t.id
     GROUP BY t.id ORDER BY count DESC, t.name COLLATE NOCASE ASC",
        &[],
    )
}

/// 상태별 개수 (탭 배지)
pub fn status_counts(db: &Db) -> Result<Json> {
    let rows = db.select(
        "SELECT status, COUNT(*) AS c FROM concepts GROUP BY status",
        &[],
    )?;
    let count = |status: &str| {
        rows.iter()
            .find(|r| r.get("status").and_then(Json::as_str) == Some(status))
            .and_then(|r| r.get("c").and_then(Json::as_i64))
            .unwrap_or(0)
    };
    let (learning, learned) = (count("learning"), count("learned"));
    Ok(json!({ "learning": learning, "learned": learned, "all": learning + learned }))
}

pub fn get_setting(db: &Db, key: &str) -> Result<Option<String>> {
    let rows = db.select("SELECT value FROM settings WHERE key = ?1", &[text(key)])?;
    Ok(rows
        .first()
        .and_then(|r| r.get("value"))
        .and_then(Json::as_str)
        .map(str::to_string))
}

pub fn set_setting(db: &Db, key: &str, value: &str) -> Result<()> {
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        &[text(key), text(value)],
    )?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvocationLog {
    pub concept_id: Option<f64>,
    pub model: String,
    pub session_id: Option<String>,
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<f64>,
    pub status: String,
    pub retry_count: Option<f64>,
}

pub fn log_invocation(db: &Db, log: &InvocationLog) -> Result<()> {
    db.execute(
        "INSERT INTO ai_invocations
       (concept_id, model, session_id, input_tokens, output_tokens, cost_usd, duration_ms, status, retry_count)
     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        &[
            opt_real(log.concept_id),
            text(&log.model),
            opt_text(log.session_id.as_deref()),
            opt_real(log.input_tokens),
            opt_real(log.output_tokens),
            opt_real(log.cost_usd),
            opt_real(log.duration_ms),
            text(&log.status),
            real(log.retry_count.unwrap_or(0.0)),
        ],
    )?;
    Ok(())
}
