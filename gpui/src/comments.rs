// lib/comments.ts 를 옮긴 것 — 노트 인라인 질문(노션 댓글식)의 저장 계층. 노트 본문(.md)을 건드리지 않게 같은 폴더의
// 사이드카 JSON 에 둔다: <이름>.md ↔ <이름>.comments.json. 이름 바꾸기/지우기 때 동행은 vault 의 노트 함수가 한다.
// "파일 없음"과 "못 읽었다"를 구분하는 게 핵심이다 — 둘을 빈 목록으로 뭉개면 손상된 사이드카에 질문 하나를 더하는 순간
// 통째로 덮여 기존 문답이 사라진다(원자적 쓰기라 .bak 도 없다). 쓰기 전엔 반드시 read_comments 를 거친다.

use serde_json::{Value as Json, json};

use crate::db::Db;
use crate::vault::{self, comments_path_for, write_atomic};

type Result<T> = std::result::Result<T, String>;

fn full(db: &Db, note_rel: &str) -> String {
    vault::notes(db).full(&comments_path_for(note_rel))
}

/// 질문 목록. Ok(None) = 못 읽었다(깨진 JSON, comments 가 배열이 아님). 파일이 없으면 빈 목록
pub fn read_comments(db: &Db, note_rel: &str) -> Option<Vec<Json>> {
    let p = db.at(&full(db, note_rel));
    if !p.exists() {
        return Some(Vec::new());
    }
    let text = String::from_utf8_lossy(&std::fs::read(&p).ok()?).into_owned();
    let d: Json = serde_json::from_str(&text).ok()?;
    let list = d.get("comments")?.as_array()?;
    let mut out = Vec::new();
    for c in list {
        // 손으로 고쳤을 수 있는 사이드카 — 앵커와 질문이 글자인 것만
        if !(c.is_object() && c["anchor"].is_string() && c["question"].is_string()) {
            continue;
        }
        let mut c = c.clone();
        let obj = c.as_object_mut().expect("객체");
        match obj.get("followUps") {
            // undefined 는 그대로(키가 없다)
            None => {}
            Some(Json::Array(turns)) => {
                let kept: Vec<Json> = turns
                    .iter()
                    .filter(|t| {
                        t.is_object() && t["question"].is_string() && t["answer"].is_string()
                    })
                    .cloned()
                    .collect();
                obj.insert("followUps".into(), Json::Array(kept));
            }
            Some(_) => {
                obj.remove("followUps");
            }
        }
        out.push(c);
    }
    Some(out)
}

/// 표시용 — 못 읽으면 빈 목록(읽기만 하는 화면에서만)
pub fn load_comments(db: &Db, note_rel: &str) -> Vec<Json> {
    read_comments(db, note_rel).unwrap_or_default()
}

/// 디스크를 정본으로 읽어 고친 뒤 저장한다 — 읽기가 실패하면 아무것도 쓰지 않고 실패를 돌려준다
pub fn mutate_comments(
    db: &Db,
    note_rel: &str,
    edit: impl FnOnce(Vec<Json>) -> Vec<Json>,
) -> Result<Vec<Json>> {
    let Some(list) = read_comments(db, note_rel) else {
        return Err(crate::i18n::t("notes.cmt.sidecarBroken"));
    };
    let next = edit(list);
    save_comments(db, note_rel, &next)?;
    Ok(next)
}

/// JSON.stringify(v, null, 1) — 한 칸 들여쓰기
fn stringify_1(v: &Json) -> String {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(v, &mut ser).expect("JSON");
    String::from_utf8(buf).expect("UTF-8")
}

/// 저장. 목록이 비면 사이드카 자체를 지워 잔여물을 남기지 않는다
pub fn save_comments(db: &Db, note_rel: &str, comments: &[Json]) -> Result<()> {
    let p = full(db, note_rel);
    if comments.is_empty() {
        let abs = db.at(&p);
        if abs.exists() {
            std::fs::remove_file(abs).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    write_atomic(
        db,
        &p,
        &stringify_1(&json!({ "version": 1, "comments": comments })),
    )
}

/// crypto.randomUUID 꼴(v4)
pub fn new_comment_id() -> String {
    let n = ulid::Ulid::new().0.to_be_bytes();
    let mut b = n;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// noteAnchor.ts 의 nthIndex — fullText 에서 anchor 의 occurrence 번째 위치(UTF-16 단위, 없으면 -1)
pub fn nth_index(full_text: &str, anchor: &str, occurrence: usize) -> i64 {
    if anchor.is_empty() {
        return -1;
    }
    let hay: Vec<u16> = full_text.encode_utf16().collect();
    let needle: Vec<u16> = anchor.encode_utf16().collect();
    let mut idx: i64 = -1;
    for _ in 0..=occurrence {
        let from = (idx + 1) as usize;
        match (from..=hay.len().saturating_sub(needle.len()))
            .find(|&i| from <= hay.len() && hay[i..].starts_with(&needle))
        {
            Some(i) => idx = i as i64,
            None => return -1,
        }
    }
    idx
}

/// 하네스가 재는 연산
pub fn call(db: &Db, op: &str, args: &[Json]) -> Option<Result<Json>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    Some(match op {
        "comments.commentsPathFor" => Ok(json!(comments_path_for(s(0)))),
        "comments.readComments" => Ok(match read_comments(db, s(0)) {
            Some(c) => json!({ "ok": true, "comments": c }),
            None => json!({ "ok": false }),
        }),
        "comments.loadComments" => Ok(json!(load_comments(db, s(0)))),
        "comments.mutateComments" => {
            let list = args
                .get(1)
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            mutate_comments(db, s(0), |_| list).map(|v| json!(v))
        }
        "comments.saveComments" => {
            let list = args
                .get(1)
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            save_comments(db, s(0), &list).map(|_| Json::Null)
        }
        "noteAnchor.nthIndex" => Ok(json!(nth_index(
            s(0),
            s(1),
            args.get(2).and_then(Json::as_u64).unwrap_or(0) as usize
        ))),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{nth_index, stringify_1};

    #[test]
    fn stringify_with_one_space() {
        let v = serde_json::json!({"version": 1, "comments": [{"a": "x"}]});
        assert_eq!(
            stringify_1(&v),
            "{\n \"version\": 1,\n \"comments\": [\n  {\n   \"a\": \"x\"\n  }\n ]\n}"
        );
    }

    #[test]
    fn nth_like_note_anchor() {
        assert_eq!(nth_index("ab ab ab", "ab", 0), 0);
        assert_eq!(nth_index("ab ab ab", "ab", 2), 6);
        assert_eq!(nth_index("ab ab ab", "ab", 3), -1);
        assert_eq!(nth_index("aaa", "aa", 1), 1, "겹쳐서도 센다(idx + 1 부터)");
        assert_eq!(nth_index("x", "", 0), -1);
    }
}
