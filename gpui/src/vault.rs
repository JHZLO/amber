// vault 파일 계층: src/lib/vaultTree.ts(트리 공용 구현), notes.ts(사이드카, 그림, 개념 출처를 따라 옮기기),
// diagrams.ts, erdStore.ts, vault.ts(개념 상세 노트), noteAssets.ts 의 copyAssetsForMove 를 옮긴 것.
// 파일이 정본인 앱에서 가장 파괴적인 코드라 동등성 하네스가 파일 상태까지 견준다(parity/fs.ts).
//
// 경로는 지금 앱처럼 앱 데이터 폴더 기준 상대경로('vault/notes/…') 또는 절대경로다.
// 문자열은 JS 와 같은 규칙으로 다룬다: 공백 = JS 의 trim 이 지우는 글자, 길이와 위치 = UTF-16 단위.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use icu_collator::options::CollatorOptions;
use icu_collator::{Collator, CollatorBorrowed};
use regex::Regex;
use serde::Serialize;

use crate::concepts::repoint_concept_source;
use crate::db::Db;

type Result<T> = std::result::Result<T, String>;

pub struct TreeConfig {
    /// 기본 보관함 — 앱 데이터 폴더 기준 상대경로
    pub root: &'static str,
    /// 사용자가 "폴더 열기"로 바꾼 작업 폴더(절대경로)가 든 설정 키 — 지금 앱 workspace.ts 의 getRoot
    pub root_pref: Option<&'static str>,
    /// 트리에 보일 확장자(소문자, 점 포함). 첫 항목이 새 파일의 확장자
    pub exts: &'static [&'static str],
    pub template: fn(&str) -> String,
    /// 트리에 안 보일 폴더 이름(노트 옆 그림 폴더 등)
    pub hidden_dirs: &'static [&'static str],
}

pub const ASSET_DIR: &str = "_assets";

pub const NOTES: TreeConfig = TreeConfig {
    root: "vault/notes",
    root_pref: Some("amber.root.notes"),
    exts: &[".md"],
    template: |title| format!("# {title}\n\n"),
    hidden_dirs: &[ASSET_DIR],
};

pub const DIAGRAMS: TreeConfig = TreeConfig {
    root: "vault/diagrams",
    root_pref: Some("amber.root.diagrams"),
    exts: &[".mmd", ".mermaid"],
    template: |_| {
        "erDiagram
    USER ||--o{ ORDER : places
    ORDER ||--|{ ORDER_ITEM : contains

    USER {
        bigint id PK
        varchar email
    }
    ORDER {
        bigint id PK
        bigint user_id FK
    }
"
        .to_string()
    },
    hidden_dirs: &[],
};

pub const ERD: TreeConfig = TreeConfig {
    root: "vault/erd",
    root_pref: None,
    exts: &[".json"],
    template: |_| String::new(),
    hidden_dirs: &[],
};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Match {
    pub name: String,
    pub path: String,
    pub snippet: Option<String>,
}

// ---- JS 와 같은 문자열 규칙 ----

/// String.prototype.trim 이 지우는 글자 — Rust 의 공백과 둘이 다르다(U+0085 는 안 지우고 U+FEFF 는 지운다)
fn js_ws(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(js_ws)
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// localeCompare(…, "ko") — ICU 한국어 정렬
fn ko_compare(a: &str, b: &str) -> Ordering {
    static KO: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    KO.get_or_init(|| {
        let ko: icu_locale_core::Locale = "ko".parse().expect("ko");
        Collator::try_new((&ko).into(), CollatorOptions::default()).expect("ko collation")
    })
    .compare(a, b)
}

// ---- 순수 계산(vaultTree.ts) ----

/// 상위 폴더의 상대경로 ('' = 루트)
pub fn parent_of(rel: &str) -> &str {
    rel.rfind('/').map_or("", |i| &rel[..i])
}

/// 경로 입력 정규화: 구간 trim, 빈 구간(연속/양끝 슬래시) 제거
pub fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(js_trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

// ---- 파일 바탕 ----

/// 임시 파일에 쓰고 rename 으로 덮는다 — 저장 도중 끊겨도 옛 파일 아니면 새 파일이 온전히 남는다
pub fn write_atomic(db: &Db, path: &str, content: &str) -> Result<()> {
    let target = db.at(path);
    let tmp = db.at(&format!("{path}.amber-tmp"));
    let res = std::fs::write(&tmp, content).and_then(|_| std::fs::rename(&tmp, &target));
    if let Err(e) = res {
        if tmp.exists() {
            let _ = std::fs::remove_file(&tmp);
        }
        return Err(e.to_string());
    }
    Ok(())
}

fn read_text(path: &Path) -> Result<String> {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| e.to_string())
}

fn mkdir_all(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| e.to_string())
}

// ---- 트리(createVaultTree) ----

pub struct Tree<'a> {
    pub db: &'a Db,
    pub cfg: &'a TreeConfig,
    root: String,
}

impl<'a> Tree<'a> {
    pub fn new(db: &'a Db, cfg: &'a TreeConfig) -> Self {
        let root = cfg
            .root_pref
            .and_then(|k| crate::prefs::get(db, k))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| cfg.root.to_string());
        Tree { db, cfg, root }
    }

    fn full(&self, rel: &str) -> String {
        if rel.is_empty() {
            self.root.clone()
        } else {
            format!("{}/{rel}", self.root)
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.db.at(&self.full(rel))
    }

    fn main_ext(&self) -> &'static str {
        self.cfg.exts[0]
    }

    fn ext_of(&self, name: &str) -> Option<&'static str> {
        self.cfg.exts.iter().copied().find(|e| {
            name.len() >= e.len()
                && name.is_char_boundary(name.len() - e.len())
                && name[name.len() - e.len()..].eq_ignore_ascii_case(e)
        })
    }

    /// 전체 트리 (폴더 먼저, 이름순). 기본 보관함이면 루트 폴더가 없을 때 만든다
    pub fn list_tree(&self) -> Result<Vec<Node>> {
        if !Path::new(&self.root).is_absolute() {
            mkdir_all(&self.path(""))?;
        }
        self.read_tree("")
    }

    fn read_tree(&self, rel: &str) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        for entry in std::fs::read_dir(self.path(rel)).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if name.starts_with('.') {
                continue;
            }
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if kind.is_dir() {
                if self.cfg.hidden_dirs.contains(&name.as_str()) {
                    continue;
                }
                nodes.push(Node {
                    name,
                    children: Some(self.read_tree(&child)?),
                    path: child,
                    is_dir: true,
                });
            } else if kind.is_file()
                && let Some(ext) = self.ext_of(&name)
            {
                nodes.push(Node {
                    name: name[..name.len() - ext.len()].to_string(),
                    path: child,
                    is_dir: false,
                    children: None,
                });
            }
        }
        nodes.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| ko_compare(&a.name, &b.name))
        });
        Ok(nodes)
    }

    /// 고친 시각(ms) — 읽기 화면 아래 '수정 N분 전'
    pub fn mtime(&self, rel: &str) -> Option<i64> {
        let m = std::fs::metadata(self.path(rel)).ok()?.modified().ok()?;
        Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
    }

    pub fn read_file(&self, rel: &str) -> Result<String> {
        read_text(&self.path(rel))
    }

    /// 있으면 본문, 없으면 None — '없을 수 있는' 파일(스냅샷 등)용
    pub fn read_file_if_exists(&self, rel: &str) -> Result<Option<String>> {
        let p = self.path(rel);
        if !p.exists() {
            return Ok(None);
        }
        read_text(&p).map(Some)
    }

    pub fn file_exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    /// 폴더를 보장한다(있으면 그대로, 없으면 중간 폴더까지)
    pub fn ensure_dir(&self, rel: &str) -> Result<()> {
        let rel = normalize_path(rel);
        if rel.is_empty() || self.path(&rel).exists() {
            return Ok(());
        }
        mkdir_all(&self.path(&rel))
    }

    pub fn write_file(&self, rel: &str, content: &str) -> Result<()> {
        write_atomic(self.db, &self.full(rel), content)
    }

    /// 새 폴더. 다단계 입력이면 중간 폴더까지. 만든 상대경로
    pub fn create_folder(&self, parent: &str, name_or_path: &str) -> Result<String> {
        let sub = normalize_path(name_or_path);
        let rel = if parent.is_empty() {
            sub
        } else {
            format!("{parent}/{sub}")
        };
        if self.path(&rel).exists() {
            return Err("common.file.dupName".into());
        }
        mkdir_all(&self.path(&rel))?;
        Ok(rel)
    }

    /// 새 파일. 경로 입력이면 중간 폴더까지. 만든 상대경로
    pub fn create_file(&self, parent: &str, name_or_path: &str) -> Result<String> {
        let sub = normalize_path(name_or_path);
        let prefix = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        let rel = format!("{prefix}{sub}{}", self.main_ext());
        if self.path(&rel).exists() {
            return Err("common.file.dupFile".into());
        }
        mkdir_all(&self.path(parent_of(&rel)))?;
        let title = sub.rsplit('/').next().unwrap_or_default();
        self.write_file(&rel, &(self.cfg.template)(title))?;
        Ok(rel)
    }

    /// 같은 폴더 안에서 이름 바꾸기. 새 상대경로
    pub fn rename_entry(&self, rel: &str, new_name: &str, is_dir: bool) -> Result<String> {
        let parent = parent_of(rel);
        let name = if is_dir {
            new_name.to_string()
        } else {
            format!("{new_name}{}", self.main_ext())
        };
        let new_rel = if parent.is_empty() {
            name
        } else {
            format!("{parent}/{name}")
        };
        if new_rel == rel {
            return Ok(new_rel);
        }
        // macOS 는 대소문자를 안 가린다 — 대소문자만 바꾸면 자기 자신이 '이미 있는 이름'으로 잡힌다
        let same_entry = new_rel.to_lowercase() == rel.to_lowercase();
        if !same_entry && self.path(&new_rel).exists() {
            return Err("common.file.dupName".into());
        }
        rename(&self.path(rel), &self.path(&new_rel))?;
        Ok(new_rel)
    }

    /// 다른 폴더로 옮기기('' = 루트). 같은 폴더면 그대로, 자기 아래로는 못 옮긴다. 새 상대경로
    pub fn move_entry(&self, rel: &str, target_dir: &str) -> Result<String> {
        let dir = normalize_path(target_dir);
        if dir == parent_of(rel) {
            return Ok(rel.to_string());
        }
        if dir == rel || dir.starts_with(&format!("{rel}/")) {
            return Err("common.folder.intoSelf".into());
        }
        let name = rel.rsplit('/').next().unwrap_or_default();
        let new_rel = if dir.is_empty() {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        };
        if self.path(&new_rel).exists() {
            return Err("common.folder.dupTarget".into());
        }
        rename(&self.path(rel), &self.path(&new_rel))?;
        Ok(new_rel)
    }

    /// 휴지통으로(폴더면 통째로). 빈 경로는 루트 자체라 막는다 — 호출부 버그 하나가 노트 전체를 날릴 수 있다
    pub fn delete_entry(&self, rel: &str) -> Result<()> {
        let rel = js_trim(rel);
        if matches!(rel, "" | "/" | "." | "..") {
            return Err("common.file.badTarget".into());
        }
        self.db.trash(&self.full(rel))
    }

    /// 파일 이름 + 본문 검색(대소문자 무시, searchFiles). 이름 일치를 앞에 둔다
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Match>> {
        let q = js_trim(query).to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        fn flat(nodes: Vec<Node>, out: &mut Vec<Node>) {
            for n in nodes {
                match n.children {
                    Some(kids) if n.is_dir => flat(kids, out),
                    _ if n.is_dir => {}
                    _ => out.push(n),
                }
            }
        }
        let mut files = Vec::new();
        flat(self.list_tree()?, &mut files);
        let snippet = |f: &Node| {
            self.read_file(&f.path)
                .ok()
                .and_then(|b| first_hit_line(&b, &q))
        };

        let (by_name_files, rest): (Vec<Node>, Vec<Node>) = files
            .into_iter()
            .partition(|f| f.name.to_lowercase().contains(&q));
        let mut out: Vec<Match> = by_name_files
            .iter()
            .take(limit)
            .map(|f| Match {
                name: f.name.clone(),
                path: f.path.clone(),
                snippet: snippet(f),
            })
            .collect();
        if out.len() >= limit {
            out.truncate(limit);
            return Ok(out);
        }
        // 지금 앱처럼 여덟 개씩 읽고, 자리가 차면 다음 묶음은 안 읽는다
        for batch in rest.chunks(8) {
            if out.len() >= limit {
                break;
            }
            for f in batch {
                if let Some(s) = snippet(f) {
                    out.push(Match {
                        name: f.name.clone(),
                        path: f.path.clone(),
                        snippet: Some(s),
                    });
                }
            }
        }
        out.truncate(limit);
        Ok(out)
    }
}

const SNIPPET_MAX: usize = 120;

/// 소문자 질의가 처음 걸린 줄. 길면 매칭 지점이 잘리지 않게 그 앞에서 자른다(위치와 길이는 UTF-16 단위)
fn first_hit_line(body: &str, q: &str) -> Option<String> {
    let q16 = utf16(q);
    for raw in body.split('\n') {
        let line = js_trim(raw);
        let low = utf16(&line.to_lowercase());
        let Some(at) = low.windows(q16.len()).position(|w| w == q16.as_slice()) else {
            continue;
        };
        let line16 = utf16(line);
        if line16.len() <= SNIPPET_MAX {
            return Some(line.to_string());
        }
        let start = at.saturating_sub(30).min(line16.len());
        let end = (start + SNIPPET_MAX).min(line16.len());
        let lead = if start > 0 { "…" } else { "" };
        return Some(format!(
            "{lead}{}…",
            String::from_utf16_lossy(&line16[start..end])
        ));
    }
    None
}

// ---- 노트(notes.ts): 사이드카와 그림, 개념 출처가 노트를 따라간다 ----

fn strip_md(rel: &str) -> &str {
    if rel.len() >= 3
        && rel.is_char_boundary(rel.len() - 3)
        && rel[rel.len() - 3..].eq_ignore_ascii_case(".md")
    {
        &rel[..rel.len() - 3]
    } else {
        rel
    }
}

fn is_md(rel: &str) -> bool {
    strip_md(rel).len() != rel.len()
}

pub fn comments_path_for(rel: &str) -> String {
    format!("{}.comments.json", strip_md(rel))
}

pub fn concepts_path_for(rel: &str) -> String {
    format!("{}.concepts.json", strip_md(rel))
}

pub fn notes(db: &Db) -> Tree<'_> {
    Tree::new(db, &NOTES)
}

/// 노트가 들어 있는 폴더의 경로(작업 폴더 포함) — 그림 저장/해석의 기준
pub fn note_dir_path(tree: &Tree, rel: &str) -> String {
    tree.full(parent_of(rel))
}

fn move_sidecars(tree: &Tree, old_rel: &str, new_rel: &str) -> Result<()> {
    for (old, new) in [
        (comments_path_for(old_rel), comments_path_for(new_rel)),
        (concepts_path_for(old_rel), concepts_path_for(new_rel)),
    ] {
        if tree.path(&old).exists() {
            rename(&tree.path(&old), &tree.path(&new))?;
        }
    }
    Ok(())
}

pub fn rename_note_entry(db: &Db, rel: &str, new_name: &str, is_dir: bool) -> Result<String> {
    let tree = notes(db);
    let new_rel = tree.rename_entry(rel, new_name, is_dir)?;
    let _ = repoint_concept_source(db, rel, &new_rel, is_dir);
    if !is_dir {
        move_sidecars(&tree, rel, &new_rel)?;
    }
    Ok(new_rel)
}

pub fn move_note_entry(db: &Db, rel: &str, target_dir: &str) -> Result<String> {
    let tree = notes(db);
    let new_rel = tree.move_entry(rel, target_dir)?;
    let _ = repoint_concept_source(db, rel, &new_rel, !is_md(rel));
    if is_md(rel) {
        // 그림 복사 실패가 이동을 실패로 만들지 않는다
        if let Ok(md) = tree.read_file(&new_rel) {
            let _ = copy_assets_for_move(
                db,
                &md,
                &note_dir_path(&tree, rel),
                &note_dir_path(&tree, &new_rel),
            );
        }
        move_sidecars(&tree, rel, &new_rel)?;
    }
    Ok(new_rel)
}

pub fn delete_note_entry(db: &Db, rel: &str) -> Result<()> {
    let tree = notes(db);
    tree.delete_entry(rel)?;
    if is_md(rel) {
        for sc in [comments_path_for(rel), concepts_path_for(rel)] {
            db.trash(&tree.full(&sc))?;
        }
    }
    Ok(())
}

/// 본문이 같은 폴더의 `_assets` 에서 가리키는 파일 이름들(중복 제거, 하위 경로와 .. 는 뺀다)
pub fn asset_refs(md: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"!\[[^\]]*\]\(\s*<?(?:\./)?_assets/([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)"#).unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(md) {
        let name = &c[1];
        if name.contains('/')
            || name.contains('\\')
            || name.starts_with('.')
            || out.iter().any(|n| n == name)
        {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// 노트를 다른 폴더로 옮길 때 본문이 가리키는 그림을 새 폴더의 `_assets/` 로 **복사**한다 —
/// 같은 폴더의 다른 노트가 같은 파일을 가리킬 수 있어서 옮기지 않는다
pub fn copy_assets_for_move(db: &Db, md: &str, from_dir: &str, to_dir: &str) -> Result<()> {
    let names = asset_refs(md);
    if names.is_empty() || from_dir == to_dir {
        return Ok(());
    }
    let (src, dst) = (
        format!("{from_dir}/{ASSET_DIR}"),
        format!("{to_dir}/{ASSET_DIR}"),
    );
    let mut made = false;
    for name in names {
        let (from, to) = (
            db.at(&format!("{src}/{name}")),
            db.at(&format!("{dst}/{name}")),
        );
        if !from.exists() || to.exists() {
            continue;
        }
        if !made {
            mkdir_all(&db.at(&dst))?;
            made = true;
        }
        std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---- 개념 상세 노트(vault.ts) ----

pub fn detail_path_for(ulid: &str) -> String {
    format!("concepts/{ulid}/index.md")
}

/// 상세 노트 저장(없으면 폴더도). 상대경로
pub fn write_concept_note(db: &Db, ulid: &str, body: &str) -> Result<String> {
    mkdir_all(&db.at(&format!("vault/concepts/{ulid}")))?;
    let detail = detail_path_for(ulid);
    let body = body.trim_start_matches(js_ws);
    write_atomic(
        db,
        &format!("vault/{detail}"),
        &format!("---\nid: {ulid}\n---\n\n{body}"),
    )?;
    Ok(detail)
}

/// 상세 노트 본문(frontmatter 를 뗀다)
pub fn read_concept_note(db: &Db, detail: &str) -> Result<String> {
    let raw = read_text(&db.at(&format!("vault/{detail}")))?;
    Ok(strip_frontmatter(&raw).to_string())
}

fn strip_frontmatter(raw: &str) -> &str {
    if !raw.starts_with("---") {
        return raw;
    }
    let Some(end) = raw[3..].find("\n---").map(|i| i + 3) else {
        return raw;
    };
    match raw[end + 1..].find('\n') {
        None => "",
        Some(i) => raw[end + 1 + i + 1..].trim_start_matches('\n'),
    }
}

pub fn concept_note_exists(db: &Db, detail: &str) -> bool {
    db.at(&format!("vault/{detail}")).exists()
}

/// 개념 폴더 통째로 휴지통으로(그림 포함)
pub fn delete_concept_dir(db: &Db, ulid: &str) -> Result<()> {
    let dir = format!("vault/concepts/{ulid}");
    if db.at(&dir).exists() {
        db.trash(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ko_order_matches_node_icu() {
        // node: [...names].sort((a, b) => a.localeCompare(b, "ko"))
        let mut names = vec![
            "가나", "Apple", "apple", "10", "2", "_x", "Zeta", "한글", "漢字", "éclair", "eclair",
            "ㄱ", "a b", "a-b", "README", "readme", "Ab", "aB", "😀", " space",
        ];
        names.sort_by(|a, b| ko_compare(a, b));
        assert_eq!(
            names,
            [
                " space", "_x", "😀", "10", "2", "ㄱ", "가나", "한글", "漢字", "a b", "a-b", "aB",
                "Ab", "apple", "Apple", "eclair", "éclair", "readme", "README", "Zeta"
            ]
        );
    }

    #[test]
    fn snippet_cuts_around_the_hit_in_utf16_units() {
        let line = format!("{}needle{}", "가".repeat(150), "나".repeat(150));
        let hit = first_hit_line(&line, "needle").unwrap();
        assert!(hit.starts_with('…') && hit.ends_with('…'));
        assert_eq!(hit.encode_utf16().count(), SNIPPET_MAX + 2);
        assert_eq!(
            first_hit_line("  짧은 줄 Needle  ", "needle").as_deref(),
            Some("짧은 줄 Needle")
        );
    }

    #[test]
    fn frontmatter_variants() {
        assert_eq!(strip_frontmatter("---\nid: X\n---\n\n\n본문"), "본문");
        assert_eq!(strip_frontmatter("본문만"), "본문만");
        assert_eq!(
            strip_frontmatter("---\nid: X\n끝이 없다"),
            "---\nid: X\n끝이 없다"
        );
        assert_eq!(strip_frontmatter("---\nid: E\n---"), "");
    }

    #[test]
    fn asset_refs_like_note_assets() {
        let md = "![a](_assets/a.png) ![](./_assets/b.png) ![](<_assets/c.png> \"t\") ![](_assets/a.png) ![](_assets/d/e.png) ![](_assets/.h.png)";
        assert_eq!(asset_refs(md), ["a.png", "b.png", "c.png"]);
    }
}
