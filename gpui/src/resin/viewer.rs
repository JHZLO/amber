// resin/viewer 의 캔버스가 그림의 DOM 에서 읽는 것을, 그린 SVG 를 한 번 읽어 모아 둔다.
// - 누른 자리(canvas.ts hitOf): 표, 컬럼, 머리글
// - 초점(view.ts createFocus): 고른 표나 컬럼과 이어진 것만 남기고 흐린다
// 웹은 클래스를 붙였다 뗀다(.is-focus, .is-on). 여기선 같은 규칙(viewer.css)을 usvg 에 얹을 CSS 로 낸다.
// 고를 때 이름을 따옴표 안에 넣지 않으려고 카드, 그림자, 행, 관계마다 id(rzx-…)를 달아 둔다.
// 접힌 서비스 카드와 서비스 영역은 amber 가 그리지 않아(generateErd 가 서비스를 안 낸다) 읽지 않는다.

use std::collections::{BTreeSet, HashMap};

use super::svg::{HEAD, SvgBox};

/// 카드의 컬럼 행 — 그림 좌표. 가로는 카드 안쪽(1 씩 들인) 전체
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    pub column: String,
    pub y: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub table: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rows: Vec<CardRow>,
}

impl Card {
    /// 머리글(이름, 설명, 꼬리표)의 높이
    pub fn head(&self) -> f64 {
        HEAD.min(self.h)
    }
}

/// 관계 — a 는 부모, b 는 자식. 복합 키면 컬럼이 여럿
#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub a: String,
    pub b: String,
    pub a_cols: Vec<String>,
    pub b_cols: Vec<String>,
}

impl Rel {
    fn endpoints(&self) -> impl Iterator<Item = (&str, &str)> {
        let a = self.a_cols.iter().map(|c| (self.a.as_str(), c.as_str()));
        let b = self.b_cols.iter().map(|c| (self.b.as_str(), c.as_str()));
        a.chain(b)
    }
}

pub struct Drawn {
    /// id 를 단 SVG — 굽는 건 이것이다
    pub svg: String,
    /// 그린 순서(뒤의 것이 위)
    pub cards: Vec<Card>,
    pub rels: Vec<Rel>,
}

/// 누른 것 — hitOf 에서 amber 가 쓰는 칸만
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Hit {
    pub table: Option<String>,
    pub column: Option<String>,
    pub header: bool,
}

/// 초점에 남는 것 — 표(카드와 그림자), 관계(Drawn.rels 의 자리), 컬럼 행
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Focus {
    pub tables: BTreeSet<String>,
    pub rels: BTreeSet<usize>,
    pub rows: BTreeSet<(String, String)>,
}

fn has_class(n: &roxmltree::Node, class: &str) -> bool {
    n.attribute("class")
        .is_some_and(|c| c.split_ascii_whitespace().any(|x| x == class))
}

fn num(n: &roxmltree::Node, name: &str) -> f64 {
    n.attribute(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0.)
}

fn table_id(i: usize) -> String {
    format!("rzx-t{i}")
}
fn shadow_id(i: usize) -> String {
    format!("rzx-s{i}")
}
fn row_id(i: usize, j: usize) -> String {
    format!("rzx-t{i}-c{j}")
}
fn rel_id(i: usize) -> String {
    format!("rzx-r{i}")
}

impl Drawn {
    /// to_svg 의 결과를 읽는다 — 카드 자리는 boxes 가, 행과 관계는 SVG 가 말한다
    pub fn read(svg: &str, boxes: &[SvgBox]) -> Result<Drawn, String> {
        let doc = roxmltree::Document::parse_with_options(
            svg,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let at: HashMap<&str, &SvgBox> = boxes.iter().map(|b| (b.table.as_str(), b)).collect();
        let mut cards = Vec::new();
        let mut rels = Vec::new();
        // (요소가 시작하는 자리, 태그 이름 길이, 달 id)
        let mut ids: Vec<(usize, usize, String)> = Vec::new();
        let mut shadows: Vec<(usize, usize, String)> = Vec::new();
        for n in doc.descendants().filter(|n| n.is_element()) {
            let tag = n.tag_name().name().len();
            if has_class(&n, "rz-t") {
                let Some(table) = n.attribute("data-t") else {
                    continue;
                };
                let Some(b) = at.get(table) else {
                    continue;
                };
                let i = cards.len();
                ids.push((n.range().start, tag, table_id(i)));
                let mut rows = Vec::new();
                for r in n.descendants().filter(|r| has_class(r, "rz-c")) {
                    let Some(column) = r.attribute("data-c") else {
                        continue;
                    };
                    let Some(hit) = r.children().find(|h| has_class(h, "rz-hit")) else {
                        continue;
                    };
                    ids.push((
                        r.range().start,
                        r.tag_name().name().len(),
                        row_id(i, rows.len()),
                    ));
                    rows.push(CardRow {
                        column: column.to_string(),
                        y: b.y + num(&hit, "y"),
                        h: num(&hit, "height"),
                    });
                }
                cards.push(Card {
                    table: table.to_string(),
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    rows,
                });
            } else if has_class(&n, "rz-s") {
                if let Some(table) = n.attribute("data-t") {
                    shadows.push((n.range().start, tag, table.to_string()));
                }
            } else if has_class(&n, "rz-r") {
                let cols = |side: &str| -> Vec<String> {
                    n.attribute(format!("data-{side}cs").as_str())
                        .and_then(|j| serde_json::from_str(j).ok())
                        .unwrap_or_else(|| {
                            vec![
                                n.attribute(format!("data-{side}c").as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            ]
                        })
                };
                ids.push((n.range().start, tag, rel_id(rels.len())));
                rels.push(Rel {
                    a: n.attribute("data-a").unwrap_or("").to_string(),
                    b: n.attribute("data-b").unwrap_or("").to_string(),
                    a_cols: cols("a"),
                    b_cols: cols("b"),
                });
            }
        }
        // 그림자는 카드보다 먼저 그려진다 — 표 이름으로 카드 자리를 찾는다
        let index: HashMap<&str, usize> = cards
            .iter()
            .enumerate()
            .map(|(i, c)| (c.table.as_str(), i))
            .collect();
        for (start, tag, table) in shadows {
            if let Some(&i) = index.get(table.as_str()) {
                ids.push((start, tag, shadow_id(i)));
            }
        }
        ids.sort_by_key(|x| x.0);
        let mut out = String::with_capacity(svg.len() + ids.len() * 20);
        let mut last = 0;
        for (start, tag, id) in ids {
            let at = start + 1 + tag;
            out.push_str(&svg[last..at]);
            out.push_str(&format!(" id=\"{id}\""));
            last = at;
        }
        out.push_str(&svg[last..]);
        Ok(Drawn {
            svg: out,
            cards,
            rels,
        })
    }

    pub fn card(&self, table: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.table == table)
    }

    /// 그림 좌표의 한 점에 무엇이 있나 — 위에 그린 카드부터. 카드 밖(선, 빈 곳)은 빈 Hit
    pub fn hit(&self, x: f64, y: f64) -> Hit {
        let Some(c) = self
            .cards
            .iter()
            .rev()
            .find(|c| x >= c.x && x <= c.x + c.w && y >= c.y && y <= c.y + c.h)
        else {
            return Hit::default();
        };
        let column = (x >= c.x + 1. && x <= c.x + c.w - 1.)
            .then(|| c.rows.iter().find(|r| y >= r.y && y < r.y + r.h))
            .flatten()
            .map(|r| r.column.clone());
        Hit {
            table: Some(c.table.clone()),
            header: column.is_none() && y < c.y + c.head(),
            column,
        }
    }

    /// 무엇을 남길지 — createFocus 의 select 그대로. 그림에 없는 표면 None(초점을 걷는다).
    /// 컬럼을 골랐는데 그 행에 걸린 관계가 없으면 표의 관계를 따라가되 상대 행은 칠하지 않는다
    pub fn focus(&self, table: &str, column: Option<&str>) -> Option<Focus> {
        self.card(table)?;
        let has_row = |t: &str, c: &str| {
            self.card(t)
                .is_some_and(|card| card.rows.iter().any(|r| r.column == c))
        };
        let by_row: Option<Vec<usize>> = column.and_then(|col| {
            let v: Vec<usize> = (0..self.rels.len())
                .filter(|&i| {
                    self.rels[i]
                        .endpoints()
                        .any(|(t, c)| t == table && c == col)
                })
                .collect();
            (!v.is_empty()).then_some(v)
        });
        let list = by_row.clone().unwrap_or_else(|| {
            (0..self.rels.len())
                .filter(|&i| self.rels[i].a == table || self.rels[i].b == table)
                .collect()
        });
        let mut f = Focus::default();
        for i in list {
            f.rels.insert(i);
            for (t, c) in self.rels[i].endpoints() {
                f.tables.insert(t.to_string());
                if by_row.is_some() && has_row(t, c) {
                    f.rows.insert((t.to_string(), c.to_string()));
                }
            }
        }
        f.tables.insert(table.to_string());
        if let Some(c) = column
            && has_row(table, c)
        {
            f.rows.insert((table.to_string(), c.to_string()));
        }
        Some(f)
    }

    /// 초점을 칠하는 CSS(viewer.css 의 .is-focus, .is-on) — 표, 관계, 그림자는 0.14 로 흐리고 남는 것만 되돌린다.
    /// 고른 행은 누를 자리(.rz-hit)를 강조색 0.18 로. usvg 는 규칙을 구체성 순으로 적용하므로 !important 없이 덮인다
    pub fn focus_css(&self, f: &Focus, accent: &str) -> String {
        let mut keep: Vec<String> = Vec::new();
        let mut rows: Vec<String> = Vec::new();
        for (i, c) in self.cards.iter().enumerate() {
            if f.tables.contains(&c.table) {
                keep.push(format!("#{}", table_id(i)));
                keep.push(format!("#{}", shadow_id(i)));
            }
            for (j, r) in c.rows.iter().enumerate() {
                if f.rows.contains(&(c.table.clone(), r.column.clone())) {
                    rows.push(format!("#{} > .rz-hit", row_id(i, j)));
                }
            }
        }
        keep.extend(f.rels.iter().map(|&i| format!("#{}", rel_id(i))));
        let mut css = String::from(".rz-t, .rz-s, .rz-r { opacity: 0.14; }");
        if !keep.is_empty() {
            css += &format!(" {} {{ opacity: 1; }}", keep.join(", "));
        }
        if !rows.is_empty() {
            css += &format!(
                " {} {{ fill: {accent}; fill-opacity: 0.18; }}",
                rows.join(", ")
            );
        }
        css
    }
}

#[cfg(test)]
mod tests {
    use super::{Drawn, Hit};
    use crate::resin::compile;
    use crate::resin::svg::{SvgOptions, to_svg};

    const SRC: &str = "\
table users {\n  id bigint pk\n  email varchar(255) uk\n}\n\
table orders {\n  id bigint pk\n  user_id bigint -> users.id\n  note text?\n}\n\
table items {\n  id bigint pk\n  order_id bigint -> orders.id\n}\n";

    fn drawn() -> Drawn {
        let model = compile(SRC).model.expect("모델");
        let svg = to_svg(
            &model,
            &SvgOptions {
                look: "aurora-dark".into(),
                stage: Some(true),
                id_prefix: Some("erd-".into()),
                ..Default::default()
            },
        )
        .unwrap();
        Drawn::read(&svg.svg, &svg.boxes).unwrap()
    }

    #[test]
    fn reads_cards_rows_and_relations() {
        let d = drawn();
        let names: Vec<&str> = d.cards.iter().map(|c| c.table.as_str()).collect();
        assert_eq!(names.len(), 3);
        for t in ["users", "orders", "items"] {
            assert!(names.contains(&t), "{t}");
        }
        let orders = d.card("orders").unwrap();
        let cols: Vec<&str> = orders.rows.iter().map(|r| r.column.as_str()).collect();
        assert_eq!(cols, ["id", "user_id", "note"]);
        assert_eq!(d.rels.len(), 2);
        let r = d.rels.iter().find(|r| r.b == "orders").unwrap();
        assert_eq!(
            (r.a.as_str(), r.a_cols.clone()),
            ("users", vec!["id".into()])
        );
        assert_eq!(r.b_cols, vec!["user_id".to_string()]);
        // 단 id 는 그대로 읽히는 SVG 여야 한다
        assert!(d.svg.contains("id=\"rzx-r0\"") && d.svg.contains("id=\"rzx-t0-c0\""));
        roxmltree::Document::parse_with_options(
            &d.svg,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn hit_finds_header_row_body_and_nothing() {
        let d = drawn();
        let c = d.card("orders").unwrap().clone();
        let head = d.hit(c.x + 10., c.y + 10.);
        assert_eq!(
            head,
            Hit {
                table: Some("orders".into()),
                column: None,
                header: true
            }
        );
        let r = &c.rows[1];
        let row = d.hit(c.x + c.w / 2., r.y + r.h / 2.);
        assert_eq!(row.column.as_deref(), Some("user_id"));
        assert!(!row.header);
        // 마지막 행 아래 여백은 몸통
        let body = d.hit(c.x + 10., c.y + c.h - 1.);
        assert_eq!(
            (body.table.as_deref(), body.column, body.header),
            (Some("orders"), None, false)
        );
        assert_eq!(d.hit(-5., -5.), Hit::default());
    }

    #[test]
    fn focus_follows_create_focus() {
        let d = drawn();
        // 표: 그 표의 관계와 양 끝 표, 행은 칠하지 않는다
        let f = d.focus("orders", None).unwrap();
        assert_eq!(f.tables.len(), 3);
        assert_eq!(f.rels.len(), 2);
        assert!(f.rows.is_empty());
        // 컬럼(관계 있음): 그 관계만, 양 끝 행까지
        let f = d.focus("orders", Some("user_id")).unwrap();
        let tables: Vec<&str> = f.tables.iter().map(|s| s.as_str()).collect();
        assert_eq!(tables, ["orders", "users"]);
        assert_eq!(f.rels.len(), 1);
        let rows: Vec<(&str, &str)> = f
            .rows
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        assert_eq!(rows, [("orders", "user_id"), ("users", "id")]);
        // 컬럼(관계 없음): 표의 관계를 따라가되 상대 행은 칠하지 않는다
        let f = d.focus("orders", Some("note")).unwrap();
        assert_eq!(f.rels.len(), 2);
        let rows: Vec<(&str, &str)> = f
            .rows
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        assert_eq!(rows, [("orders", "note")]);
        assert!(d.focus("nope", None).is_none());
    }

    /// CSS 가 usvg 에서 정말 흐리게 하는지 — 투명도가 걸린 묶음을 센다
    #[test]
    fn focus_css_dims_the_rest_in_usvg() {
        let d = drawn();
        let f = d.focus("users", Some("id")).unwrap();
        let css = d.focus_css(&f, "#e2a64d");
        let opts = |css: Option<String>| resvg::usvg::Options {
            style_sheet: css,
            ..Default::default()
        };
        let plain = resvg::usvg::Tree::from_str(&d.svg, &opts(None)).unwrap();
        let styled = resvg::usvg::Tree::from_str(&d.svg, &opts(Some(css))).unwrap();
        fn dimmed(g: &resvg::usvg::Group) -> usize {
            let own = usize::from((g.opacity().get() - 0.14).abs() < 1e-3);
            own + g
                .children()
                .iter()
                .map(|n| match n {
                    resvg::usvg::Node::Group(g) => dimmed(g),
                    _ => 0,
                })
                .sum::<usize>()
        }
        assert_eq!(dimmed(plain.root()), 0);
        // items 의 카드와 그림자, orders→items 관계
        assert_eq!(dimmed(styled.root()), 3);
    }
}
