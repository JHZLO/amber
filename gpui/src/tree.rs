// src/lib/todoTree.ts 를 옮긴 것 — 할 일 트리 계산(순수). 목록 그리기, 진행률, 끌어 놓기 해석이 여기서 정해진다.
// 입력은 DB 행(Row)이든 하네스의 JSON 이든 id, parent_id 만 있으면 된다(Node). 돌려주는 행은 받은 것 그대로다.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::db::Row;

pub trait Node: Clone {
    fn field(&self, key: &str) -> Option<&Json>;

    fn id(&self) -> i64 {
        self.field("id").and_then(Json::as_i64).unwrap_or_default()
    }
    /// JS 의 `parent_id ?? null`
    fn parent(&self) -> Option<i64> {
        self.field("parent_id").and_then(Json::as_i64)
    }
    fn is(&self, key: &str, v: i64) -> bool {
        self.field(key).and_then(Json::as_i64) == Some(v)
    }
}

impl Node for Row {
    fn field(&self, key: &str) -> Option<&Json> {
        self.get(key)
    }
}

impl Node for Json {
    fn field(&self, key: &str) -> Option<&Json> {
        self.get(key)
    }
}

/// 렌더 순서대로 펼친 한 행
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TreeRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub depth: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DropSlot {
    pub idx: usize,
    pub depth: i64,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DropResolution {
    pub new_parent_id: Option<i64>,
    pub new_sort_order: usize,
    pub ordered_sibling_ids: Vec<i64>,
}

/// 형제 그룹 — 입력 순서(sort_order, id) 그대로
pub fn children_of<T: Node>(nodes: &[T], parent: Option<i64>) -> Vec<T> {
    nodes
        .iter()
        .filter(|n| n.parent() == parent)
        .cloned()
        .collect()
}

/// 조상 경로(바깥쪽부터). 고리가 있어도 멈춘다
pub fn ancestor_path<T: Node>(rows: &[T], parent: Option<i64>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = parent;
    while let Some(id) = cur {
        if !seen.insert(id) {
            break;
        }
        let Some(row) = rows.iter().find(|r| r.id() == id) else {
            break;
        };
        out.insert(
            0,
            row.field("content")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        cur = row.parent();
    }
    out
}

/// 루트에서 펼친 순서(깊이 포함)
pub fn flatten_tree<T: Node>(nodes: &[T]) -> Vec<TreeRow> {
    fn walk<T: Node>(nodes: &[T], parent: Option<i64>, depth: i64, out: &mut Vec<TreeRow>) {
        for n in nodes.iter().filter(|n| n.parent() == parent) {
            out.push(TreeRow {
                id: n.id(),
                parent_id: n.parent(),
                depth,
            });
            walk(nodes, Some(n.id()), depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, None, 0, &mut out);
    out
}

/// 이 목록 안에서 최상위로 그릴 행 — 부모가 목록에 없으면 루트다
pub fn visible_roots<T: Node>(nodes: &[T]) -> Vec<T> {
    let present: HashSet<i64> = nodes.iter().map(Node::id).collect();
    nodes
        .iter()
        .filter(|n| n.parent().is_none_or(|p| !present.contains(&p)))
        .cloned()
        .collect()
}

/// 부분 집합용 펼치기 — 모든 행이 정확히 한 번 나온다(순환이 있어도)
pub fn flatten_subset<T: Node>(nodes: &[T]) -> Vec<TreeRow> {
    fn walk<T: Node>(
        nodes: &[T],
        node: &T,
        depth: i64,
        seen: &mut HashSet<i64>,
        out: &mut Vec<TreeRow>,
    ) {
        if !seen.insert(node.id()) {
            return;
        }
        out.push(TreeRow {
            id: node.id(),
            parent_id: node.parent(),
            depth,
        });
        for c in nodes.iter().filter(|c| c.parent() == Some(node.id())) {
            walk(nodes, c, depth + 1, seen, out);
        }
    }
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    for n in &visible_roots(nodes) {
        walk(nodes, n, 0, &mut seen, &mut out);
    }
    // 순환 뭉치는 위에서 루트를 못 찾는다 — 남은 행을 최상위로
    for n in nodes {
        walk(nodes, n, 0, &mut seen, &mut out);
    }
    out
}

/// root 와 그 모든 자손 — 찾은 순서대로
pub fn subtree_ids<T: Node>(nodes: &[T], root: i64) -> Vec<i64> {
    let mut ids = vec![root];
    let mut set = HashSet::from([root]);
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        for n in nodes {
            if n.parent() != Some(pid) || set.contains(&n.id()) {
                continue;
            }
            set.insert(n.id());
            ids.push(n.id());
            stack.push(n.id());
        }
    }
    ids
}

pub fn descendant_count<T: Node>(nodes: &[T], root: i64) -> usize {
    subtree_ids(nodes, root).len() - 1
}

/// 유효 깊이: 아래 행의 깊이 이상, 위 행의 깊이 + 1 이하
pub fn clamp_drop_depth(desired: i64, above: Option<&TreeRow>, below: Option<&TreeRow>) -> i64 {
    let max = above.map_or(0, |a| a.depth + 1);
    let min = below.map_or(0, |b| b.depth);
    min.max(max.min(desired))
}

/// 슬롯 → 새 부모 + 새 형제 순서. 제자리거나 규칙상 안 되는 슬롯이면 None
pub fn resolve_drop<T: Node>(
    nodes: &[T],
    candidates: &[TreeRow],
    dragged_id: i64,
    slot: DropSlot,
    invalid_parents: Option<&HashSet<i64>>,
) -> Option<DropResolution> {
    let dragged = nodes.iter().find(|n| n.id() == dragged_id)?;
    let DropSlot { idx, depth } = slot;
    let mut new_parent = None;
    if depth > 0 {
        for i in (0..idx.min(candidates.len())).rev() {
            if candidates[i].depth == depth - 1 {
                new_parent = Some(candidates[i].id);
                break;
            }
            if candidates[i].depth < depth - 1 {
                break;
            }
        }
        let p = new_parent?;
        if invalid_parents.is_some_and(|s| s.contains(&p)) {
            return None;
        }
    }
    let new_sort_order = candidates
        .iter()
        .take(idx)
        .filter(|r| r.parent_id == new_parent)
        .count();
    let mut ordered: Vec<i64> = children_of(nodes, new_parent)
        .iter()
        .map(Node::id)
        .filter(|id| *id != dragged_id)
        .collect();
    ordered.insert(new_sort_order.min(ordered.len()), dragged_id);
    if new_parent == dragged.parent() {
        let old: Vec<i64> = children_of(nodes, new_parent)
            .iter()
            .map(Node::id)
            .collect();
        if old == ordered {
            return None;
        }
    }
    Some(DropResolution {
        new_parent_id: new_parent,
        new_sort_order,
        ordered_sibling_ids: ordered,
    })
}

/// 하루의 진행률 — 달력 칸과 목록 아래 요약이 같은 규칙으로 센다.
/// 넘긴 줄(고스트)은 넣고, 지워진 기록(gone)과 묶음 머리글(자식이 있는 최상위 줄)은 뺀다
pub fn day_progress<T: Node>(rows: &[T]) -> (usize, usize) {
    let live: Vec<T> = rows.iter().filter(|r| !r.is("gone", 1)).cloned().collect();
    let with_kids: HashSet<i64> = live.iter().filter_map(Node::parent).collect();
    let roots: HashSet<i64> = visible_roots(&live).iter().map(Node::id).collect();
    let (mut done, mut total) = (0, 0);
    for r in &live {
        if roots.contains(&r.id()) && with_kids.contains(&r.id()) {
            continue;
        }
        total += 1;
        if r.is("done", 1) {
            done += 1;
        }
    }
    (done, total)
}

/// 하네스가 부르는 이름 → 결과
pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let nodes = || -> Vec<Json> {
        args.first()
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let int = |i: usize| args.get(i).and_then(Json::as_i64);
    let row = |i: usize| -> Option<TreeRow> { serde_json::from_value(args.get(i)?.clone()).ok() };
    let ok = |v: Json| Some(Ok(v));
    match op {
        "todoTree.childrenOf" => ok(json!(children_of(&nodes(), int(1)))),
        "todoTree.ancestorPath" => ok(json!(ancestor_path(&nodes(), int(1)))),
        "todoTree.flattenTree" => ok(json!(flatten_tree(&nodes()))),
        "todoTree.visibleRoots" => ok(json!(visible_roots(&nodes()))),
        "todoTree.flattenSubset" => ok(json!(flatten_subset(&nodes()))),
        "todoTree.subtreeIds" => ok(json!(subtree_ids(&nodes(), int(1)?))),
        "todoTree.descendantCount" => ok(json!(descendant_count(&nodes(), int(1)?))),
        "todoTree.clampDropDepth" => ok(json!(clamp_drop_depth(
            int(0)?,
            row(1).as_ref(),
            row(2).as_ref()
        ))),
        "todoTree.resolveDrop" => {
            let candidates: Vec<TreeRow> = serde_json::from_value(args.get(1)?.clone()).ok()?;
            let slot: DropSlot = serde_json::from_value(args.get(3)?.clone()).ok()?;
            let invalid: Option<HashSet<i64>> = args
                .get(4)
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(Json::as_i64).collect());
            ok(json!(resolve_drop(
                &nodes(),
                &candidates,
                int(2)?,
                slot,
                invalid.as_ref()
            )))
        }
        "todoTree.dayProgress" => {
            let (done, total) = day_progress(&nodes());
            ok(json!({ "done": done, "total": total }))
        }
        _ => None,
    }
}
