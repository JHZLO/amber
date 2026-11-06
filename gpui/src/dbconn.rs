// src/lib/dbconn.ts 의 DB 함수(연결 프로필)를 옮긴 것. SQL 은 그대로다($n → ?n 만).
// 키체인, 외부 DB 접속, 스냅샷 파일은 여기 없다.

use rusqlite::types::Value;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use crate::db::{Db, Result, Row, int, text};

const COLS: &str = "id, ulid, name, kind, env, host, port, username, tls, credential_command, folder_path, schemas_json, \
     last_sync_at, last_error, created_at, updated_at";

/// JS 의 String(v)
fn js_string(v: &Json) -> String {
    match v {
        Json::Null => "null".into(),
        Json::String(s) => s.clone(),
        Json::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    js_string(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Json::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

/// 스키마 설정 JSON 을 고르게 읽는다. 깨졌으면 빈 목록, 이름이 문자열이 아닌 항목은 뺀다
fn parse_schemas(raw: &str) -> Vec<Json> {
    let Ok(Json::Array(arr)) = serde_json::from_str::<Json>(raw) else {
        return Vec::new();
    };
    arr.iter()
        .filter(|s| s["name"].is_string())
        .map(|s| {
            let label = match &s["label"] {
                Json::Null => String::new(),
                v => js_string(v),
            };
            let mut p = json!({ "name": s["name"], "label": label, "enabled": s["enabled"] != json!(false) });
            if s["audit"] == json!(false) {
                p["audit"] = json!(false);
            }
            if s["starred"] == json!(true) {
                p["starred"] = json!(true);
            }
            p
        })
        .collect()
}

fn from_row(mut r: Row) -> Json {
    let raw = r.shift_remove("schemas_json");
    let schemas = parse_schemas(raw.as_ref().and_then(Json::as_str).unwrap_or_default());
    r.insert("schemas".into(), Json::from(schemas));
    Json::Object(r)
}

pub fn list_connections(db: &Db) -> Result<Vec<Json>> {
    let rows = db.select(
        &format!("SELECT {COLS} FROM db_connections ORDER BY name COLLATE NOCASE, id"),
        &[],
    )?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub fn get_connection(db: &Db, id: i64) -> Result<Option<Json>> {
    let rows = db.select(
        &format!("SELECT {COLS} FROM db_connections WHERE id = ?1"),
        &[int(id)],
    )?;
    Ok(rows.into_iter().next().map(from_row))
}

#[derive(Deserialize)]
pub struct ConnectionInput {
    pub name: String,
    pub kind: String,
    pub env: String,
    pub host: String,
    pub port: f64,
    pub username: String,
    pub tls: String,
    pub credential_command: String,
    pub folder_path: String,
    pub schemas: Json,
}

/// 프로필 저장. 비밀번호는 여기 오지 않는다 — 호출부가 키체인에 따로 넣는다
pub fn create_connection(db: &Db, input: &ConnectionInput) -> Result<Json> {
    let (_, id) = db.execute(
        "INSERT INTO db_connections (ulid, name, kind, env, host, port, username, tls, credential_command, folder_path, schemas_json)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        &[
            text(&db.new_ulid()),
            text(input.name.trim()),
            text(&input.kind),
            text(&input.env),
            text(input.host.trim()),
            Value::Real(input.port),
            text(input.username.trim()),
            text(&input.tls),
            text(input.credential_command.trim()),
            text(&input.folder_path),
            text(&input.schemas.to_string()),
        ],
    )?;
    get_connection(db, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

/// updateConnection 의 Partial<ConnectionInput>
#[derive(Deserialize, Default)]
pub struct ConnectionPatch {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub env: Option<String>,
    pub host: Option<String>,
    pub port: Option<f64>,
    pub username: Option<String>,
    pub tls: Option<String>,
    pub credential_command: Option<String>,
    pub folder_path: Option<String>,
    pub schemas: Option<Json>,
}

pub fn update_connection(db: &Db, id: i64, patch: &ConnectionPatch) -> Result<()> {
    let trimmed = |s: &Option<String>| s.as_deref().map(|s| text(s.trim()));
    let plain = |s: &Option<String>| s.as_deref().map(text);
    let mut sets = Vec::new();
    let mut args = Vec::new();
    for (col, v) in [
        ("name", trimmed(&patch.name)),
        ("kind", plain(&patch.kind)),
        ("env", plain(&patch.env)),
        ("host", trimmed(&patch.host)),
        ("port", patch.port.map(Value::Real)),
        ("username", trimmed(&patch.username)),
        ("tls", plain(&patch.tls)),
        ("credential_command", trimmed(&patch.credential_command)),
        ("folder_path", plain(&patch.folder_path)),
        (
            "schemas_json",
            patch.schemas.as_ref().map(|s| text(&s.to_string())),
        ),
    ] {
        if let Some(v) = v {
            args.push(v);
            sets.push(format!("{col} = ?{}", args.len()));
        }
    }
    if sets.is_empty() {
        return Ok(());
    }
    args.push(Value::Real(db.now() as f64));
    sets.push(format!("updated_at = ?{}", args.len()));
    args.push(int(id));
    db.execute(
        &format!(
            "UPDATE db_connections SET {} WHERE id = ?{}",
            sets.join(", "),
            args.len()
        ),
        &args,
    )?;
    Ok(())
}

/// 동기화 결과 기록. 성공이면 시각 갱신 + 에러 지움, 실패면 코드만 남긴다(문장은 화면이 만든다)
/// deleteConnection 의 행 지우기 — 키체인과 스냅샷 폴더는 부르는 쪽이 같이 치운다
pub fn delete_connection_row(db: &Db, id: i64) -> Result<()> {
    db.execute("DELETE FROM db_connections WHERE id = ?1", &[int(id)])
        .map(|_| ())
}

pub fn mark_sync(db: &Db, id: i64, error_code: Option<&str>) -> Result<()> {
    let now = Value::Real(db.now() as f64);
    match error_code.filter(|c| !c.is_empty()) {
        Some(code) => db.execute(
            "UPDATE db_connections SET last_error = ?1, updated_at = ?2 WHERE id = ?3",
            &[text(code), now, int(id)],
        ),
        None => db.execute(
            "UPDATE db_connections SET last_sync_at = ?1, last_error = NULL, updated_at = ?1 WHERE id = ?2",
            &[now, int(id)],
        ),
    }?;
    Ok(())
}

/// 스키마 하나의 별표를 켜고 끈다. 끄면 키를 지운다(없음 = 꺼짐)
pub fn set_schema_starred(
    db: &Db,
    conn: &Map<String, Json>,
    schema: &str,
    starred: bool,
) -> Result<()> {
    let schemas: Vec<Json> = conn
        .get("schemas")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(|p| match p.as_object() {
            Some(obj) if p["name"] == schema => {
                let mut rest = obj.clone();
                rest.shift_remove("starred");
                if starred {
                    rest.insert("starred".into(), json!(true));
                }
                Json::Object(rest)
            }
            _ => p.clone(),
        })
        .collect();
    let id = conn.get("id").and_then(Json::as_i64).unwrap_or_default();
    update_connection(
        db,
        id,
        &ConnectionPatch {
            schemas: Some(Json::from(schemas)),
            ..Default::default()
        },
    )
}

// ---- 순수 판정(lib/dbconn.ts) — 설정 목록과 ERD 트리가 같은 판정, 같은 색을 쓴다 ----

/// 연동 대상 스키마 하나 — 물리 이름 + 표시명. 체크를 풀면 enabled=false 로 남는다
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct DbSchemaPref {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub enabled: bool,
    /// ERD 에 감사 표를 그리는가 — 없으면 true
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
}

/// db_connections 한 행(list_connections 의 JSON)
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct DbConnection {
    pub id: i64,
    pub ulid: String,
    pub name: String,
    pub kind: String,
    pub env: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub tls: String,
    #[serde(default)]
    pub credential_command: String,
    /// ERD 저장소 기준 연결 폴더 — 스키마 폴더는 `{folder_path}/{schema}`
    pub folder_path: String,
    pub schemas: Vec<DbSchemaPref>,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

pub fn pref_audit(p: &DbSchemaPref) -> bool {
    p.audit != Some(false)
}

pub fn schema_folder(c: &DbConnection, schema: &str) -> String {
    format!("{}/{schema}", c.folder_path)
}

/// 스키마 폴더 안의 스냅샷 — 트리는 .mmd 만 보이니 여기 뜨지 않는다
pub fn snapshot_path(c: &DbConnection, schema: &str) -> String {
    format!("{}/schema.json", schema_folder(c, schema))
}

/// 열 때 자동 동기화하는 최소 간격 — 폴링 대신 '열 때 한 번'
pub const SYNC_THROTTLE_MS: i64 = 10 * 60 * 1000;

pub fn is_stale(synced_at: Option<i64>, now: i64) -> bool {
    synced_at.is_none_or(|t| now - t > SYNC_THROTTLE_MS)
}

pub fn enabled_schemas(c: &DbConnection) -> Vec<&DbSchemaPref> {
    c.schemas.iter().filter(|s| s.enabled).collect()
}

pub fn env_label(env: &str) -> String {
    crate::i18n::t(match env {
        "prod" => "erd.db.env.prod",
        "staging" => "erd.db.env.staging",
        _ => "erd.db.env.dev",
    })
}

/// 연결 상태 — 키체인을 읽어 확인하지 않는다(읽기만 해도 macOS 확인창). 실패 흔적(last_error)으로 안다
pub fn conn_status(c: &DbConnection) -> &'static str {
    match c.last_error.as_deref() {
        Some("KEYCHAIN_MISSING") => "needsPassword",
        Some(e) if !e.is_empty() => "failed",
        _ if c.last_sync_at.is_some_and(|t| t != 0) => "connected",
        _ => "never",
    }
}

pub fn conn_status_label(st: &str) -> String {
    crate::i18n::t(&format!("erd.db.status.{st}"))
}

/// 상태 점의 꼴 — 채움=연결됨, 색=안 됐다면 왜. 동기화 전은 빈 글(무채색)
pub fn conn_status_dot(st: &str) -> &'static str {
    match st {
        "connected" => "ok",
        "needsPassword" => "warn",
        "failed" => "bad",
        _ => "",
    }
}

/// 연결 이름 → 폴더 이름. 경로 구분자, 앞의 점만 거른다(80 UTF-16 단위까지)
pub fn folder_name_for(name: &str) -> String {
    let t: String = name
        .trim_matches(|c: char| {
            // JS 의 trim 이 보는 공백(U+0085 는 아니고 U+FEFF 는 맞다)
            matches!(
                c,
                '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}'
                        | '\u{2028}'
                        | '\u{2029}'
                        | '\u{202f}'
                        | '\u{205f}'
                        | '\u{3000}'
                        | '\u{feff}'
            )
        })
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let t = t.trim_start_matches('.');
    let units: Vec<u16> = t.encode_utf16().take(80).collect();
    let out = String::from_utf16_lossy(&units);
    if out.is_empty() { "db".into() } else { out }
}

/// 하네스 — 순수 판정
pub fn call_pure(op: &str, args: &[Json]) -> Option<std::result::Result<Json, String>> {
    let conn = || -> std::result::Result<DbConnection, String> {
        serde_json::from_value(args.first().cloned().unwrap_or(Json::Null))
            .map_err(|e| e.to_string())
    };
    let s = |i: usize| {
        args.get(i)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let out = match op {
        "dbconn.prefAudit" => {
            serde_json::from_value::<DbSchemaPref>(args.first().cloned().unwrap_or(Json::Null))
                .map(|p| json!(pref_audit(&p)))
                .map_err(|e| e.to_string())
        }
        "dbconn.schemaFolder" => conn().map(|c| json!(schema_folder(&c, &s(1)))),
        "dbconn.enabledSchemas" => conn().map(|c| json!(enabled_schemas(&c))),
        "dbconn.isStale" => {
            let synced = args
                .first()
                .and_then(|v| v.get("synced_at"))
                .and_then(Json::as_i64);
            let now = args.get(1).and_then(Json::as_i64).unwrap_or_default();
            Ok(json!(is_stale(synced, now)))
        }
        "dbconn.envLabel" => Ok(json!(env_label(&s(0)))),
        "dbconn.connStatus" => conn().map(|c| json!(conn_status(&c))),
        "dbconn.connStatusLabel" => Ok(json!(conn_status_label(&s(0)))),
        "dbconn.connStatusDot" => Ok(json!(conn_status_dot(&s(0)))),
        "dbconn.folderNameFor" => Ok(json!(folder_name_for(&s(0)))),
        "dbconn.profileOf" => {
            let a = args.first().cloned().unwrap_or(Json::Null);
            Ok(
                json!({"ulid": a["ulid"], "kind": a["kind"], "host": a["host"], "port": a["port"], "username": a["username"], "tls": a["tls"], "credential_command": a["credential_command"]}),
            )
        }
        _ => return None,
    };
    Some(out)
}
