// 렌더된 mermaid SVG 에서 무엇이 무엇과 이어져 있는지 읽는다 — 지금 앱 lib/diagramGraph.ts 와 DiagramCanvas.tsx 의
// buildGraph, readColumns, extractNodeId, findLineForNodeId. 원문을 다시 파싱하지 않고 merman 이 만든 SVG(mermaid 와 같은
// 마크업: g.node, g.edgePaths > path[data-id], g.label.attribute-*)를 읽는다 — ER 관계 표기와 별칭을 다시 풀면 틀리기 쉽다.
//
// 그림은 usvg 가 다시 읽는데 usvg 는 class 를 버린다. 그래서 이 단계에서 id 가 없는 라벨(g.label, g.edgeLabel)에
// id 를 심어 둔다 — 강조와 흐리기를 <style> 의 #id 규칙으로 걸고, 자리는 usvg 의 node_by_id 로 잰다.

use serde_json::{Value as Json, json};

// ---- lib/diagramGraph.ts ----

/// `string?` → ("string", true). `?` 가 없으면 그대로
pub fn split_optional_type(raw: &str) -> (String, bool) {
    match raw.strip_suffix('?') {
        Some(t) => (t.to_string(), true),
        None => (raw.to_string(), false),
    }
}

/// 코멘트 앞머리의 `[NULL]`/`[NOTNULL]` — `/\[\s*(?:NOT\s*NULL|NULL)\s*\]/i` 의 첫 일치
pub fn null_flag_of(comment: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| regex::Regex::new(r"(?i)\[\s*(?:NOT\s*NULL|NULL)\s*\]").expect("re"));
    re.find(comment)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

/// 한 컬럼의 널 표기 — `?` 규약을 같은 표에서 한 번이라도 썼을 때만 나머지를 NOT NULL 로 읽는다
pub fn nullability_flag(raw_type: &str, comment: &str, entity_uses_optional: bool) -> String {
    if split_optional_type(raw_type).1 {
        return "[NULL]".into();
    }
    if entity_uses_optional {
        return "[NOTNULL]".into();
    }
    null_flag_of(comment)
}

/// 엣지 data-id → (원천, 대상). 이름에 '_' 가 들어가서 알려진 노드 id 로 맞춰 본다(긴 것부터)
pub fn parse_edge_endpoints(data_id: &str, node_ids: &[String]) -> Option<(String, String)> {
    if data_id.is_empty() {
        return None;
    }
    // dagre 자기루프 3토막 — 세 조각 모두 같은 노드의 자기참조
    for suffix in [
        "-cyclic-special-1",
        "-cyclic-special-2",
        "-cyclic-special-mid",
    ] {
        if let Some(id) = data_id.strip_suffix(suffix).filter(|s| !s.is_empty()) {
            return node_ids
                .iter()
                .any(|n| n == id)
                .then(|| (id.to_string(), id.to_string()));
        }
    }
    let rest = data_id.strip_prefix("id_")?;
    let mut by_len: Vec<&String> = node_ids.iter().collect();
    // 안정 정렬 — 길이가 같으면 원래 순서(JS sort 와 같다)
    by_len.sort_by_key(|s| std::cmp::Reverse(s.encode_utf16().count()));
    for source in &by_len {
        let Some(after) = rest.strip_prefix(&format!("{source}_")) else {
            continue;
        };
        for target in &by_len {
            if after == target.as_str() || after.starts_with(&format!("{target}_")) {
                return Some((source.to_string(), target.to_string()));
            }
        }
    }
    None
}

/// 직접 이어진 노드들(자기 자신은 빼고) — JS Set 처럼 처음 넣은 순서
pub fn neighbors_of(node_id: &str, edges: &[(String, String)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |x: &String| {
        if !out.contains(x) {
            out.push(x.clone());
        }
    };
    for (s, t) in edges {
        if s == node_id {
            add(t);
        }
        if t == node_id {
            add(s);
        }
    }
    out.retain(|x| x != node_id);
    out
}

// ---- DiagramCanvas.tsx ----

/// mermaid 가 붙인 접두사와 끝 번호를 뗀 노드 id(`flowchart-A-0` → `A`)
pub fn extract_node_id(raw: &str) -> String {
    let s = match raw.rfind('-') {
        Some(i) if i + 1 < raw.len() && raw[i + 1..].bytes().all(|b| b.is_ascii_digit()) => {
            &raw[..i]
        }
        _ => raw,
    };
    const PREFIXES: [&str; 13] = [
        "flowchart",
        "graph",
        "state",
        "actor",
        "classId",
        "class",
        "er",
        "mindmap",
        "note",
        "gantt",
        "pie",
        "quadrant",
        "sankey",
    ];
    for p in PREFIXES {
        if s.len() > p.len() + 1
            && s[..p.len()].eq_ignore_ascii_case(p)
            && s.as_bytes()[p.len()] == b'-'
        {
            return s[p.len() + 1..].to_string();
        }
    }
    s.to_string()
}

/// 원문에서 노드 id 가 처음 나오는 줄(1부터, 없으면 -1) — 앞뒤가 구분 문자여야 한다
pub fn find_line_for_node_id(node_id: &str, code: &str) -> i64 {
    if node_id.is_empty() || code.is_empty() {
        return -1;
    }
    let pat = format!(
        r#"(^|[\s\[\(\{{<>"|*&!\->,;]){}([\s\[\(\{{<>"|*&!\->,;:]|$)"#,
        regex::escape(node_id)
    );
    let Ok(re) = regex::Regex::new(&pat) else {
        return -1;
    };
    code.split('\n')
        .position(|l| re.is_match(l))
        .map_or(-1, |i| i as i64 + 1)
}

// ---- 구조 읽기 ----

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub ty: String,
    pub name: String,
    pub keys: String,
    pub flag: String,
}

#[derive(Debug, Clone)]
pub struct GNode {
    /// 그래프 키 — svg id 접두사를 뗀 것(`entity-USER-0`)
    pub key: String,
    pub dom_id: String,
    /// 보일 이름 — 첫 라벨의 글자
    pub text: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone)]
pub struct GEdge {
    pub dom_id: String,
    /// 라벨은 순서로만 짝지을 수 있다(개수가 같을 때만)
    pub label_id: Option<String>,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct GLabel {
    pub dom_id: String,
    pub text: String,
    /// 이 라벨을 품은 노드(키)
    pub node: Option<String>,
    /// 라벨 안 <use> 가 가리키는 글자 묶음의 id — 글자는 defs 에 있어 라벨 id 로는 닿지 않는다
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Structure {
    /// 라벨에 id 를 심은 SVG — 이걸 그린다
    pub svg: String,
    pub nodes: Vec<GNode>,
    pub edges: Vec<GEdge>,
    pub labels: Vec<GLabel>,
}

fn has_class(n: &roxmltree::Node, c: &str) -> bool {
    n.attribute("class")
        .is_some_and(|v| v.split_whitespace().any(|x| x == c))
}

/// 보이는 글자 — merman 의 resvg 길은 HTML 라벨을 defs 의 글자로 옮기고 <use href="#…"> 로 가리킨다. 그 끝까지 따라간다
fn text_of(n: &roxmltree::Node, ids: &std::collections::HashMap<&str, roxmltree::Node>) -> String {
    fn collect<'a>(
        n: &roxmltree::Node<'a, 'a>,
        ids: &std::collections::HashMap<&str, roxmltree::Node<'a, 'a>>,
        out: &mut String,
        depth: u8,
    ) {
        for d in n.descendants() {
            if d.is_text() {
                out.push_str(d.text().unwrap_or_default());
                out.push(' ');
            } else if d.has_tag_name("use") && depth < 4 {
                let href = d
                    .attribute("href")
                    .or_else(|| d.attribute(("http://www.w3.org/1999/xlink", "href")))
                    .unwrap_or_default();
                if let Some(t) = href.strip_prefix('#').and_then(|id| ids.get(id)) {
                    collect(t, ids, out, depth + 1);
                }
            }
        }
    }
    let mut out = String::new();
    collect(n, ids, &mut out, 0);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// SVG 를 읽어 노드, 엣지, 라벨을 모으고 id 없는 라벨에 id 를 심는다. 못 읽으면 빈 구조(포커스, 찾기가 꺼진다)
pub fn read_structure(svg: &str) -> Structure {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return Structure {
            svg: svg.to_string(),
            ..Default::default()
        };
    };
    let root = doc.root_element();
    let by_id: std::collections::HashMap<&str, roxmltree::Node> = doc
        .descendants()
        .filter_map(|n| n.attribute("id").map(|i| (i, n)))
        .collect();
    let prefix = root
        .attribute("id")
        .map(|i| format!("{i}-"))
        .unwrap_or_default();
    // 심을 id: (요소 시작 바이트, id)
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut id_of = |n: &roxmltree::Node, made: String| -> String {
        match n.attribute("id") {
            Some(i) => i.to_string(),
            None => {
                inserts.push((n.range().start, made.clone()));
                made
            }
        }
    };
    let mut nodes: Vec<GNode> = Vec::new();
    let mut labels: Vec<GLabel> = Vec::new();
    for n in root
        .descendants()
        .filter(|n| n.has_tag_name("g") && has_class(n, "node"))
    {
        let Some(raw) = n.attribute("id") else {
            continue;
        };
        // 흐름도는 merman 이 DOM id 를 제 것으로 짓고 mermaid 의 노드 id 는 data-id 에 둔다. ER 엔티티는 DOM id 가 곧 키
        let Some(key) = n.attribute("data-id").or_else(|| raw.strip_prefix(&prefix)) else {
            continue;
        };
        // 엔티티 본문은 g.label.attribute-{type|name|keys|comment} 로 쪼개져 있다 — 칸이 다시 나오면 다음 줄
        let mut raw_cols: Vec<[String; 4]> = Vec::new();
        let mut cur: [Option<String>; 4] = Default::default();
        let mut name = None;
        for l in n
            .descendants()
            .filter(|d| d.has_tag_name("g") && has_class(d, "label"))
        {
            let txt = text_of(&l, &by_id);
            if name.is_none() && !txt.is_empty() {
                name = Some(txt.clone());
            }
            let field = [
                "attribute-type",
                "attribute-name",
                "attribute-keys",
                "attribute-comment",
            ]
            .iter()
            .position(|c| has_class(&l, c));
            if let Some(f) = field {
                if cur[f].is_some() {
                    raw_cols.push(cur.clone().map(Option::unwrap_or_default));
                    cur = Default::default();
                }
                cur[f] = Some(txt.clone());
            }
        }
        if cur.iter().any(Option::is_some) {
            raw_cols.push(cur.map(Option::unwrap_or_default));
        }
        let uses_optional = raw_cols.iter().any(|c| split_optional_type(&c[0]).1);
        let columns = raw_cols
            .iter()
            .map(|[ty, name, keys, comment]| Column {
                ty: split_optional_type(ty).0,
                name: name.clone(),
                keys: keys.clone(),
                flag: nullability_flag(ty, comment, uses_optional),
            })
            .collect();
        nodes.push(GNode {
            key: key.to_string(),
            dom_id: raw.to_string(),
            text: name.unwrap_or_else(|| text_of(&n, &by_id)),
            columns,
        });
    }
    // 찾기 대상 — 라벨마다(없으면 text 마다). 품은 노드를 같이 적는다
    let pool: Vec<roxmltree::Node> = {
        let ls: Vec<_> = root
            .descendants()
            .filter(|d| d.has_tag_name("g") && has_class(d, "label"))
            .collect();
        if ls.is_empty() {
            root.descendants()
                .filter(|d| d.has_tag_name("text"))
                .collect()
        } else {
            ls
        }
    };
    for (i, l) in pool.iter().enumerate() {
        let owner = l
            .ancestors()
            .find(|a| a.has_tag_name("g") && has_class(a, "node"))
            .and_then(|a| {
                a.attribute("data-id")
                    .or_else(|| a.attribute("id")?.strip_prefix(&prefix))
            })
            .map(str::to_string);
        let dom_id = id_of(l, format!("dgm-l-{i}"));
        let refs = l
            .descendants()
            .filter(|d| d.has_tag_name("use"))
            .filter_map(|d| {
                d.attribute("href")
                    .or_else(|| d.attribute(("http://www.w3.org/1999/xlink", "href")))?
                    .strip_prefix('#')
                    .map(str::to_string)
            })
            .collect();
        labels.push(GLabel {
            dom_id,
            text: text_of(l, &by_id),
            node: owner,
            refs,
        });
    }
    let keys: Vec<String> = nodes.iter().map(|n| n.key.clone()).collect();
    let paths: Vec<_> = root
        .descendants()
        .filter(|d| {
            d.has_tag_name("path")
                && d.parent_element()
                    .is_some_and(|p| has_class(&p, "edgePaths"))
        })
        .collect();
    let edge_labels: Vec<_> = root
        .descendants()
        .filter(|d| {
            d.has_tag_name("g")
                && has_class(d, "edgeLabel")
                && d.parent_element()
                    .is_some_and(|p| has_class(&p, "edgeLabels"))
        })
        .collect();
    let pairable = edge_labels.len() == paths.len();
    let mut edges = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let Some((source, target)) =
            parse_edge_endpoints(p.attribute("data-id").unwrap_or_default(), &keys)
        else {
            continue;
        };
        let dom_id = id_of(p, format!("dgm-p-{i}"));
        let label_id = pairable.then(|| id_of(&edge_labels[i], format!("dgm-el-{i}")));
        edges.push(GEdge {
            dom_id,
            label_id,
            source,
            target,
        });
    }
    // 뒤에서부터 심어야 앞 자리가 밀리지 않는다. 요소 시작은 '<태그' — 태그 이름 뒤에 넣는다
    inserts.sort_by_key(|(pos, _)| std::cmp::Reverse(*pos));
    inserts.dedup_by_key(|(pos, _)| *pos);
    let mut out = svg.to_string();
    for (pos, id) in inserts {
        let tag_end = out[pos + 1..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map_or(pos + 1, |j| pos + 1 + j);
        out.insert_str(tag_end, &format!(" id=\"{id}\""));
    }
    Structure {
        svg: out,
        nodes,
        edges,
        labels,
    }
}

// ---- 하네스 ----

pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let ids = |i: usize| -> Vec<String> {
        args.get(i)
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    let out = match op {
        "diagramGraph.splitOptionalType" => {
            let (t, o) = split_optional_type(s(0));
            json!({"type": t, "optional": o})
        }
        "diagramGraph.nullFlagOf" => json!(null_flag_of(s(0))),
        "diagramGraph.nullabilityFlag" => json!(nullability_flag(
            s(0),
            s(1),
            args.get(2).and_then(Json::as_bool).unwrap_or(false)
        )),
        "diagramGraph.parseEdgeEndpoints" => match parse_edge_endpoints(s(0), &ids(1)) {
            Some((source, target)) => json!({"source": source, "target": target}),
            None => Json::Null,
        },
        "diagramGraph.neighborsOf" => {
            let edges: Vec<(String, String)> = args
                .get(1)
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .map(|e| {
                    (
                        e["source"].as_str().unwrap_or_default().to_string(),
                        e["target"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect();
            json!(neighbors_of(s(0), &edges))
        }
        _ => return None,
    };
    Some(Ok(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_merman_er_structure() {
        let svg = crate::mermaid::to_svg(
            "erDiagram\n  USER ||--o{ ORDER : places\n  USER {\n    bigint id PK\n    varchar? email \"메일\"\n  }\n  ORDER {\n    bigint id PK\n  }\n",
        )
        .unwrap();
        let s = read_structure(&svg);
        let user = s
            .nodes
            .iter()
            .find(|n| n.text == "USER")
            .unwrap_or_else(|| panic!("USER 엔티티: {:#?}", s.nodes));
        assert!(user.key.contains("USER"), "{}", user.key);
        assert_eq!(
            user.columns,
            vec![
                Column {
                    ty: "bigint".into(),
                    name: "id".into(),
                    keys: "PK".into(),
                    flag: "[NOTNULL]".into()
                },
                Column {
                    ty: "varchar".into(),
                    name: "email".into(),
                    keys: "".into(),
                    flag: "[NULL]".into()
                },
            ]
        );
        assert_eq!(s.edges.len(), 1);
        let order = s.nodes.iter().find(|n| n.text == "ORDER").unwrap();
        assert_eq!(
            (s.edges[0].source.as_str(), s.edges[0].target.as_str()),
            (user.key.as_str(), order.key.as_str())
        );
        // 심은 id 로 usvg 가 다시 찾는다
        let mut opt = resvg::usvg::Options::default();
        std::sync::Arc::make_mut(&mut opt.fontdb).load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&s.svg, &opt).unwrap();
        assert!(tree.node_by_id(&user.dom_id).is_some());
        assert!(tree.node_by_id(&s.edges[0].dom_id).is_some());
        let label = s
            .labels
            .iter()
            .find(|l| l.text == "email")
            .expect("컬럼 라벨");
        assert_eq!(label.node.as_deref(), Some(user.key.as_str()));
        assert!(tree.node_by_id(&label.dom_id).is_some(), "id 를 심었다");
    }

    #[test]
    fn reads_flowchart_edges() {
        let svg =
            crate::mermaid::to_svg("flowchart LR\n  A[Start] --> B[Mid]\n  B --> C\n").unwrap();
        let s = read_structure(&svg);
        let keys: Vec<(&str, &str)> = s
            .nodes
            .iter()
            .map(|n| (n.key.as_str(), n.text.as_str()))
            .collect();
        assert_eq!(keys, vec![("A", "Start"), ("B", "Mid"), ("C", "C")]);
        // 흐름도 엣지는 mermaid 11.16 도 merman 도 data-id 가 `L_A_B_0` 이다. 지금 앱의 parseEdgeEndpoints 는 `id_`(ER)만
        // 읽으니 흐름도에는 포커스가 없다 — 같게 둔다
        assert!(s.svg.contains("data-id=\"L_A_B_0\""));
        assert!(s.edges.is_empty());
    }

    #[test]
    fn node_ids_and_lines() {
        assert_eq!(extract_node_id("flowchart-A-0"), "A");
        assert_eq!(extract_node_id("entity-USER-3"), "entity-USER");
        assert_eq!(
            find_line_for_node_id("B", "flowchart LR\n  A --> B\n  B --> C"),
            2
        );
        assert_eq!(find_line_for_node_id("BB", "A --> B"), -1);
    }
}
