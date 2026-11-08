//! 노트 → 개념 올리기의 노트 쪽 역참조(src/lib/noteConcepts.ts). 노트 본문을 더럽히지 않게 같은 폴더의 사이드카
//! `<이름>.concepts.json` 에 목록으로 둔다. 개념(DB)이 정본이고 이건 "이 노트에서 무엇을 만들었나" 목록이다.

use serde_json::{Value as Json, json};

use crate::comments::stringify_1;
use crate::db::Db;
use crate::vault::{self, concepts_path_for, write_atomic};

type Result<T> = std::result::Result<T, String>;

fn full(db: &Db, note_rel: &str) -> String {
    vault::notes(db).full(&concepts_path_for(note_rel))
}

/// 노트에서 만든 개념 링크 — 없거나 깨졌으면 빈 목록. conceptId 가 숫자인 것만
pub fn load_note_concepts(db: &Db, note_rel: &str) -> Vec<Json> {
    let p = db.at(&full(db, note_rel));
    let Ok(bytes) = std::fs::read(&p) else {
        return Vec::new();
    };
    let Ok(d) = serde_json::from_str::<Json>(&String::from_utf8_lossy(&bytes)) else {
        return Vec::new();
    };
    d["links"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| l["conceptId"].is_number())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn save_links(db: &Db, note_rel: &str, links: &[Json]) -> Result<()> {
    let p = full(db, note_rel);
    if links.is_empty() {
        let abs = db.at(&p);
        if abs.exists() {
            std::fs::remove_file(abs).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    write_atomic(
        db,
        &p,
        &stringify_1(&json!({ "version": 1, "links": links })),
    )
}

fn id_of(l: &Json) -> Option<f64> {
    l["conceptId"].as_f64()
}

/// 올리기 링크를 더한다(같은 개념이면 갈아 끼운다 — 맨 뒤로)
pub fn add_note_concept(db: &Db, note_rel: &str, link: &Json) -> Result<()> {
    let id = id_of(link);
    let mut next: Vec<Json> = load_note_concepts(db, note_rel)
        .into_iter()
        .filter(|l| id_of(l) != id)
        .collect();
    next.push(link.clone());
    save_links(db, note_rel, &next)
}

/// 개념을 지우거나 링크를 풀 때 노트 쪽에서도 뺀다
pub fn remove_note_concept(db: &Db, note_rel: &str, concept_id: f64) -> Result<()> {
    let next: Vec<Json> = load_note_concepts(db, note_rel)
        .into_iter()
        .filter(|l| id_of(l) != Some(concept_id))
        .collect();
    save_links(db, note_rel, &next)
}

/// 하네스가 재는 연산
pub fn call(db: &Db, op: &str, args: &[Json]) -> Option<std::result::Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let out = match op {
        "noteConcepts.conceptsPathFor" => Ok(Json::from(concepts_path_for(s(0)))),
        "noteConcepts.loadNoteConcepts" => Ok(Json::from(load_note_concepts(db, s(0)))),
        "noteConcepts.addNoteConcept" => {
            add_note_concept(db, s(0), args.get(1).unwrap_or(&Json::Null)).map(|_| Json::Null)
        }
        "noteConcepts.removeNoteConcept" => remove_note_concept(
            db,
            s(0),
            args.get(1).and_then(Json::as_f64).unwrap_or(f64::NAN),
        )
        .map(|_| Json::Null),
        _ => return None,
    };
    Some(out)
}
