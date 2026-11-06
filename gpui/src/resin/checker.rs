//! 뜻 검사(checker.ts)와 이름 풀기(resolution.ts) — 파싱은 되지만 말이 안 되는 것: 없는 표를 가리키는 참조, 두 번 쓴 이름,
//! 널이 되는 기본 키. 모델(model.rs)은 오류 없이 이 검사를 지난 나무만 본다.
//! JS 는 객체 자체를 키로 쓰지만 여기선 자리(표 번호, 컬럼 번호)로 가리킨다.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{
    ConstraintKind, Diagnostic, Document, Ident, RefKind, Table, error, table_id, warning,
};

pub const AUDIT_SUFFIX: &str = "_aud";
pub const REVINFO: &str = "revinfo";
const AUDIT_METHODS: [&str; 1] = ["envers"];
/// 맨 `audit envers` 가 빼는 컬럼 — 시각을 버전으로 남길 까닭이 없다
const AUDIT_SKIP: [&str; 2] = ["created_at", "updated_at"];

/// 한 번의 분석이 쓰는 색인
pub struct Resolution {
    pub names: HashMap<String, Vec<usize>>,
    /// 표마다 이름 → 첫 컬럼
    columns: Vec<HashMap<String, usize>>,
    keys: Vec<Vec<usize>>,
    scopes: HashMap<Option<String>, HashMap<String, usize>>,
    refs: RefCell<HashMap<(usize, usize), Option<(usize, usize)>>>,
}

impl Resolution {
    pub fn new(doc: &Document) -> Self {
        let mut names: HashMap<String, Vec<usize>> = HashMap::new();
        let mut scopes: HashMap<Option<String>, HashMap<String, usize>> = HashMap::new();
        let mut columns = Vec::new();
        let mut keys = Vec::new();
        for (ti, t) in doc.tables.iter().enumerate() {
            names.entry(t.name.text.clone()).or_default().push(ti);
            scopes
                .entry(t.service.as_ref().map(|s| s.text.clone()))
                .or_default()
                .entry(t.name.text.clone())
                .or_insert(ti);
            let mut cols = HashMap::new();
            for (ci, c) in t.columns.iter().enumerate() {
                cols.entry(c.name.text.clone()).or_insert(ci);
            }
            columns.push(cols);
            keys.push(
                t.columns
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.pk)
                    .map(|(i, _)| i)
                    .collect(),
            );
        }
        Resolution {
            names,
            columns,
            keys,
            scopes,
            refs: RefCell::default(),
        }
    }

    /// 이름이 가리키는 표 — 서비스를 적었으면 그 서비스에서, 아니면 제 서비스에서 먼저, 그다음 문서 전체에서 하나뿐일 때
    pub fn table(
        &self,
        doc: &Document,
        name: &str,
        owner: Option<usize>,
        service: Option<&str>,
    ) -> Option<usize> {
        if let Some(s) = service {
            return self
                .scopes
                .get(&Some(s.to_string()))
                .and_then(|m| m.get(name))
                .copied();
        }
        let scope = owner.and_then(|o| doc.tables[o].service.as_ref().map(|s| s.text.clone()));
        if let Some(t) = self.scopes.get(&scope).and_then(|m| m.get(name)) {
            return Some(*t);
        }
        match self.names.get(name) {
            Some(m) if m.len() == 1 => Some(m[0]),
            _ => None,
        }
    }

    /// 컬럼 참조가 가리키는 (표, 컬럼) — 풀 수 없으면 None(검사기가 이미 알렸다)
    pub fn reference(&self, doc: &Document, ti: usize, ci: usize) -> Option<(usize, usize)> {
        let r = doc.tables[ti].columns[ci].reference.as_ref()?;
        if let Some(hit) = self.refs.borrow().get(&(ti, ci)) {
            return *hit;
        }
        let table = self.table(
            doc,
            &r.table.text,
            Some(ti),
            r.service.as_ref().map(|s| s.text.as_str()),
        );
        let result = table.and_then(|t| {
            let target = match &r.column {
                Some(c) => self.columns[t].get(&c.text).copied(),
                None if self.keys[t].len() == 1 => Some(self.keys[t][0]),
                None => None,
            };
            target.map(|c| (t, c))
        });
        self.refs.borrow_mut().insert((ti, ci), result);
        result
    }
}

/// `audit` 이 감사 표로 옮기는 컬럼(기본 키 빼고). audit 이 없으면 빈 목록
pub fn audited_columns(t: &Table) -> Vec<usize> {
    let Some(a) = &t.audit else {
        return Vec::new();
    };
    let pick = |c: &super::Column| match &a.columns {
        None => !c.pk && !AUDIT_SKIP.contains(&c.name.text.as_str()),
        Some(list) => !c.pk && list.iter().any(|i| i.text == c.name.text),
    };
    t.columns
        .iter()
        .enumerate()
        .filter(|(_, c)| pick(c))
        .map(|(i, _)| i)
        .collect()
}

pub fn check(doc: &Document, resolution: &Resolution) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut tables: HashSet<String> = HashSet::new();
    for t in &doc.tables {
        if !tables.insert(table_id(t, None)) {
            out.push(error(
                format!("table `{}` is declared twice", t.name.text),
                t.name.span,
                None,
            ));
        }
    }

    let audited: Vec<&Table> = doc
        .tables
        .iter()
        .filter(|t| t.audit.is_some() && !t.external)
        .collect();
    if !audited.is_empty() {
        let mut generated: HashSet<String> = HashSet::from([REVINFO.to_string()]);
        generated.extend(
            audited
                .iter()
                .map(|t| table_id(t, Some(&format!("{}{AUDIT_SUFFIX}", t.name.text)))),
        );
        for t in &doc.tables {
            if generated.contains(&table_id(t, None)) {
                out.push(error(
                    format!(
                        "`{}` clashes with a table that `audit` generates",
                        t.name.text
                    ),
                    t.name.span,
                    Some("remove it and let `audit` generate it, or rename this table".into()),
                ));
            }
        }
    }

    // 서비스 — 서비스끼리 이름이 겹치면 안 되고, 표가 없는 서비스는 그릴 게 없다
    let mut services: HashSet<&str> = HashSet::new();
    for s in &doc.services {
        if !services.insert(&s.name.text) {
            out.push(error(
                format!("service `{}` is declared twice", s.name.text),
                s.name.span,
                None,
            ));
        }
        if !doc
            .tables
            .iter()
            .any(|t| t.service.as_ref().is_some_and(|x| x.text == s.name.text))
        {
            out.push(warning(
                format!("service `{}` has no tables", s.name.text),
                s.name.span,
                Some("put tables inside it, or remove it".into()),
            ));
        }
    }

    for ti in 0..doc.tables.len() {
        check_table(doc, ti, &mut out, resolution);
    }
    out
}

fn check_table(doc: &Document, ti: usize, out: &mut Vec<Diagnostic>, resolution: &Resolution) {
    let t = &doc.tables[ti];
    let mut cols: HashMap<&str, usize> = HashMap::new();
    for (ci, c) in t.columns.iter().enumerate() {
        if cols.contains_key(c.name.text.as_str()) {
            out.push(error(
                format!(
                    "column `{}` appears twice in table `{}`",
                    c.name.text, t.name.text
                ),
                c.name.span,
                None,
            ));
        } else {
            cols.insert(&c.name.text, ci);
        }
    }
    let known = |i: &Ident, out: &mut Vec<Diagnostic>| {
        if !cols.contains_key(i.text.as_str()) {
            out.push(error(
                format!("table `{}` has no column `{}`", t.name.text, i.text),
                i.span,
                None,
            ));
        }
    };

    for (ci, c) in t.columns.iter().enumerate() {
        if c.pk && c.nullable {
            out.push(error(
                format!("primary key `{}` cannot be nullable", c.name.text),
                c.ty.span,
                Some("remove the `?` after the type".into()),
            ));
        }
        if let Some(r) = &c.reference {
            if t.external {
                out.push(error(
                    "columns of an external table cannot hold references".into(),
                    r.span,
                    Some("external tables only receive references; declare it as `table` to draw this relation".into()),
                ));
            } else {
                check_ref(doc, ti, ci, out, resolution);
            }
        }
    }

    for k in &t.constraints {
        for i in &k.columns {
            known(i, out);
        }
        if k.columns.len() == 1 {
            let (kind, modifier) = match k.kind {
                ConstraintKind::Unique => ("unique", "uk"),
                ConstraintKind::Index => ("index", "index"),
            };
            let named = k
                .name
                .as_ref()
                .map(|n| format!(" (`{modifier} as {}`)", n.text))
                .unwrap_or_default();
            out.push(warning(
                format!("`{kind}(...)` over a single column"),
                k.span,
                Some(format!("use the column modifier `{modifier}`{named}")),
            ));
        }
    }

    for fk in &t.foreign_keys {
        for i in &fk.columns {
            known(i, out);
        }
        if t.external {
            out.push(error(
                "external tables cannot hold foreign keys".into(),
                fk.span,
                None,
            ));
        }
        if fk.columns.len() != fk.target_columns.len() {
            out.push(error(
                "foreign key lists must have the same number of columns".into(),
                fk.span,
                None,
            ));
        } else if fk.columns.len() < 2 {
            out.push(error(
                "a composite foreign key needs at least two columns".into(),
                fk.span,
                Some("use a column reference for one column".into()),
            ));
        }
        let distinct =
            |l: &[Ident]| l.iter().map(|c| &c.text).collect::<HashSet<_>>().len() == l.len();
        if !distinct(&fk.columns) || !distinct(&fk.target_columns) {
            out.push(error(
                "foreign key lists cannot repeat a column".into(),
                fk.span,
                None,
            ));
        }
        let target = resolution.table(
            doc,
            &fk.table.text,
            Some(ti),
            fk.service.as_ref().map(|s| s.text.as_str()),
        );
        match target {
            None => out.push(error(
                format!(
                    "referenced table `{}` is missing or ambiguous",
                    fk.table.text
                ),
                fk.table.span,
                Some("qualify the table with its service".into()),
            )),
            Some(target) => {
                let target = &doc.tables[target];
                for (i, named) in fk.target_columns.iter().enumerate() {
                    let to = target.columns.iter().find(|c| c.name.text == named.text);
                    let from = fk
                        .columns
                        .get(i)
                        .and_then(|c| cols.get(c.text.as_str()))
                        .map(|&ci| &t.columns[ci]);
                    match (to, from) {
                        (None, _) => out.push(error(
                            format!(
                                "table `{}` has no column `{}`",
                                target.name.text, named.text
                            ),
                            named.span,
                            None,
                        )),
                        (Some(to), Some(from)) if from.ty.name.text != to.ty.name.text => {
                            out.push(warning(
                                format!(
                                    "type mismatch in composite reference: `{}` and `{}`",
                                    from.name.text, named.text
                                ),
                                from.ty.span,
                                None,
                            ))
                        }
                        _ => {}
                    }
                }
                if fk.kind == RefKind::Physical
                    && let (Some(a), Some(b)) = (&t.service, &target.service)
                    && a.text != b.text
                {
                    out.push(warning(
                        "a composite foreign key crosses service boundaries".into(),
                        fk.span,
                        Some("write `~>` for a logical reference".into()),
                    ));
                }
            }
        }
    }

    // 인덱스, 유니크 이름은 보이기만 하지만 한 표 안에 같은 이름이 둘이면 모호하다
    let mut names: HashSet<String> = HashSet::new();
    let mut named = |i: Option<&Ident>, out: &mut Vec<Diagnostic>| {
        let Some(i) = i else {
            return;
        };
        if !names.insert(i.text.clone()) {
            out.push(error(
                format!("name `{}` is used twice in table `{}`", i.text, t.name.text),
                i.span,
                None,
            ));
        }
    };
    for c in &t.columns {
        named(c.uk.as_ref().and_then(|u| u.name.as_ref()), out);
        named(c.index.as_ref().and_then(|u| u.name.as_ref()), out);
    }
    for k in &t.constraints {
        named(k.name.as_ref(), out);
    }
    for k in &t.foreign_keys {
        named(k.name.as_ref(), out);
    }

    if let Some(a) = &t.audit {
        if t.external {
            out.push(error(
                "`audit` cannot be used on an external table".into(),
                a.span,
                None,
            ));
        } else if !AUDIT_METHODS.contains(&a.method.text.as_str()) {
            out.push(error(
                format!("unknown audit method `{}`", a.method.text),
                a.method.span,
                Some(format!(
                    "available methods: {}",
                    AUDIT_METHODS
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ));
        } else {
            if !t.columns.iter().any(|c| c.pk) {
                out.push(error(
                    format!(
                        "`audit` needs a primary key, and table `{}` has none",
                        t.name.text
                    ),
                    a.span,
                    None,
                ));
            }
            for i in a.columns.iter().flatten() {
                known(i, out);
                if cols
                    .get(i.text.as_str())
                    .is_some_and(|&ci| t.columns[ci].pk)
                {
                    out.push(warning(
                        format!("primary key `{}` is always part of the audit table", i.text),
                        i.span,
                        Some("you can drop it from the list".into()),
                    ));
                }
            }
        }
    }
}

fn check_ref(
    doc: &Document,
    ti: usize,
    ci: usize,
    out: &mut Vec<Diagnostic>,
    resolution: &Resolution,
) {
    let t = &doc.tables[ti];
    let c = &t.columns[ci];
    let r = c.reference.as_ref().expect("참조가 있는 컬럼만");
    let target = resolution.table(
        doc,
        &r.table.text,
        Some(ti),
        r.service.as_ref().map(|s| s.text.as_str()),
    );
    let Some(target) = target else {
        let ambiguous = r.service.is_none()
            && resolution
                .names
                .get(&r.table.text)
                .is_some_and(|m| m.len() > 1);
        out.push(error(
            if ambiguous {
                format!("referenced table `{}` is ambiguous", r.table.text)
            } else {
                format!(
                    "referenced table `{}` is not in this document",
                    r.table.text
                )
            },
            r.table.span,
            Some(if ambiguous {
                "write `service.table.column` to select its owner".into()
            } else {
                "declare it with `external table` if it lives outside this document".into()
            }),
        ));
        return;
    };
    let tt = &doc.tables[target];
    if let Some(col) = &r.column
        && !tt.columns.iter().any(|x| x.name.text == col.text)
    {
        out.push(error(
            format!("table `{}` has no column `{}`", tt.name.text, col.text),
            col.span,
            None,
        ));
        return;
    }
    if r.column.is_none() && tt.columns.iter().filter(|x| x.pk).count() != 1 {
        out.push(error(
            format!(
                "cannot pick a target column: the primary key of `{}` is not a single column",
                tt.name.text
            ),
            r.table.span,
            Some(format!("name the column, e.g. `{}.column`", tt.name.text)),
        ));
        return;
    }
    // 타입 이름만 견준다 — FK 는 길이가 섞이는 일이 흔하다(varchar(32) 와 varchar(64))
    if let Some((rt, rc)) = resolution.reference(doc, ti, ci) {
        let rcol = &doc.tables[rt].columns[rc];
        if rcol.ty.name.text != c.ty.name.text {
            out.push(warning(
                format!(
                    "type mismatch: `{}` is {} but `{}.{}` is {}",
                    c.name.text,
                    c.ty.name.text,
                    doc.tables[rt].name.text,
                    rcol.name.text,
                    rcol.ty.name.text
                ),
                c.ty.span,
                None,
            ));
        }
    }
    // 서비스마다 DB 가 따로라 FOREIGN KEY 는 다른 DB 에 닿지 못한다
    if r.kind == RefKind::Physical
        && let (Some(a), Some(b)) = (&t.service, &tt.service)
        && a.text != b.text
    {
        out.push(warning(
            format!(
                "foreign key `{}.{}` crosses from service `{}` into `{}`",
                t.name.text, c.name.text, a.text, b.text
            ),
            r.span,
            Some("a FOREIGN KEY ties the data of two services together; write `~>` for a logical reference".into()),
        ));
    }
}
