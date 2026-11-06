//! DB 연동 — 스키마 스냅샷 → resin 원문(src/lib/erdGen.ts). 여기는 사내 스키마를 읽는 규칙만 갖는다: 이름과 코멘트로
//! 논리 참조 추론, 암호화 컬럼 판정, 깨진 인코딩 복원, CHECK 의 값 목록. resin 원문 쓰기(타입 정규화, 키, 인덱스, FK,
//! Envers 접기, 정렬)는 resin 의 fromSchema(resin::sql) 가 한다.
//! 결정적이어야 한다 — 같은 스냅샷이면 바이트까지 같은 원문. 정렬은 JS 처럼 UTF-16 코드 유닛 비교.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::resin::RefKind;
use crate::resin::sql::{
    SchemaColumn, SchemaForeignKey, SchemaIndex, SchemaInput, SchemaTable, from_schema,
};

// ---- 스냅샷(schemaSnapshot.ts) — 형태와 지문, 비교는 schema_snapshot 이 갖는다 ----

pub use crate::schema_snapshot::{
    SchemaSnapshot, SnapshotCheck, SnapshotColumn, SnapshotForeignKey, SnapshotIndex, SnapshotTable,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefEdge {
    /// 자식 — `*_id` 컬럼을 가진 쪽
    pub from: String,
    pub from_column: String,
    pub to: String,
    /// 논리 참조는 'id'(또는 같은 이름의 UNIQUE), 물리 FK 는 참조 컬럼
    pub to_column: String,
    /// true = 선언된 FOREIGN KEY, false = 이름이나 코멘트로 추론
    pub physical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unresolved {
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErdGenResult {
    pub source: String,
    /// resin 이 그대로 옮기지 못한 것 — 사람이 읽을 문장
    pub notes: Vec<String>,
}

// ---- 이름, 문자열 ----

/// 로케일 무관 — JS 의 < 와 같은 UTF-16 코드 유닛 비교
fn cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn is_audit(n: &str) -> bool {
    n.to_lowercase().ends_with("_aud")
}
fn is_revinfo(n: &str) -> bool {
    n.to_lowercase() == "revinfo"
}
/// 도메인 표 = 감사 표도 revinfo 도 아닌 것
fn is_domain(n: &str) -> bool {
    !is_audit(n) && !is_revinfo(n)
}

/// windows-1252 가 0x80~0x9F 에 둔 글자 → 바이트
const CP1252_REVERSE: [(u16, u8); 27] = [
    (0x20ac, 0x80),
    (0x201a, 0x82),
    (0x0192, 0x83),
    (0x201e, 0x84),
    (0x2026, 0x85),
    (0x2020, 0x86),
    (0x2021, 0x87),
    (0x02c6, 0x88),
    (0x2030, 0x89),
    (0x0160, 0x8a),
    (0x2039, 0x8b),
    (0x0152, 0x8c),
    (0x017d, 0x8e),
    (0x2018, 0x91),
    (0x2019, 0x92),
    (0x201c, 0x93),
    (0x201d, 0x94),
    (0x2022, 0x95),
    (0x2013, 0x96),
    (0x2014, 0x97),
    (0x02dc, 0x98),
    (0x2122, 0x99),
    (0x0161, 0x9a),
    (0x203a, 0x9b),
    (0x0153, 0x9c),
    (0x017e, 0x9e),
    (0x0178, 0x9f),
];

/// 잘못된 문자셋으로 저장된 코멘트("ì—¬í–‰")를 되살린다 — UTF-8 바이트를 latin1 로 읽은 꼴이면 되돌린다
fn fix_mojibake(s: &str) -> String {
    if !s.chars().any(|c| ('\u{c2}'..='\u{f4}').contains(&c)) {
        return s.to_string();
    }
    let mut bytes = Vec::new();
    for code in s.encode_utf16() {
        if code > 0xff {
            match CP1252_REVERSE.iter().find(|(c, _)| *c == code) {
                Some((_, b)) => bytes.push(*b),
                None => return s.to_string(),
            }
        } else {
            bytes.push(code as u8);
        }
    }
    match String::from_utf8(bytes) {
        // TextDecoder 는 앞의 BOM 을 먹는다
        Ok(t) => t.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(t),
        Err(_) => s.to_string(),
    }
}

/// JS 의 \s
fn js_space(c: char) -> bool {
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
}

/// 정규식 안에 쓰는 JS \s
const JS_WS: &str = r"[\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]";

/// 코멘트 정리 — 깨진 인코딩 복원, 줄바꿈이 든 공백 덩어리는 공백 하나로(설명은 한 줄), 양끝 공백 제거
fn clean_text(s: &str) -> String {
    let fixed = fix_mojibake(s);
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.contains(['\r', '\n']) {
            out.push(' ');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in fixed.chars() {
        if js_space(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out.trim_matches(js_space).to_string()
}

fn is_created_at(c: &SnapshotColumn) -> bool {
    c.name.to_lowercase() == "created_at"
}
fn is_updated_at(c: &SnapshotColumn) -> bool {
    c.name.to_lowercase() == "updated_at"
}

/// 표 이름들의 최장 공통 접두사를 마지막 '_' 직후까지 — 도메인 표만 넘긴다
pub fn common_table_prefix(names: &[String]) -> String {
    if names.len() < 2 {
        return String::new();
    }
    let mut lcp: Vec<u16> = names[0].encode_utf16().collect();
    for n in &names[1..] {
        if lcp.is_empty() {
            break;
        }
        let n: Vec<u16> = n.encode_utf16().collect();
        let k = lcp.iter().zip(&n).take_while(|(a, b)| a == b).count();
        lcp.truncate(k);
    }
    if let Some(cut) = lcp.iter().rposition(|&u| u == b'_' as u16) {
        return String::from_utf16_lossy(&lcp[..=cut]);
    }
    // 공통이 없어도 대다수가 같은 첫 토큰을 쓰면 그게 접두사다 — `editing_logs` 하나 때문에 `ts_` 마흔 개가 접두사를 잃지 않게
    let mut counts: Vec<(String, usize)> = Vec::new();
    for n in names {
        if let Some(i) = n.find('_').filter(|&i| i > 0) {
            let tok = n[..=i].to_string();
            match counts.iter_mut().find(|(t, _)| *t == tok) {
                Some(e) => e.1 += 1,
                None => counts.push((tok, 1)),
            }
        }
    }
    let (mut best, mut best_n) = (String::new(), 0);
    for (tok, c) in counts {
        if c > best_n || (c == best_n && cmp(&tok, &best).is_lt()) {
            best = tok;
            best_n = c;
        }
    }
    if best_n >= 2 && best_n * 5 >= names.len() * 3 {
        best
    } else {
        String::new()
    }
}

// ---- 인덱스 ----

fn is_primary(name: &str) -> bool {
    name.eq_ignore_ascii_case("primary")
}

fn indexes_of(t: &SnapshotTable) -> Vec<&SnapshotIndex> {
    t.indexes
        .iter()
        .filter(|ix| !is_primary(&ix.name))
        .collect()
}

fn has_single_unique(t: &SnapshotTable, c: &SnapshotColumn) -> bool {
    c.key == "UNI"
        || indexes_of(t)
            .iter()
            .any(|ix| ix.unique && ix.columns.len() == 1 && ix.columns[0] == c.name)
}

// ---- 타입 호환 ----

const INT_TYPES: [&str; 6] = [
    "tinyint",
    "smallint",
    "mediumint",
    "int",
    "integer",
    "bigint",
];
const CHAR_TYPES: [&str; 6] = [
    "char",
    "varchar",
    "text",
    "tinytext",
    "mediumtext",
    "longtext",
];

fn types_compatible(a: &str, b: &str) -> bool {
    let (x, y) = (a.to_lowercase(), b.to_lowercase());
    x == y
        || (INT_TYPES.contains(&x.as_str()) && INT_TYPES.contains(&y.as_str()))
        || (CHAR_TYPES.contains(&x.as_str()) && CHAR_TYPES.contains(&y.as_str()))
}

/// 후보 표에서 자식 컬럼이 가리킬 컬럼 — 타입이 맞는 `id` PK, 아니면 같은 이름의 UNIQUE/PK 컬럼
fn target_column(t: &SnapshotTable, c: &SnapshotColumn) -> Option<String> {
    if let Some(id) = t
        .columns
        .iter()
        .find(|col| col.name == "id" && col.key == "PRI")
        && types_compatible(&id.data_type, &c.data_type)
    {
        return Some("id".into());
    }
    let same = t.columns.iter().find(|col| {
        col.name == c.name && (col.key == "PRI" || col.key == "UNI" || has_single_unique(t, col))
    })?;
    types_compatible(&same.data_type, &c.data_type).then(|| same.name.clone())
}

// ---- 코멘트 속 참조 ----

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// /(?<![\w.])([A-Za-z]\w*)\.([A-Za-z_]\w*)(?![\w.])/g — 앞뒤에 점이 없는 `table.column` 한 쌍(세 토막은 걸리지 않는다)
fn table_dot_column(s: &str) -> Vec<(String, String)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let ok_before = i == 0 || !(word(b[i - 1]) || b[i - 1] == b'.');
        if !(ok_before && b[i].is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && word(b[j]) {
            j += 1;
        }
        if j + 1 < b.len() && b[j] == b'.' && (b[j + 1].is_ascii_alphabetic() || b[j + 1] == b'_') {
            let mut k = j + 2;
            while k < b.len() && word(b[k]) {
                k += 1;
            }
            if k == b.len() || !(word(b[k]) || b[k] == b'.') {
                out.push((s[i..j].to_string(), s[j + 1..k].to_string()));
                i = k;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn comment_refs(
    comment: &str,
    by_name: &HashMap<&str, &SnapshotTable>,
    me: &str,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (table, column) in table_dot_column(comment) {
        let Some(t) = by_name.get(table.as_str()) else {
            continue;
        };
        if table == me || !is_domain(&table) || !t.columns.iter().any(|c| c.name == column) {
            continue;
        }
        if !out.contains(&(table.clone(), column.clone())) {
            out.push((table, column));
        }
    }
    out
}

// ---- 논리 참조 추론 ----

/// `x_id` 의 x 가 가리킬 만한 표 이름 — 단수, 복수, 접두사 유무
fn candidate_names(x: &str, prefix: &str) -> Vec<String> {
    let mut raw = vec![
        x.to_string(),
        format!("{x}s"),
        format!("{x}es"),
        format!("{prefix}{x}"),
        format!("{prefix}{x}s"),
        format!("{prefix}{x}es"),
    ];
    if let Some(stem) = x.strip_suffix('y') {
        let ies = format!("{stem}ies");
        raw.push(ies.clone());
        raw.push(format!("{prefix}{ies}"));
    }
    let mut out: Vec<String> = Vec::new();
    for r in raw {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// 이름이 `_x`, `_xs` 로 끝나는 표 — 정확한 이름이 안 맞을 때의 2차 후보
fn suffix_candidates(x: &str, names: &[&str]) -> Vec<String> {
    let mut forms = vec![x.to_string(), format!("{x}s"), format!("{x}es")];
    if let Some(stem) = x.strip_suffix('y') {
        forms.push(format!("{stem}ies"));
    }
    names
        .iter()
        .filter(|n| forms.iter().any(|f| n.ends_with(&format!("_{f}"))))
        .map(|n| n.to_string())
        .collect()
}

/// 스냅샷 전체의 관계 — 선언된 FK(물리) + 이름과 코멘트로 추론한 참조(논리). 후보가 하나일 때만 잇는다
pub fn infer_references(snapshot: &SchemaSnapshot) -> (Vec<RefEdge>, Vec<Unresolved>) {
    let by_name: HashMap<&str, &SnapshotTable> = snapshot
        .tables
        .iter()
        .map(|t| (t.name.as_str(), t))
        .collect();
    let domain: Vec<String> = snapshot
        .tables
        .iter()
        .map(|t| t.name.clone())
        .filter(|n| is_domain(n))
        .collect();
    let prefix = common_table_prefix(&domain);
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();

    for t in &snapshot.tables {
        let mut covered: Vec<&str> = Vec::new();
        for fk in &t.foreign_keys {
            covered.extend(fk.columns.iter().map(String::as_str));
            if fk.columns.is_empty() {
                continue;
            }
            if fk.ref_schema != snapshot.schema || !by_name.contains_key(fk.ref_table.as_str()) {
                continue;
            }
            edges.push(RefEdge {
                from: t.name.clone(),
                from_column: fk.columns[0].clone(),
                to: fk.ref_table.clone(),
                to_column: fk
                    .ref_columns
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "id".into()),
                physical: true,
            });
        }
        if !is_domain(&t.name) {
            continue;
        }
        let domain_names: Vec<&str> = snapshot
            .tables
            .iter()
            .map(|x| x.name.as_str())
            .filter(|n| *n != t.name && is_domain(n))
            .collect();
        for c in &t.columns {
            if c.key == "PRI" || covered.contains(&c.name.as_str()) {
                continue;
            }
            let logical = |to: &str, to_column: &str| RefEdge {
                from: t.name.clone(),
                from_column: c.name.clone(),
                to: to.into(),
                to_column: to_column.into(),
                physical: false,
            };
            // 1) 코멘트가 대상을 적어 뒀으면 그게 가장 정확하다
            let refs: Vec<(String, String)> = comment_refs(&c.comment, &by_name, &t.name)
                .into_iter()
                .filter(|(table, column)| {
                    let target = by_name[table.as_str()]
                        .columns
                        .iter()
                        .find(|col| &col.name == column)
                        .expect("있는 컬럼만 남겼다");
                    types_compatible(&target.data_type, &c.data_type)
                })
                .collect();
            if refs.len() == 1 {
                edges.push(logical(&refs[0].0, &refs[0].1));
                continue;
            }
            // 2) `x_id` 이름 규칙 — 정확한 이름 후보가 하나면 그것, 아니면 `_x` 로 끝나는 표가 하나일 때
            let Some(x) = c.name.strip_suffix("_id").filter(|x| !x.is_empty()) else {
                continue;
            };
            let resolve = |names: Vec<String>| -> Vec<(String, String)> {
                names
                    .into_iter()
                    .filter_map(|n| target_column(by_name[n.as_str()], c).map(|col| (n, col)))
                    .collect()
            };
            let exact = resolve(
                candidate_names(x, &prefix)
                    .into_iter()
                    .filter(|n| domain_names.contains(&n.as_str()))
                    .collect(),
            );
            let hits = if exact.len() == 1 {
                exact
            } else {
                resolve(suffix_candidates(x, &domain_names))
            };
            if hits.len() != 1 {
                unresolved.push(Unresolved {
                    table: t.name.clone(),
                    column: c.name.clone(),
                });
                continue;
            }
            edges.push(logical(&hits[0].0, &hits[0].1));
        }
    }
    edges.sort_by(|a, b| {
        cmp(&a.to, &b.to)
            .then_with(|| cmp(&a.from, &b.from))
            .then_with(|| cmp(&a.from_column, &b.from_column))
    });
    unresolved.sort_by(|a, b| cmp(&a.table, &b.table).then_with(|| cmp(&a.column, &b.column)));
    (edges, unresolved)
}

// ---- 생성 ----

/// /암호화|encrypt|enc\b/i
fn mentions_encryption(s: &str) -> bool {
    let low = s.to_ascii_lowercase();
    if low.contains("암호화") || low.contains("encrypt") {
        return true;
    }
    low.match_indices("enc")
        .any(|(i, _)| low.as_bytes().get(i + 3).is_none_or(|&b| !word(b)))
}

static TRIVIAL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^(id|pk|primary key|고유{JS_WS}*식별자|식별자|아이디)$"
    ))
    .unwrap()
});
const SENSITIVE: [&str; 10] = [
    "name", "phone", "mobile", "email", "birth", "passport", "doc_id", "ssn", "card_no", "address",
];
static WIDE_VARCHAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^varchar\((\d+)\)").unwrap());
static QUOTED_VALUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'((?:[^']|'')*)'").unwrap());

/// 암호문을 담을 만큼 넓은 컬럼 — text 또는 varchar(255+)
fn is_wide_text(c: &SnapshotColumn) -> bool {
    if c.data_type.to_lowercase() == "text" {
        return true;
    }
    WIDE_VARCHAR
        .captures(c.column_type.trim_matches(js_space))
        .and_then(|m| m[1].parse::<f64>().ok())
        .is_some_and(|n| n >= 255.)
}

/// 코멘트가 암호화를 말하거나, 같은 표에 그런 컬럼이 있고 이름, 폭이 개인정보 컬럼답다
fn is_encrypted(c: &SnapshotColumn, table_has_enc_comment: bool) -> bool {
    if mentions_encryption(&c.comment) {
        return true;
    }
    let low = c.name.to_ascii_lowercase();
    table_has_enc_comment && SENSITIVE.iter().any(|w| low.contains(w)) && is_wide_text(c)
}

/// `'A','B'` 에서 값만 — MySQL CHECK 의 `_utf8mb4'A'` 도 통과한다
fn quoted_values(s: &str) -> Vec<String> {
    QUOTED_VALUE
        .captures_iter(s)
        .map(|m| m[1].replace("''", "'"))
        .collect()
}

/// 같은 표 CHECK 의 `col in (...)` 값 목록
fn check_values(t: &SnapshotTable, c: &SnapshotColumn) -> Option<Vec<String>> {
    let re = Regex::new(&format!(
        r"(?i)(?:^|[^A-Za-z0-9_])`?{}`?{JS_WS}+in{JS_WS}*\(([^)]*)\)",
        regex::escape(&c.name)
    ))
    .ok()?;
    for k in &t.checks {
        let Some(m) = re.captures(&k.clause) else {
            continue;
        };
        let v = quoted_values(&m[1]);
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// 컬럼 설명 — 코멘트의 뜻만. `id` PK 의 'ID' 같은 되풀이는 비운다
fn meaning_of(c: &SnapshotColumn) -> String {
    let meaning = clean_text(&c.comment);
    if c.key == "PRI" && c.name.to_lowercase() == "id" && TRIVIAL_ID.is_match(&meaning) {
        return String::new();
    }
    meaning
}

/// created_at → updated_at 을 맨 뒤로. 나머지는 스냅샷(DDL) 순서
fn order_columns(cols: &[SnapshotColumn]) -> Vec<&SnapshotColumn> {
    cols.iter()
        .filter(|c| !is_created_at(c) && !is_updated_at(c))
        .chain(cols.iter().filter(|c| is_created_at(c)))
        .chain(cols.iter().filter(|c| is_updated_at(c)))
        .collect()
}

/// 스냅샷 → resin 원문. 선언된 FK 는 `->`, 추론한 논리 참조는 `~>`. audit = false 면 감사 표를 싣지 않는다
pub fn generate_erd(snapshot: &SchemaSnapshot, audit: Option<bool>) -> ErdGenResult {
    let (edges, _) = infer_references(snapshot);
    let mut logical: Vec<(String, Vec<RefEdge>)> = Vec::new();
    for e in edges.into_iter().filter(|e| !e.physical) {
        match logical.iter_mut().find(|(f, _)| *f == e.from) {
            Some(l) => l.1.push(e),
            None => logical.push((e.from.clone(), vec![e])),
        }
    }
    // 표는 이름순 — 스냅샷이 어떤 순서로 오든 같은 원문
    let mut tables: Vec<&SnapshotTable> = snapshot
        .tables
        .iter()
        .filter(|t| audit != Some(false) || is_domain(&t.name))
        .collect();
    tables.sort_by(|a, b| cmp(&a.name, &b.name));

    let input = SchemaInput {
        tables: tables
            .iter()
            .map(|t| {
                let enc_table = t.columns.iter().any(|c| mentions_encryption(&c.comment));
                let primary = t.indexes.iter().find(|ix| is_primary(&ix.name));
                let mut foreign_keys: Vec<SchemaForeignKey> = t
                    .foreign_keys
                    .iter()
                    .filter(|fk| !fk.columns.is_empty())
                    .map(|fk| SchemaForeignKey {
                        name: Some(fk.name.clone()),
                        columns: fk.columns.clone(),
                        table: fk.ref_table.clone(),
                        // 같은 스키마면 이름만 — 스냅샷에 없는 대상이 `svc_x.t` 같은 긴 이름이 되지 않게
                        schema: (fk.ref_schema != snapshot.schema).then(|| fk.ref_schema.clone()),
                        references: (!fk.ref_columns.is_empty()).then(|| fk.ref_columns.clone()),
                        kind: None,
                    })
                    .collect();
                if let Some((_, es)) = logical.iter().find(|(f, _)| *f == t.name) {
                    foreign_keys.extend(es.iter().map(|e| SchemaForeignKey {
                        name: None,
                        columns: vec![e.from_column.clone()],
                        table: e.to.clone(),
                        schema: Some(snapshot.schema.clone()),
                        references: Some(vec![e.to_column.clone()]),
                        kind: Some(RefKind::Logical),
                    }));
                }
                let ixs = indexes_of(t);
                let mut indexes: Vec<SchemaIndex> = ixs
                    .iter()
                    .map(|ix| SchemaIndex {
                        name: Some(ix.name.clone()),
                        columns: ix.columns.clone(),
                        unique: Some(ix.unique),
                    })
                    .collect();
                // key 는 UNI 인데 인덱스 목록에 그 UNIQUE 가 없으면(덜 읽힌 스냅샷) 이름 없이라도 싣는다
                indexes.extend(
                    t.columns
                        .iter()
                        .filter(|c| {
                            c.key == "UNI"
                                && !ixs.iter().any(|ix| {
                                    ix.unique && ix.columns.len() == 1 && ix.columns[0] == c.name
                                })
                        })
                        .map(|c| SchemaIndex {
                            name: None,
                            columns: vec![c.name.clone()],
                            unique: Some(true),
                        }),
                );
                let comment = clean_text(&t.comment);
                SchemaTable {
                    name: t.name.clone(),
                    schema: Some(snapshot.schema.clone()),
                    comment: (!comment.is_empty()).then_some(comment),
                    columns: order_columns(&t.columns)
                        .into_iter()
                        .map(|c| {
                            let meaning = meaning_of(c);
                            SchemaColumn {
                                name: c.name.clone(),
                                ty: c.column_type.clone(),
                                nullable: c.nullable,
                                comment: (!meaning.is_empty()).then_some(meaning),
                                values: check_values(t, c),
                                encrypted: Some(is_encrypted(c, enc_table)),
                            }
                        })
                        .collect(),
                    primary_key: Some(match primary {
                        Some(p) => p.columns.clone(),
                        None => t
                            .columns
                            .iter()
                            .filter(|c| c.key == "PRI")
                            .map(|c| c.name.clone())
                            .collect(),
                    }),
                    indexes: Some(indexes),
                    foreign_keys: Some(foreign_keys),
                }
            })
            .collect(),
    };
    let r = from_schema(&input);
    ErdGenResult {
        source: r.source,
        notes: r.notes.into_iter().map(|n| n.message).collect(),
    }
}

// ---- 하네스 ----

pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let snapshot = || -> Result<SchemaSnapshot, String> {
        serde_json::from_value(args.first().cloned().unwrap_or(Json::Null))
            .map_err(|e| e.to_string())
    };
    let out = match op {
        "erdGen.generateErd" => {
            let audit = args
                .get(1)
                .and_then(|o| o.get("audit"))
                .and_then(Json::as_bool);
            snapshot().map(|s| json!(generate_erd(&s, audit)))
        }
        "erdGen.inferReferences" => snapshot().map(|s| {
            let (edges, unresolved) = infer_references(&s);
            json!({"edges": edges, "unresolved": unresolved})
        }),
        "erdGen.commonTablePrefix" => {
            let names: Vec<String> = args
                .first()
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            Ok(json!(common_table_prefix(&names)))
        }
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mojibake_and_text_cleaning() {
        // "여행" 의 UTF-8 바이트를 latin1(windows-1252)로 읽은 꼴
        let broken: String = "여행"
            .bytes()
            .map(|b| {
                CP1252_REVERSE
                    .iter()
                    .find(|(_, x)| *x == b)
                    .map(|(c, _)| char::from_u32(*c as u32).unwrap())
                    .unwrap_or(b as char)
            })
            .collect();
        assert_eq!(fix_mojibake(&broken), "여행");
        assert_eq!(
            fix_mojibake("café"),
            "café",
            "되돌리면 깨진 UTF-8 이라 그대로"
        );
        assert_eq!(clean_text("  a \t b \n\n  c  "), "a \t b c");
    }

    #[test]
    fn comment_refs_take_two_part_names_only() {
        assert_eq!(
            table_dot_column("ts_customers.id - 유저, FK to ts_org.id, ts_x.id. svc.ts_p.id x.y.z"),
            vec![
                ("ts_customers".into(), "id".into()),
                ("ts_org".into(), "id".into())
            ],
            "뒤에 점이 오면(ts_x.id.) 걸리지 않는다"
        );
        assert!(mentions_encryption("AES 암호화"));
        assert!(mentions_encryption("ENC"));
        assert!(!mentions_encryption("encode"));
    }
}
