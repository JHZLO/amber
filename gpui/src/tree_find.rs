// lib/treeFind.ts 를 옮긴 것 — 파일 트리 안에서 찾기(⌘F)의 순수 로직. 본문 검색과 달리 트리의 목표는 접힌 폴더 안의
// 노트라 화면이 아니라 자료구조를 거른다: 맞은 것과 그 조상만 남기고, 조상 폴더를 임시로 펼친다.
// 대소문자 무시는 JS 의 toLowerCase, 위치는 UTF-16 단위(indexOf, slice)로 — 하네스가 TS 와 같은지 잰다.

use serde_json::{Value as Json, json};

use crate::vault::{Node, js_trim};

/// 걸러 낼 수 있는 트리 노드 — 화면의 vault::Node 와 하네스의 JSON 이 함께 쓴다
pub trait FindNode: Clone {
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn is_dir(&self) -> bool;
    fn kids(&self) -> Vec<Self>;
    fn with_kids(&self, kids: Vec<Self>) -> Self;
}

impl FindNode for Node {
    fn name(&self) -> &str {
        &self.name
    }
    fn path(&self) -> &str {
        &self.path
    }
    fn is_dir(&self) -> bool {
        self.is_dir
    }
    fn kids(&self) -> Vec<Self> {
        self.children.clone().unwrap_or_default()
    }
    fn with_kids(&self, kids: Vec<Self>) -> Self {
        Node {
            children: Some(kids),
            ..self.clone()
        }
    }
}

impl FindNode for Json {
    fn name(&self) -> &str {
        self["name"].as_str().unwrap_or_default()
    }
    fn path(&self) -> &str {
        self["path"].as_str().unwrap_or_default()
    }
    fn is_dir(&self) -> bool {
        self["isDir"].as_bool().unwrap_or(false)
    }
    fn kids(&self) -> Vec<Self> {
        self["children"].as_array().cloned().unwrap_or_default()
    }
    fn with_kids(&self, kids: Vec<Self>) -> Self {
        let mut v = self.clone();
        v["children"] = Json::Array(kids);
        v
    }
}

/// 맞은 것 — (경로, 폴더인가). 트리에 보이는 순서(전위)
pub type Match = (String, bool);

pub struct Found<T> {
    pub nodes: Vec<T>,
    pub matches: Vec<Match>,
    /// 임시로 펼칠 폴더 — 맞은 것의 조상(맞은 폴더 자신은 넣지 않는다), 넣은 순서
    pub expand: Vec<String>,
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn index_of(hay: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(hay.len()));
    }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| hay[i..].starts_with(needle))
}

/// 이름에 검색어가 들어 있나(대소문자 무시)
pub fn hits(name: &str, query: &str) -> bool {
    let q = utf16(&js_trim(query).to_lowercase());
    !q.is_empty() && index_of(&utf16(&name.to_lowercase()), &q, 0).is_some()
}

/// 검색어로 거른다. 폴더가 맞으면 그 아래는 통째로 남긴다(찾은 폴더를 열어 볼 수 있게)
pub fn filter_tree<T: FindNode>(nodes: &[T], query: &str) -> Found<T> {
    let q = js_trim(query);
    let mut found = Found {
        nodes: Vec::new(),
        matches: Vec::new(),
        expand: Vec::new(),
    };
    if q.is_empty() {
        found.nodes = nodes.to_vec();
        return found;
    }
    fn walk<T: FindNode>(list: &[T], ancestors: &[String], q: &str, f: &mut Found<T>) -> Vec<T> {
        let mut out = Vec::new();
        for n in list {
            let me = hits(n.name(), q);
            if me {
                f.matches.push((n.path().to_string(), n.is_dir()));
                for a in ancestors {
                    if !f.expand.contains(a) {
                        f.expand.push(a.clone());
                    }
                }
            }
            if !n.is_dir() {
                if me {
                    out.push(n.clone());
                }
                continue;
            }
            if me {
                // 맞은 폴더 자신이 차례에 든다 — 그 안까지 차례에 넣으면 접힌 행으로 이동하게 된다
                out.push(n.clone());
                continue;
            }
            let mut next = ancestors.to_vec();
            next.push(n.path().to_string());
            let kept = walk(&n.kids(), &next, q, f);
            if !kept.is_empty() {
                out.push(n.with_kids(kept));
            }
        }
        out
    }
    found.nodes = walk(nodes, &[], q, &mut found);
    found
}

/// 라벨을 맞은 조각 기준으로 쪼갠다 — 화면에서 그 글자만 칠하려고. 위치는 UTF-16 단위
pub fn split_hit(name: &str, query: &str) -> Vec<(String, bool)> {
    let q = utf16(&js_trim(query).to_lowercase());
    if q.is_empty() {
        return vec![(name.to_string(), false)];
    }
    let hay = utf16(&name.to_lowercase());
    let orig = utf16(name);
    let cut =
        |a: usize, b: usize| String::from_utf16_lossy(&orig[a.min(orig.len())..b.min(orig.len())]);
    let mut out = Vec::new();
    let mut at = 0;
    let mut i = index_of(&hay, &q, 0);
    while let Some(ix) = i {
        if ix > at {
            out.push((cut(at, ix), false));
        }
        out.push((cut(ix, ix + q.len()), true));
        at = ix + q.len();
        i = index_of(&hay, &q, at);
    }
    if at < orig.len() {
        out.push((cut(at, orig.len()), false));
    }
    out
}

/// 하네스가 재는 연산
pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let out = match op {
        "treeFind.hits" => json!(hits(s(0), s(1))),
        "treeFind.splitHit" => json!(
            split_hit(s(0), s(1))
                .into_iter()
                .map(|(text, hit)| json!({"text": text, "hit": hit}))
                .collect::<Vec<_>>()
        ),
        "treeFind.filterTree" => {
            let nodes = args
                .first()
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let f = filter_tree(&nodes, s(1));
            json!({
                "nodes": f.nodes,
                "matches": f.matches.into_iter().map(|(path, is_dir)| json!({"path": path, "isDir": is_dir})).collect::<Vec<_>>(),
                "expand": f.expand,
            })
        }
        _ => return None,
    };
    Some(Ok(out))
}
