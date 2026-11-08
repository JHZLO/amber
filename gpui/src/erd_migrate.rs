//! 다이어그램 탭 시절 ERD 옮기기(src/lib/erdMigrate.ts) — v0.23 에 DB 연동 ERD 가 ERD 탭으로 왔다. 옮길 것은 스냅샷뿐이다:
//! ERD 탭은 스냅샷에서 바로 그려 다이어그램 보관함의 mermaid 파일은 필요 없다. 스냅샷은 말없이 복사하고(원본은 그대로),
//! 옛 자리에 남은 자동 생성물을 지우는 것은 사용자가 고른다 — 휴지통으로, 손으로 둔 파일은 건드리지 않는다.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::db::Db;
use crate::dbconn::{DbConnection, enabled_schemas, schema_folder, snapshot_path};
use crate::schema_snapshot::parse_snapshot_json;
use crate::vault::{DIAGRAMS, ERD, Tree};

/// JS 정규식의 `\s` 와 `.`(줄 끝 넷을 뺀 전부) — Rust 의 기본값과 \r, U+FEFF, U+0085 에서 갈린다
const JS_WS: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
const JS_NOT_WS: &str = r"[^\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
const JS_DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

/// 자동 생성된 ERD 의 표식 — 앞머리의 `%% amber:db <연결>/<스키마> …` 한 줄(연결 이름엔 공백이 올 수 있다)
pub fn is_generated_erd(source: &str) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let not_ws_slash = JS_NOT_WS.replacen("[^", "[^/", 1);
        Regex::new(&format!("^{JS_WS}*%% amber:db {JS_DOT}+/{not_ws_slash}+ ")).unwrap()
    });
    // source.split("\n", 5)
    source.split('\n').take(5).any(|line| RE.is_match(line))
}

/// 옛 자리 — 지금 다이어그램 보관함(사용자가 고른 폴더일 수 있다)
fn legacy(db: &Db) -> Tree<'_> {
    Tree::new(db, &DIAGRAMS)
}

/// ERD 저장소에 없는 스냅샷을 옛 자리에서 복사해 온다 — 읽힌 모양 그대로(키 순서까지) JSON.stringify(snap, null, 2)
pub fn copy_legacy_snapshots(db: &Db, list: &[DbConnection]) -> Result<(), String> {
    let erd = Tree::new(db, &ERD);
    let old = legacy(db);
    for c in list {
        for p in enabled_schemas(c) {
            let rel = snapshot_path(c, &p.name);
            let have = erd
                .read_file_if_exists(&rel)
                .ok()
                .flatten()
                .and_then(|b| parse_snapshot_json(&b));
            if have.is_some() {
                continue;
            }
            let Some(snap) = old
                .read_file_if_exists(&rel)
                .ok()
                .flatten()
                .and_then(|b| parse_snapshot_json(&b))
            else {
                continue;
            };
            erd.ensure_dir(&schema_folder(c, &p.name))?;
            let text = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())? + "\n";
            erd.write_file(&rel, &text)?;
        }
    }
    Ok(())
}

/// 옛 자리에서 지울 것 — 폴더째(자동 생성물뿐일 때) 또는 파일 하나씩(손으로 둔 파일이 섞였을 때)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LegacyPlan {
    pub folders: Vec<String>,
    pub files: Vec<String>,
    /// 그중 mermaid ERD 파일 수 — 사용자에게 말할 숫자
    pub erds: usize,
}

/// 폴더 아래 모든 파일(상대경로) — 확장자를 거르지 않는다(손으로 넣은 .png, .md 도 사용자 파일로 센다).
/// 점으로 시작하는 것(.DS_Store)만 뺀다. 순서는 readDir 그대로
fn list_all(tree: &Tree, rel: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(tree.abs(rel)).ok()? {
        let e = e.ok()?;
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{rel}/{name}");
        if e.file_type().ok()?.is_dir() {
            out.extend(list_all(tree, &path)?);
        } else {
            out.push(path);
        }
    }
    Some(out)
}

fn is_mermaid(f: &str) -> bool {
    let lower = f.to_lowercase();
    lower.ends_with(".mmd") || lower.ends_with(".mermaid")
}

/// 연결 폴더마다 옛 자리에 남은 자동 생성물을 찾는다. 없으면 None. 지우기 직전에 다시 부른다 —
/// 띠가 떠 있는 사이 다이어그램 탭에서 그 폴더에 파일을 만들 수 있다
pub fn find_legacy_erds(db: &Db, list: &[DbConnection]) -> Option<LegacyPlan> {
    let old = legacy(db);
    let mut plan = LegacyPlan::default();
    for c in list {
        if !old.file_exists(&c.folder_path) {
            continue;
        }
        let Some(files) = list_all(&old, &c.folder_path) else {
            continue;
        };
        // 스냅샷은 이 연결의 스키마 자리에 있고 실제로 스냅샷으로 읽히는 것만 — 이름이 schema.json 이라고 다 아니다
        let snapshots: Vec<String> = c
            .schemas
            .iter()
            .map(|p| snapshot_path(c, &p.name))
            .collect();
        let read = |f: &str| old.read_file(f).unwrap_or_default();
        let mut generated = Vec::new();
        let mut own = false;
        for f in files {
            if snapshots.contains(&f) {
                if parse_snapshot_json(&read(&f)).is_some() {
                    generated.push(f);
                } else {
                    own = true;
                }
            } else if is_mermaid(&f) && is_generated_erd(&read(&f)) {
                generated.push(f);
                plan.erds += 1;
            } else {
                own = true;
            }
        }
        if generated.is_empty() {
            continue;
        }
        if own {
            plan.files.extend(generated);
        } else {
            plan.folders.push(c.folder_path.clone());
        }
    }
    (!plan.folders.is_empty() || !plan.files.is_empty()).then_some(plan)
}

/// 찾은 것을 휴지통으로 — 되돌릴 수 있다
pub fn trash_legacy_erds(db: &Db, plan: &LegacyPlan) -> Result<(), String> {
    let old = legacy(db);
    for p in plan.folders.iter().chain(&plan.files) {
        old.delete_entry(p)?;
    }
    Ok(())
}

/// 하네스가 재는 연산
pub fn call(db: &Db, op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Json::Null);
    let list = || serde_json::from_value::<Vec<DbConnection>>(arg(0)).map_err(|e| e.to_string());
    let out = match op {
        "erdMigrate.isGeneratedErd" => Ok(json!(is_generated_erd(arg(0).as_str().unwrap_or("")))),
        "erdMigrate.copyLegacySnapshots" => list()
            .and_then(|l| copy_legacy_snapshots(db, &l))
            .map(|_| Json::Null),
        "erdMigrate.findLegacyErds" => list().map(|l| json!(find_legacy_erds(db, &l))),
        "erdMigrate.trashLegacyErds" => serde_json::from_value::<LegacyPlan>(arg(0))
            .map_err(|e| e.to_string())
            .and_then(|p| trash_legacy_erds(db, &p))
            .map(|_| Json::Null),
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::is_generated_erd;

    #[test]
    fn generated_marker_reads_like_javascript() {
        assert!(is_generated_erd("%% amber:db a/b x"));
        assert!(is_generated_erd("  %% amber:db a b/c x"));
        assert!(!is_generated_erd("%% amber:db a/b"));
        assert!(
            !is_generated_erd("x\ny\nz\nw\nv\n%% amber:db a/b x"),
            "다섯 줄까지만"
        );
        // JS 의 `.` 는 \r 을 건너지 못한다
        assert!(!is_generated_erd("%% amber:db a\r/b x"));
        // JS 의 `\s` 는 U+FEFF 를 공백으로 친다
        assert!(is_generated_erd("\u{feff}%% amber:db a/b x"));
    }
}
