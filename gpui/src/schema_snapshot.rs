//! 스키마 스냅샷(src/lib/schemaSnapshot.ts) — `schema.json` 의 형태와 그 위의 순수 연산: 지문, 마무리, 읽기, 비교.
//! 지문은 앱이 파일에 남긴 값과 같아야 한다(바뀌었나를 그걸로 본다): cyrb53 을 UTF-16 단위로, 이름순은 로캘 없는 localeCompare,
//! 정규형은 JSON.stringify 와 같은 키 순서. 필드 순서는 웹이 쓰는 파일(JSON.stringify(snap, null, 2))과 같게 둔다.

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::vault::default_compare;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SnapshotColumn {
    pub name: String,
    /// 길이 없는 물리 타입 소문자 — 'bigint' 'varchar'
    pub data_type: String,
    /// 선언 그대로 — 'varchar(255)' 'bigint unsigned' "enum('A','B')"
    pub column_type: String,
    pub nullable: bool,
    /// PRI / UNI / MUL / ''
    pub key: String,
    pub default_value: Option<String>,
    pub extra: String,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SnapshotIndex {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SnapshotForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SnapshotCheck {
    pub name: String,
    pub clause: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SnapshotTable {
    pub name: String,
    pub comment: String,
    /// 추정치. 받은 숫자 그대로(1200 을 1200.0 으로 쓰지 않게)
    pub rows_estimate: Option<serde_json::Number>,
    pub columns: Vec<SnapshotColumn>,
    pub indexes: Vec<SnapshotIndex>,
    pub foreign_keys: Vec<SnapshotForeignKey>,
    pub checks: Vec<SnapshotCheck>,
}

/// 생성이 읽는 건 schema 와 tables 뿐 — 나머지는 옛 파일에 없어도 읽힌다
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SchemaSnapshot {
    #[serde(default)]
    pub amber: i64,
    #[serde(default)]
    pub connection: String,
    pub schema: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub synced_at: i64,
    pub tables: Vec<SnapshotTable>,
    #[serde(default)]
    pub fingerprint: String,
}

/// DB 가 돌려준 것(db_introspect) — 지문은 여기서 붙인다
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawSnapshot {
    pub connection: String,
    pub schema: String,
    pub server: String,
    pub synced_at: i64,
    pub tables: Vec<SnapshotTable>,
}

/// cyrb53 — Math.imul 은 i32 감싸 곱, `>>>` 는 u32 로 민다. 글자는 UTF-16 단위(charCodeAt)
fn hash53(s: &str) -> String {
    let mut h1: i32 = 0xdeadbeef_u32 as i32;
    let mut h2: i32 = 0x41c6ce57;
    for ch in s.encode_utf16() {
        let ch = ch as i32;
        h1 = (h1 ^ ch).wrapping_mul(2654435761_u32 as i32);
        h2 = (h2 ^ ch).wrapping_mul(1597334677);
    }
    let ushr = |v: i32, n: u32| ((v as u32) >> n) as i32;
    h1 = (h1 ^ ushr(h1, 16)).wrapping_mul(2246822507_u32 as i32)
        ^ (h2 ^ ushr(h2, 13)).wrapping_mul(3266489909_u32 as i32);
    h2 = (h2 ^ ushr(h2, 16)).wrapping_mul(2246822507_u32 as i32)
        ^ (h1 ^ ushr(h1, 13)).wrapping_mul(3266489909_u32 as i32);
    let n = 4294967296_u64 * ((2097151 & h2) as u64) + (h1 as u32 as u64);
    format!("{n:014x}")
}

/// [...xs].sort((a, b) => a.name.localeCompare(b.name)) — 안정 정렬
fn by_name<T>(xs: &[T], name: impl Fn(&T) -> &str) -> Vec<&T> {
    let mut v: Vec<&T> = xs.iter().collect();
    v.sort_by(|a, b| default_compare(name(a), name(b)));
    v
}

/// 구조 지문 — 표, 컬럼, 인덱스, FK, CHECK 를 이름순으로. rows_estimate 는 빼고(행이 는 건 스키마 변경이 아니다)
/// 코멘트는 넣는다(ERD 설명이 바뀐다)
pub fn fingerprint_of(tables: &[SnapshotTable]) -> String {
    // JSON.stringify 의 키 순서 n, c, cols, ix, fk, ck — 구조체는 선언 순서로 쓰인다
    #[derive(Serialize)]
    struct Canon<'a> {
        n: &'a str,
        c: &'a str,
        cols: Vec<(
            &'a str,
            &'a str,
            bool,
            &'a str,
            &'a Option<String>,
            &'a str,
            &'a str,
        )>,
        ix: Vec<(&'a str, bool, &'a [String])>,
        fk: Vec<(&'a str, &'a [String], &'a str, &'a str, &'a [String])>,
        ck: Vec<(&'a str, &'a str)>,
    }
    let canon: Vec<Canon> = by_name(tables, |t| &t.name)
        .into_iter()
        .map(|t| Canon {
            n: &t.name,
            c: &t.comment,
            cols: by_name(&t.columns, |c| &c.name)
                .into_iter()
                .map(|c| {
                    (
                        c.name.as_str(),
                        c.column_type.as_str(),
                        c.nullable,
                        c.key.as_str(),
                        &c.default_value,
                        c.extra.as_str(),
                        c.comment.as_str(),
                    )
                })
                .collect(),
            ix: by_name(&t.indexes, |i| &i.name)
                .into_iter()
                .map(|i| (i.name.as_str(), i.unique, i.columns.as_slice()))
                .collect(),
            fk: by_name(&t.foreign_keys, |f| &f.name)
                .into_iter()
                .map(|f| {
                    (
                        f.name.as_str(),
                        f.columns.as_slice(),
                        f.ref_schema.as_str(),
                        f.ref_table.as_str(),
                        f.ref_columns.as_slice(),
                    )
                })
                .collect(),
            ck: by_name(&t.checks, |k| &k.name)
                .into_iter()
                .map(|k| (k.name.as_str(), k.clause.as_str()))
                .collect(),
        })
        .collect();
    hash53(&serde_json::to_string(&canon).unwrap_or_default())
}

pub fn finalize_snapshot(raw: RawSnapshot) -> SchemaSnapshot {
    let fingerprint = fingerprint_of(&raw.tables);
    SchemaSnapshot {
        amber: 1,
        connection: raw.connection,
        schema: raw.schema,
        server: raw.server,
        synced_at: raw.synced_at,
        tables: raw.tables,
        fingerprint,
    }
}

/// `schema.json` 본문의 모양만 본다(amber 가 1, schema 가 글자, tables 가 배열) — 어긋나면 None(깨진 파일은 없는 것)
pub fn parse_snapshot_json(text: &str) -> Option<Json> {
    let v: Json = serde_json::from_str(text).ok()?;
    let ok = v.get("amber").and_then(Json::as_f64) == Some(1.)
        && v.get("schema").is_some_and(Json::is_string)
        && v.get("tables").is_some_and(Json::is_array);
    ok.then_some(v)
}

/// 앱이 읽는 스냅샷 — 모양을 본 뒤 그 형태로(형태가 틀린 표가 섞였으면 None)
pub fn parse_snapshot(text: &str) -> Option<SchemaSnapshot> {
    serde_json::from_value(parse_snapshot_json(text)?).ok()
}

/// 파일로 쓸 본문 — JSON.stringify(snap, null, 2) + "\n"
pub fn snapshot_text(snap: &SchemaSnapshot) -> String {
    serde_json::to_string_pretty(snap).unwrap_or_default() + "\n"
}

// ---- 비교 ----

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    pub tables_added: Vec<String>,
    pub tables_removed: Vec<String>,
    /// 'table.column'
    pub columns_added: Vec<String>,
    pub columns_removed: Vec<String>,
    /// 타입, 널, 키, 기본값, 코멘트가 바뀐 컬럼 'table.column'
    pub columns_changed: Vec<String>,
    /// 인덱스, FK, CHECK 가 바뀐 표
    pub constraint_tables: Vec<String>,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.tables_added.is_empty()
            && self.tables_removed.is_empty()
            && self.columns_added.is_empty()
            && self.columns_removed.is_empty()
            && self.columns_changed.is_empty()
            && self.constraint_tables.is_empty()
    }
}

/// new Map(xs.map(x => [name, x])) — 같은 이름이면 자리는 처음, 값은 나중 것
fn js_map<'a, T>(xs: &'a [T], name: impl Fn(&'a T) -> &'a str) -> Vec<(&'a str, &'a T)> {
    let mut out: Vec<(&'a str, &'a T)> = Vec::new();
    for x in xs {
        let n = name(x);
        match out.iter_mut().find(|(k, _)| *k == n) {
            Some(slot) => slot.1 = x,
            None => out.push((n, x)),
        }
    }
    out
}

fn col_sig(c: &SnapshotColumn) -> String {
    [
        c.column_type.clone(),
        c.nullable.to_string(),
        c.key.clone(),
        c.default_value.clone().unwrap_or_else(|| " ".into()),
        c.extra.clone(),
        c.comment.clone(),
    ]
    .join("|")
}

fn constraint_sig(t: &SnapshotTable) -> String {
    serde_json::to_string(&(
        by_name(&t.indexes, |i| &i.name),
        by_name(&t.foreign_keys, |f| &f.name),
        by_name(&t.checks, |k| &k.name),
    ))
    .unwrap_or_default()
}

pub fn diff_snapshots(prev: &SchemaSnapshot, next: &SchemaSnapshot) -> SchemaDiff {
    let mut d = SchemaDiff::default();
    let before = js_map(&prev.tables, |t| t.name.as_str());
    let after = js_map(&next.tables, |t| t.name.as_str());
    let has = |m: &[(&str, &SnapshotTable)], n: &str| m.iter().any(|(k, _)| *k == n);
    for (name, _) in &after {
        if !has(&before, name) {
            d.tables_added.push(name.to_string());
        }
    }
    for (name, _) in &before {
        if !has(&after, name) {
            d.tables_removed.push(name.to_string());
        }
    }
    for (name, b) in &before {
        let Some((_, a)) = after.iter().find(|(k, _)| k == name) else {
            continue;
        };
        let bc = js_map(&b.columns, |c| c.name.as_str());
        let ac = js_map(&a.columns, |c| c.name.as_str());
        for (cn, _) in &ac {
            if !bc.iter().any(|(k, _)| k == cn) {
                d.columns_added.push(format!("{name}.{cn}"));
            }
        }
        for (cn, c) in &bc {
            match ac.iter().find(|(k, _)| k == cn) {
                None => d.columns_removed.push(format!("{name}.{cn}")),
                Some((_, n)) if col_sig(c) != col_sig(n) => {
                    d.columns_changed.push(format!("{name}.{cn}"))
                }
                _ => {}
            }
        }
        if constraint_sig(b) != constraint_sig(a) {
            d.constraint_tables.push(name.to_string());
        }
    }
    d
}

/// 하네스가 재는 연산
pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Json::Null);
    let parse =
        |i: usize| serde_json::from_value::<SchemaSnapshot>(arg(i)).map_err(|e| e.to_string());
    let out = match op {
        "schemaSnapshot.fingerprintOf" => serde_json::from_value::<Vec<SnapshotTable>>(arg(0))
            .map(|t| json!(fingerprint_of(&t)))
            .map_err(|e| e.to_string()),
        "schemaSnapshot.finalizeSnapshot" => serde_json::from_value::<RawSnapshot>(arg(0))
            .map(|r| json!(finalize_snapshot(r)))
            .map_err(|e| e.to_string()),
        "schemaSnapshot.parseSnapshot" => Ok(arg(0)
            .as_str()
            .and_then(parse_snapshot_json)
            .unwrap_or(Json::Null)),
        "schemaSnapshot.diffSnapshots" => {
            parse(0).and_then(|p| parse(1).map(|n| json!(diff_snapshots(&p, &n))))
        }
        "schemaSnapshot.diffIsEmpty" => {
            let d = arg(0);
            let empty = |k: &str| d[k].as_array().is_none_or(|a| a.is_empty());
            Ok(json!(
                [
                    "tablesAdded",
                    "tablesRemoved",
                    "columnsAdded",
                    "columnsRemoved",
                    "columnsChanged",
                    "constraintTables"
                ]
                .iter()
                .all(|k| empty(k))
            ))
        }
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{SchemaSnapshot, hash53, parse_snapshot, snapshot_text};

    /// node 의 cyrb53 결과
    #[test]
    fn hash53_matches_javascript() {
        assert_eq!(hash53(""), "0bdcb81aee8d83");
        // 서로게이트 쌍은 두 단위로 센다
        assert_eq!(hash53("😀한"), "091ae20634f6e4");
    }

    /// 파일 본문은 JSON.stringify(snap, null, 2) + "\n" 과 같은 모양 — 키 순서, 빈 배열, 정수
    #[test]
    fn snapshot_text_round_trips() {
        let text = "{\n  \"amber\": 1,\n  \"connection\": \"c\",\n  \"schema\": \"s\",\n  \"server\": \"MySQL 8\",\n  \"synced_at\": 5,\n  \"tables\": [\n    {\n      \"name\": \"t\",\n      \"comment\": \"\",\n      \"rows_estimate\": 1200,\n      \"columns\": [],\n      \"indexes\": [],\n      \"foreign_keys\": [],\n      \"checks\": []\n    }\n  ],\n  \"fingerprint\": \"f\"\n}\n";
        let snap: SchemaSnapshot = parse_snapshot(text).unwrap();
        assert_eq!(snapshot_text(&snap), text);
        assert!(parse_snapshot("{\"amber\":2,\"schema\":\"s\",\"tables\":[]}").is_none());
    }
}
