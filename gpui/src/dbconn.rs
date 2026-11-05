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
