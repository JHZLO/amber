//! 검사를 지난 나무 → 모델(model.ts). 그림은 모델만 읽는다. 참조 풀기, 일대일/일대다, 부모가 없어도 되는지, 감사 표 펼치기를
//! 여기서 한 번 정한다. 자리는 버린다. 직렬화는 resin 의 JSON 과 같은 모양(픽스처와 그대로 견준다).

use serde::Serialize;

use super::checker::{AUDIT_SUFFIX, REVINFO, Resolution, audited_columns};
use super::{ConstraintKind, Document, RefKind, Table, table_id, type_text};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Model {
    /// 문서 순서(외부 표 포함), 그 뒤에 펼친 감사 표(revinfo, *_aud)
    pub tables: Vec<ModelTable>,
    /// 자식 표와 컬럼의 문서 순서, 그 뒤에 감사 관계
    pub relations: Vec<Relation>,
    pub services: Vec<ModelService>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelService {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableOrigin {
    Table,
    External,
    Audit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelAudit {
    pub method: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelForeignKey {
    pub columns: Vec<String>,
    pub target: String,
    pub target_columns: Vec<String>,
    pub kind: RefKind,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTable {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub description: Option<String>,
    pub origin: TableOrigin,
    pub columns: Vec<ModelColumn>,
    pub constraints: Vec<ModelConstraint>,
    pub audit: Option<ModelAudit>,
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_keys: Option<Vec<ModelForeignKey>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelIndex {
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelRef {
    pub table: String,
    pub column: String,
    pub kind: RefKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelColumn {
    pub name: String,
    /// 인자까지 쓴 그대로: `varchar(32)`
    #[serde(rename = "type")]
    pub ty: String,
    pub nullable: bool,
    pub pk: bool,
    pub uk: bool,
    pub uk_name: Option<String>,
    pub enc: bool,
    pub enum_values: Option<Vec<String>>,
    pub index: Option<ModelIndex>,
    pub description: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelConstraint {
    pub kind: ConstraintKind,
    pub name: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub parent: String,
    pub parent_column: String,
    pub child: String,
    pub child_column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_columns: Option<Vec<String>>,
    /// 복합 FK 의 이름 — 복합 FK 관계에만 있다(이름이 없으면 null)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<Option<String>>,
    pub kind: RefKind,
    /// 일대일 — 가리키는 컬럼이 `uk` 이거나 표의 유일한 기본 키
    pub one: bool,
    /// 부모가 없어도 된다 — 가리키는 컬럼이 널이 된다
    pub optional: bool,
    pub origin: TableOrigin,
}

fn plain(name: &str, ty: &str) -> ModelColumn {
    ModelColumn {
        name: name.into(),
        ty: ty.into(),
        nullable: false,
        pk: false,
        uk: false,
        uk_name: None,
        enc: false,
        enum_values: None,
        index: None,
        description: None,
        reference: None,
    }
}

/// 서비스 안의 표나 점이 든 이름은 이름표를 따로 든다(모델 이름은 `서비스.표`)
fn label(t: &Table, name: String) -> Option<String> {
    (t.service.is_some() || t.name.text.contains('.')).then_some(name)
}

pub fn lower(doc: &Document, resolution: &Resolution) -> Model {
    let mut tables = Vec::new();
    let mut relations = Vec::new();
    for (ti, t) in doc.tables.iter().enumerate() {
        let pk_count = t.columns.iter().filter(|c| c.pk).count();
        let fk_target = |fk: &super::ForeignKey| {
            resolution
                .table(
                    doc,
                    &fk.table.text,
                    Some(ti),
                    fk.service.as_ref().map(|s| s.text.as_str()),
                )
                .expect("검사를 지났다")
        };
        tables.push(ModelTable {
            name: table_id(t, None),
            label: label(t, t.name.text.clone()),
            description: t.description.clone(),
            origin: if t.external {
                TableOrigin::External
            } else {
                TableOrigin::Table
            },
            columns: (0..t.columns.len())
                .map(|ci| column(doc, resolution, ti, ci))
                .collect(),
            constraints: t
                .constraints
                .iter()
                .map(|k| ModelConstraint {
                    kind: k.kind,
                    name: k.name.as_ref().map(|n| n.text.clone()),
                    columns: k.columns.iter().map(|i| i.text.clone()).collect(),
                })
                .collect(),
            audit: t.audit.as_ref().map(|a| ModelAudit {
                method: a.method.text.clone(),
                columns: audited_columns(t)
                    .into_iter()
                    .map(|ci| t.columns[ci].name.text.clone())
                    .collect(),
            }),
            service: t.service.as_ref().map(|s| s.text.clone()),
            foreign_keys: (!t.foreign_keys.is_empty()).then(|| {
                t.foreign_keys
                    .iter()
                    .map(|fk| ModelForeignKey {
                        columns: fk.columns.iter().map(|c| c.text.clone()).collect(),
                        target: table_id(&doc.tables[fk_target(fk)], None),
                        target_columns: fk.target_columns.iter().map(|c| c.text.clone()).collect(),
                        kind: fk.kind,
                        name: fk.name.as_ref().map(|n| n.text.clone()),
                    })
                    .collect()
            }),
        });
        for (ci, c) in t.columns.iter().enumerate() {
            let Some((rt, rc)) = resolution.reference(doc, ti, ci) else {
                continue;
            };
            relations.push(Relation {
                parent: table_id(&doc.tables[rt], None),
                parent_column: doc.tables[rt].columns[rc].name.text.clone(),
                child: table_id(t, None),
                child_column: c.name.text.clone(),
                parent_columns: None,
                child_columns: None,
                constraint: None,
                kind: c.reference.as_ref().expect("풀린 참조").kind,
                one: c.uk.is_some() || (c.pk && pk_count == 1),
                optional: c.nullable,
                origin: TableOrigin::Table,
            });
        }
        for fk in &t.foreign_keys {
            let target = &doc.tables[fk_target(fk)];
            let cols: Vec<String> = fk.columns.iter().map(|c| c.text.clone()).collect();
            let pk: Vec<String> = t
                .columns
                .iter()
                .filter(|c| c.pk)
                .map(|c| c.name.text.clone())
                .collect();
            let same = |other: &[String]| {
                other.len() == cols.len() && cols.iter().all(|c| other.contains(c))
            };
            relations.push(Relation {
                parent: table_id(target, None),
                child: table_id(t, None),
                parent_column: fk.target_columns[0].text.clone(),
                child_column: cols[0].clone(),
                parent_columns: Some(fk.target_columns.iter().map(|c| c.text.clone()).collect()),
                child_columns: Some(cols.clone()),
                constraint: Some(fk.name.as_ref().map(|n| n.text.clone())),
                kind: fk.kind,
                one: same(&pk)
                    || t.constraints.iter().any(|k| {
                        k.kind == ConstraintKind::Unique
                            && same(&k.columns.iter().map(|c| c.text.clone()).collect::<Vec<_>>())
                    }),
                optional: t
                    .columns
                    .iter()
                    .any(|c| cols.contains(&c.name.text) && c.nullable),
                origin: TableOrigin::Table,
            });
        }
    }

    let audited: Vec<&Table> = doc
        .tables
        .iter()
        .filter(|t| t.audit.is_some() && !t.external)
        .collect();
    if !audited.is_empty() {
        let mut rev = plain("rev", "int");
        rev.pk = true;
        tables.push(ModelTable {
            name: REVINFO.into(),
            label: None,
            description: None,
            origin: TableOrigin::Audit,
            columns: vec![rev, plain("revtstmp", "bigint")],
            constraints: Vec::new(),
            audit: None,
            service: None,
            foreign_keys: None,
        });
        for t in &audited {
            tables.push(envers(t));
        }
        for t in &audited {
            relations.push(Relation {
                parent: REVINFO.into(),
                parent_column: "rev".into(),
                child: table_id(t, Some(&format!("{}{AUDIT_SUFFIX}", t.name.text))),
                child_column: "rev".into(),
                parent_columns: None,
                child_columns: None,
                constraint: None,
                kind: RefKind::Logical,
                one: false,
                optional: false,
                origin: TableOrigin::Audit,
            });
        }
    }
    Model {
        tables,
        relations,
        services: doc
            .services
            .iter()
            .map(|s| ModelService {
                name: s.name.text.clone(),
                description: s.description.clone(),
            })
            .collect(),
    }
}

fn column(doc: &Document, resolution: &Resolution, ti: usize, ci: usize) -> ModelColumn {
    let c = &doc.tables[ti].columns[ci];
    let target = resolution.reference(doc, ti, ci);
    ModelColumn {
        name: c.name.text.clone(),
        ty: type_text(&c.ty),
        nullable: c.nullable,
        pk: c.pk,
        uk: c.uk.is_some(),
        uk_name: c
            .uk
            .as_ref()
            .and_then(|u| u.name.as_ref())
            .map(|n| n.text.clone()),
        enc: c.enc,
        enum_values: c
            .enum_values
            .as_ref()
            .map(|v| v.iter().map(|e| e.text.clone()).collect()),
        index: c.index.as_ref().map(|i| ModelIndex {
            name: i.name.as_ref().map(|n| n.text.clone()),
        }),
        description: c.description.clone(),
        reference: target
            .zip(c.reference.as_ref())
            .map(|((rt, rc), r)| ModelRef {
                table: table_id(&doc.tables[rt], None),
                column: doc.tables[rt].columns[rc].name.text.clone(),
                kind: r.kind,
            }),
    }
}

/// Hibernate Envers 감사 표 — 기본 키, rev, revtype, 감사 컬럼. 지운 판에선 키 말고 다 비어 감사 컬럼은 널이 된다
fn envers(t: &Table) -> ModelTable {
    let name = format!("{}{AUDIT_SUFFIX}", t.name.text);
    let mut columns: Vec<ModelColumn> = t
        .columns
        .iter()
        .filter(|c| c.pk)
        .map(|c| {
            let mut m = plain(&c.name.text, &type_text(&c.ty));
            m.pk = true;
            m
        })
        .collect();
    let mut rev = plain("rev", "int");
    rev.pk = true;
    rev.reference = Some(ModelRef {
        table: REVINFO.into(),
        column: "rev".into(),
        kind: RefKind::Logical,
    });
    columns.push(rev);
    columns.push(plain("revtype", "tinyint"));
    columns.extend(audited_columns(t).into_iter().map(|ci| {
        let c = &t.columns[ci];
        let mut m = plain(&c.name.text, &type_text(&c.ty));
        m.nullable = true;
        m
    }));
    ModelTable {
        name: table_id(t, Some(&name)),
        label: label(t, name),
        description: None,
        origin: TableOrigin::Audit,
        columns,
        constraints: Vec::new(),
        audit: None,
        // 감사 표는 감사하는 표와 같은 서비스에 앉는다
        service: t.service.as_ref().map(|s| s.text.clone()),
        foreign_keys: None,
    }
}

/// 한 표에서 참조 steps 번 안에 닿는 부분 — 그 표들과 그 사이의 관계. 한 걸음 = 부모와 자식. 감사 관계는 걸음이 아니다.
/// 빠진 표를 가리키는 컬럼의 참조는 그대로 둔다(여전히 FK 로 읽히게). 서비스는 다 남긴다(자리가 색을 정한다)
pub fn neighbors(model: &Model, table: &str, steps: usize) -> Model {
    let mut keep: Vec<String> = vec![table.to_string()];
    for _ in 0..steps {
        let from = keep.clone();
        for r in &model.relations {
            if r.origin == TableOrigin::Table
                && (from.contains(&r.parent) || from.contains(&r.child))
            {
                for n in [&r.parent, &r.child] {
                    if !keep.contains(n) {
                        keep.push(n.clone());
                    }
                }
            }
        }
    }
    Model {
        tables: model
            .tables
            .iter()
            .filter(|t| keep.contains(&t.name))
            .cloned()
            .collect(),
        relations: model
            .relations
            .iter()
            .filter(|r| keep.contains(&r.parent) && keep.contains(&r.child))
            .cloned()
            .collect(),
        services: model.services.clone(),
    }
}
