// 표 패널과 컬럼 정보창 — resin/viewer 의 tableDetails, columnDetails 를 같은 모양의 요소 나무로 옮겼다.
// 웹은 DOM 을 만들고 viewer.css 가 칠한다. 여기선 같은 나무(태그, 클래스, 자식)를 만들고 화면이 클래스를 읽어 칠한다.
// 비교 시험(tests/resin_details.json)은 진짜 resin 이 만든 나무와 마디 하나까지 견준다.
// 비교판(diff)의 표시(t.change, c.change)는 amber 가 쓰지 않아 뺐다. 서비스 패널(serviceDetails)도 그렇다.

use super::{ConstraintKind, Model, ModelColumn, ModelTable, RefKind, Relation, TableOrigin};

/// 누르면 할 일 — 웹의 open(name), pick(table, column), openService(name)
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Open(String),
    Pick(String, String),
    Service(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    El(El),
}

#[derive(Debug, Clone, PartialEq)]
pub struct El {
    pub tag: &'static str,
    pub class: Option<String>,
    /// tr 의 data-c(컬럼 행)
    pub c: Option<String>,
    pub title: Option<String>,
    pub act: Option<Act>,
    pub kids: Vec<Node>,
}

impl El {
    pub fn has(&self, class: &str) -> bool {
        self.class
            .as_deref()
            .is_some_and(|c| c.split(' ').any(|x| x == class))
    }

    /// 자식 글자를 이어 붙인 것
    pub fn text(&self) -> String {
        let mut s = String::new();
        for k in &self.kids {
            match k {
                Node::Text(t) => s += t,
                Node::El(e) => s += &e.text(),
            }
        }
        s
    }

    /// 비교용 — fixture 와 같은 모양의 JSON
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("tag".into(), self.tag.into());
        m.insert(
            "class".into(),
            self.class
                .clone()
                .map_or(serde_json::Value::Null, Into::into),
        );
        if let Some(c) = &self.c {
            m.insert("c".into(), c.clone().into());
        }
        if let Some(t) = &self.title {
            m.insert("title".into(), t.clone().into());
        }
        m.insert(
            "kids".into(),
            self.kids
                .iter()
                .map(|k| match k {
                    Node::Text(t) => serde_json::Value::String(t.clone()),
                    Node::El(e) => e.to_json(),
                })
                .collect(),
        );
        serde_json::Value::Object(m)
    }
}

/// 웹의 h(tag, cls, ...children) — 빈 자식(null)은 넘기지 않는다. 빈 글자("")는 마디로 남는다
fn h(tag: &'static str, class: Option<&str>, kids: Vec<Option<Node>>) -> El {
    El {
        tag,
        class: class.map(str::to_string),
        c: None,
        title: None,
        act: None,
        kids: kids.into_iter().flatten().collect(),
    }
}

fn t(s: impl Into<String>) -> Option<Node> {
    Some(Node::Text(s.into()))
}

fn e(el: El) -> Option<Node> {
    Some(Node::El(el))
}

fn service_description(model: &Model, name: &str) -> String {
    model
        .services
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| s.description.as_deref())
        .filter(|d| !d.is_empty())
        .map(|d| format!(", {d}"))
        .unwrap_or_default()
}

/// 컬럼의 가장 센 역할: 기본 키, 유일, 참조, 그냥 색인
fn key_of(c: &ModelColumn, refs: usize, table: &ModelTable) -> Option<&'static str> {
    if c.pk {
        Some("PK")
    } else if c.uk {
        Some("UK")
    } else if c.reference.is_some()
        || refs > 0
        || table
            .foreign_keys
            .as_ref()
            .is_some_and(|fks| fks.iter().any(|fk| fk.columns.contains(&c.name)))
    {
        Some("FK")
    } else if c.index.is_some() {
        Some("IX")
    } else {
        None
    }
}

fn kind_of(r: &Relation) -> &'static str {
    if r.kind == RefKind::Physical {
        "foreign key"
    } else {
        "logical"
    }
}

fn child_columns(r: &Relation) -> Vec<String> {
    r.child_columns
        .clone()
        .unwrap_or_else(|| vec![r.child_column.clone()])
}

fn parent_columns(r: &Relation) -> Vec<String> {
    r.parent_columns
        .clone()
        .unwrap_or_else(|| vec![r.parent_column.clone()])
}

fn column_suffix(columns: &[String]) -> String {
    if columns.len() == 1 {
        format!(".{}", columns[0])
    } else {
        format!(" ({})", columns.join(", "))
    }
}

fn link(name: &str, act: Act) -> El {
    let mut b = h("button", Some("d-link"), vec![t(name)]);
    b.title = Some(format!("Show {name}"));
    b.act = Some(act);
    b
}

fn open(name: &str) -> El {
    link(name, Act::Open(name.to_string()))
}

fn key(k: Option<&str>) -> El {
    let class = k.map_or("d-key".to_string(), |k| {
        format!("d-key k-{}", k.to_lowercase())
    });
    h("span", Some(&class), vec![t(k.unwrap_or(""))])
}

fn code(text: &str) -> El {
    h("code", None, vec![t(text)])
}

/// NULL 은 눈에 띄게(그림과 같은 호박색), 흔한 NOT NULL 은 조용히
fn nullity(c: &ModelColumn) -> El {
    h(
        "span",
        Some(if c.nullable {
            "t-null is-null"
        } else {
            "t-null"
        }),
        vec![t(if c.nullable { "NULL" } else { "NOT NULL" })],
    )
}

fn slashed(values: &[String]) -> Vec<Option<Node>> {
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(e(h("i", None, vec![t("/")])));
        }
        out.push(t(v.clone()));
    }
    out
}

fn list(title: &str, items: Vec<El>) -> El {
    h(
        "section",
        Some("d-section"),
        vec![
            e(h("h3", None, vec![t(title)])),
            e(h("ul", None, items.into_iter().map(e).collect())),
        ],
    )
}

/// 패널의 표 하나 — 좁으면 옆으로 굴린다
fn table(title: &str, class: &str, heads: &[(&str, Option<&str>)], rows: Vec<El>) -> El {
    let head = h(
        "tr",
        None,
        heads
            .iter()
            .map(|(label, c)| e(h("th", *c, vec![t(*label)])))
            .collect(),
    );
    h(
        "section",
        Some("d-section"),
        vec![
            e(h("h3", None, vec![t(title)])),
            e(h(
                "div",
                Some("t-wrap"),
                vec![e(h(
                    "table",
                    Some(&format!("t-table {class}")),
                    vec![
                        e(h("thead", None, vec![e(head)])),
                        e(h("tbody", None, rows.into_iter().map(e).collect())),
                    ],
                ))],
            )),
        ],
    )
}

struct Ix {
    kind: &'static str,
    name: Option<String>,
    columns: Vec<String>,
}

/// 표의 색인 전부 — 기본 키, 유일 키, 색인 순(한 컬럼, 여러 컬럼)
fn indexes_of(tb: &ModelTable) -> Vec<Ix> {
    let mut out = Vec::new();
    let pk: Vec<String> = tb
        .columns
        .iter()
        .filter(|c| c.pk)
        .map(|c| c.name.clone())
        .collect();
    if !pk.is_empty() {
        out.push(Ix {
            kind: "PK",
            name: None,
            columns: pk,
        });
    }
    for c in tb.columns.iter().filter(|c| c.uk) {
        out.push(Ix {
            kind: "UK",
            name: c.uk_name.clone(),
            columns: vec![c.name.clone()],
        });
    }
    for k in tb
        .constraints
        .iter()
        .filter(|k| k.kind == ConstraintKind::Unique)
    {
        out.push(Ix {
            kind: "UK",
            name: k.name.clone(),
            columns: k.columns.clone(),
        });
    }
    for c in &tb.columns {
        if let Some(ix) = &c.index {
            out.push(Ix {
                kind: "IX",
                name: ix.name.clone(),
                columns: vec![c.name.clone()],
            });
        }
    }
    for k in tb
        .constraints
        .iter()
        .filter(|k| k.kind == ConstraintKind::Index)
    {
        out.push(Ix {
            kind: "IX",
            name: k.name.clone(),
            columns: k.columns.clone(),
        });
    }
    out
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn label_of(tb: &ModelTable) -> &str {
    tb.label.as_deref().unwrap_or(&tb.name)
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.is_empty())
}

/// 오른쪽 패널: 표 하나에 대한 전부
pub fn table_details(model: &Model, name: &str) -> Option<El> {
    let tb = model.tables.iter().find(|x| x.name == name)?;
    let outgoing: Vec<&Relation> = model.relations.iter().filter(|r| r.child == name).collect();
    let incoming: Vec<&Relation> = model
        .relations
        .iter()
        .filter(|r| r.parent == name)
        .collect();
    let mut tags: Vec<String> = Vec::new();
    match tb.origin {
        TableOrigin::External => tags.push("EXTERNAL".into()),
        TableOrigin::Audit => tags.push("GENERATED".into()),
        TableOrigin::Table => {}
    }
    if let Some(a) = &tb.audit {
        tags.push(a.method.to_uppercase());
    }
    let columns: Vec<El> = tb
        .columns
        .iter()
        .map(|c| {
            let mut name_button = h("button", Some("d-col-name"), vec![t(c.name.clone())]);
            name_button.title = Some("Show this column on the diagram".into());
            let mut notes: Vec<Option<Node>> = Vec::new();
            if let Some(values) = &c.enum_values {
                let mut kids = vec![e(h("span", Some("t-lab"), vec![t("values")])), t(" ")];
                kids.extend(slashed(values));
                notes.push(e(h("span", Some("t-note"), kids)));
            }
            if c.enc {
                notes.push(e(h(
                    "span",
                    Some("t-note"),
                    vec![
                        e(h("span", Some("t-lab"), vec![t("stored")])),
                        t(" encrypted"),
                    ],
                )));
            }
            let refs = outgoing
                .iter()
                .filter(|r| child_columns(r).contains(&c.name))
                .count();
            let mut desc = vec![
                non_empty(&c.description)
                    .map(|d| Node::El(h("span", Some("t-desc-text"), vec![t(d)]))),
            ];
            desc.extend(notes);
            let mut row = h(
                "tr",
                None,
                vec![
                    e(h("td", Some("t-key"), vec![e(key(key_of(c, refs, tb)))])),
                    e(h("td", Some("t-name"), vec![e(name_button)])),
                    e(h("td", Some("t-type"), vec![t(c.ty.clone())])),
                    e(h("td", None, vec![e(nullity(c))])),
                    e(h("td", Some("t-desc"), desc)),
                ],
            );
            row.c = Some(c.name.clone());
            // 행 전체가 컬럼을 고른다
            row.act = Some(Act::Pick(tb.name.clone(), c.name.clone()));
            row
        })
        .collect();
    let indexes: Vec<El> = indexes_of(tb)
        .into_iter()
        .map(|ix| {
            let named = ix.name.as_deref().is_some_and(|n| !n.is_empty());
            let text = ix.name.clone().unwrap_or_else(|| {
                if ix.kind == "PK" {
                    "primary key".into()
                } else {
                    "unnamed".into()
                }
            });
            h(
                "tr",
                None,
                vec![
                    e(h("td", Some("t-key"), vec![e(key(Some(ix.kind)))])),
                    e(h(
                        "td",
                        Some(if named { "t-name" } else { "t-name t-none" }),
                        vec![t(text)],
                    )),
                    e(h("td", Some("t-type"), vec![t(ix.columns.join(", "))])),
                ],
            )
        })
        .collect();
    let references: Vec<El> = outgoing
        .iter()
        .map(|r| {
            h(
                "tr",
                None,
                vec![
                    e(h(
                        "td",
                        Some("t-name"),
                        vec![t(child_columns(r).join(", "))],
                    )),
                    e(h(
                        "td",
                        Some("t-name"),
                        vec![e(open(&r.parent)), t(column_suffix(&parent_columns(r)))],
                    )),
                    e(h(
                        "td",
                        Some("t-rel"),
                        vec![t(format!(
                            "{}, {}{}",
                            kind_of(r),
                            if r.one { "one-to-one" } else { "many-to-one" },
                            if r.optional { ", optional" } else { "" }
                        ))],
                    )),
                ],
            )
        })
        .collect();
    let referenced_by: Vec<El> = incoming
        .iter()
        .map(|r| {
            h(
                "tr",
                None,
                vec![
                    e(h(
                        "td",
                        Some("t-name"),
                        vec![e(open(&r.child)), t(column_suffix(&child_columns(r)))],
                    )),
                    e(h(
                        "td",
                        Some("t-name"),
                        vec![t(parent_columns(r).join(", "))],
                    )),
                    e(h(
                        "td",
                        Some("t-rel"),
                        vec![t(format!(
                            "{}, {}",
                            kind_of(r),
                            if r.one { "one-to-one" } else { "one-to-many" }
                        ))],
                    )),
                ],
            )
        })
        .collect();
    let mut audit: Vec<El> = Vec::new();
    if let Some(a) = &tb.audit {
        let mut kids = vec![t(format!("Audited with {} on ", a.method))];
        for (i, c) in a.columns.iter().enumerate() {
            kids.push(t(if i > 0 { ", " } else { "" }));
            kids.push(e(code(c)));
        }
        kids.push(t("."));
        audit.push(h("li", Some("d-note"), kids));
        let aud = format!("{}_aud", label_of(tb));
        let aud_name = model
            .tables
            .iter()
            .find(|x| {
                x.origin == TableOrigin::Audit && x.service == tb.service && label_of(x) == aud
            })
            .map(|x| x.name.clone())
            .unwrap_or_else(|| format!("{}_aud", tb.name));
        audit.push(h(
            "li",
            Some("d-note"),
            vec![
                t("Generated tables: "),
                e(open("revinfo")),
                t(", "),
                e(open(&aud_name)),
            ],
        ));
    } else if tb.origin == TableOrigin::Audit {
        let label = label_of(tb);
        let base = label.strip_suffix("_aud").and_then(|stem| {
            model
                .tables
                .iter()
                .find(|x| x.audit.is_some() && x.service == tb.service && label_of(x) == stem)
                .map(|x| x.name.clone())
        });
        audit.push(match base {
            Some(base) => h(
                "li",
                Some("d-note"),
                vec![
                    t("Generated by the audit of "),
                    e(open(&base)),
                    t(": one row per revision of each change."),
                ],
            ),
            None => h(
                "li",
                Some("d-note"),
                vec![t("One row per revision, shared by every audited table.")],
            ),
        });
    }
    let head = h(
        "header",
        Some("d-head"),
        vec![
            e(h("h2", Some("d-name"), vec![t(tb.name.clone())])),
            (!tags.is_empty()).then(|| {
                Node::El(h(
                    "span",
                    Some("d-tags"),
                    tags.into_iter()
                        .map(|x| e(h("span", Some("d-chip"), vec![t(x)])))
                        .collect(),
                ))
            }),
            non_empty(&tb.description).map(|d| Node::El(h("p", Some("d-desc"), vec![t(d)]))),
            e(h(
                "p",
                Some("d-meta"),
                vec![t(format!(
                    "{}, {}, referenced by {}",
                    plural(tb.columns.len(), "column"),
                    plural(outgoing.len(), "reference"),
                    incoming.len()
                ))],
            )),
            non_empty(&tb.service).map(|s| {
                Node::El(h(
                    "p",
                    Some("d-meta"),
                    vec![
                        t("In service "),
                        e(link(s, Act::Service(s.to_string()))),
                        t(service_description(model, s)),
                    ],
                ))
            }),
        ],
    );
    Some(h(
        "div",
        Some("d-table"),
        vec![
            e(head),
            e(table(
                "Columns",
                "t-cols",
                &[
                    ("Key", Some("t-key")),
                    ("Column", None),
                    ("Type", None),
                    ("Null", None),
                    ("Description", None),
                ],
                columns,
            )),
            (!indexes.is_empty()).then(|| {
                Node::El(table(
                    "Indexes",
                    "t-ix",
                    &[("Key", Some("t-key")), ("Name", None), ("Columns", None)],
                    indexes,
                ))
            }),
            (!references.is_empty()).then(|| {
                Node::El(table(
                    "References",
                    "t-refs",
                    &[("Column", None), ("References", None), ("Relation", None)],
                    references,
                ))
            }),
            (!referenced_by.is_empty()).then(|| {
                Node::El(table(
                    "Referenced by",
                    "t-refs",
                    &[("From", None), ("Column", None), ("Relation", None)],
                    referenced_by,
                ))
            }),
            (!audit.is_empty()).then(|| Node::El(list("Audit", audit))),
        ],
    ))
}

/// 정보창: 컬럼 하나를 몇 줄로
pub fn column_details(model: &Model, table: &str, column: &str) -> Option<El> {
    let tb = model.tables.iter().find(|x| x.name == table)?;
    let c = tb.columns.iter().find(|x| x.name == column)?;
    let outgoing: Vec<&Relation> = model
        .relations
        .iter()
        .filter(|r| r.child == table && child_columns(r).iter().any(|x| x == column))
        .collect();
    let incoming: Vec<&Relation> = model
        .relations
        .iter()
        .filter(|r| r.parent == table && parent_columns(r).iter().any(|x| x == column))
        .collect();
    let mut facts: Vec<(&str, Vec<Option<Node>>)> = vec![
        ("Table", vec![e(open(&tb.name))]),
        ("Null", vec![e(nullity(c))]),
    ];
    if let Some(values) = &c.enum_values {
        facts.push(("Values", vec![t(values.join(", "))]));
    }
    if c.enc {
        facts.push(("Stored", vec![t("encrypted")]));
    }
    if let Some(ix) = &c.index {
        facts.push((
            "Index",
            vec![t(ix.name.clone().unwrap_or_else(|| "unnamed".into()))],
        ));
    }
    if c.uk {
        facts.push((
            "Unique",
            vec![t(c.uk_name.clone().unwrap_or_else(|| "unnamed".into()))],
        ));
    }
    for r in &outgoing {
        facts.push((
            "References",
            vec![
                e(open(&r.parent)),
                t(format!(
                    "{}, {}",
                    column_suffix(&parent_columns(r)),
                    kind_of(r)
                )),
            ],
        ));
    }
    if !incoming.is_empty() {
        let mut v = Vec::new();
        for (i, r) in incoming.iter().enumerate() {
            v.push(t(if i > 0 { ", " } else { "" }));
            v.push(e(open(&r.child)));
            v.push(t(column_suffix(&child_columns(r))));
        }
        facts.push(("Referenced by", v));
    }
    let mut dl = Vec::new();
    for (k, v) in facts {
        dl.push(e(h("dt", None, vec![t(k)])));
        dl.push(e(h("dd", None, v)));
    }
    Some(h(
        "div",
        Some("d-pop"),
        vec![
            e(h(
                "div",
                Some("d-pop-head"),
                vec![
                    e(key(key_of(c, outgoing.len(), tb))),
                    e(h("span", Some("d-pop-name"), vec![t(c.name.clone())])),
                    e(h("span", Some("d-type"), vec![t(c.ty.clone())])),
                ],
            )),
            non_empty(&c.description).map(|d| Node::El(h("p", Some("d-desc"), vec![t(d)]))),
            e(h("dl", Some("d-facts"), dl)),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::{column_details, table_details};
    use crate::resin::compile;
    use serde_json::Value;

    /// 진짜 resin 이 만든 나무와 견준다 — 처음 다른 표나 컬럼에서 멈춘다
    fn check(cases: &[Value]) -> (usize, usize) {
        let (mut panels, mut pops) = (0, 0);
        for case in cases {
            let model = compile(case["source"].as_str().unwrap())
                .model
                .expect("모델");
            for tb in case["tables"].as_array().unwrap() {
                let name = tb["table"].as_str().unwrap();
                let got = table_details(&model, name).expect(name).to_json();
                assert_eq!(got, tb["panel"], "패널 {name}");
                panels += 1;
                for col in tb["columns"].as_array().unwrap() {
                    let column = col["column"].as_str().unwrap();
                    let got = column_details(&model, name, column)
                        .expect(column)
                        .to_json();
                    assert_eq!(got, col["pop"], "정보창 {name}.{column}");
                    pops += 1;
                }
            }
        }
        (panels, pops)
    }

    #[test]
    fn matches_resin_details() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/resin_details.json")).unwrap();
        let (panels, pops) = check(&cases);
        assert!(panels > 50 && pops > 150, "{panels} {pops}");
    }

    /// 실데이터(로컬 전용) — AMBER_REAL_DETAILS 에 resin-fixture.mjs 가 남긴 erd_details.json
    #[test]
    fn matches_real_details() {
        let Ok(path) = std::env::var("AMBER_REAL_DETAILS") else {
            return;
        };
        let cases: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (panels, pops) = check(&cases);
        eprintln!("real: {panels} panels, {pops} columns");
    }

    #[test]
    fn missing_table_or_column_is_none() {
        let model = compile("table a {\n  id bigint pk\n}\n").model.unwrap();
        assert!(table_details(&model, "b").is_none());
        assert!(column_details(&model, "a", "x").is_none());
    }
}
