//! resin(ERD 언어, github.com/JHZLO/resin) 의 Rust 이식 — amber 가 쓰는 0.2.7 그대로(PLAN D4). 원문 → 토큰(lexer) → 구문 나무(parser)
//! → 검사(checker) → 모델(model). 그리기(toSvg)는 svg.rs, 배치는 elk.rs(JavaScriptCore 의 elkjs).
//! 자리(span)는 JS 처럼 UTF-16 단위로 센다 — 오류 줄의 `줄:열` 이 지금 앱과 같아야 한다.
//! 기준은 gpui/tools/resin-fixture.mjs 가 고정 resin 으로 만든 tests/resin_compile.json(resin 시험 파일의 문자열 전부).

mod checker;
mod lexer;
mod model;
mod parser;
pub mod sql;

pub use model::{
    Model, ModelAudit, ModelColumn, ModelConstraint, ModelForeignKey, ModelIndex, ModelRef,
    ModelService, ModelTable, Relation, TableOrigin, neighbors,
};

use serde::Serialize;

/// 원문의 자리 — 줄과 열은 1부터, 길이는 UTF-16 단위(0 = 점). 여러 줄 노드는 첫 줄만 든다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

pub(crate) fn error(message: String, span: Span, hint: Option<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message,
        span,
        hint,
    }
}

pub(crate) fn warning(message: String, span: Span, hint: Option<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        message,
        span,
        hint,
    }
}

/// 정규식 \p{L}, \p{N} — JS 의 /u 판정과 같은 일반 범주
pub(crate) fn is_letter(c: char) -> bool {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^\p{L}$").unwrap());
    RE.is_match(c.encode_utf8(&mut [0; 4]))
}

pub(crate) fn is_number(c: char) -> bool {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^\p{N}$").unwrap());
    RE.is_match(c.encode_utf8(&mut [0; 4]))
}

pub fn has_errors(ds: &[Diagnostic]) -> bool {
    ds.iter().any(|d| d.severity == Severity::Error)
}

// ---- 구문 나무(ast.ts) ----

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
    /// 백틱으로 썼다 — 키워드로 읽지 않는다
    pub quoted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    /// 문서 순서 — 서비스 안의 표도
    pub tables: Vec<Table>,
    pub services: Vec<Service>,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: Ident,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: Ident,
    pub external: bool,
    pub description: Option<String>,
    pub columns: Vec<Column>,
    pub constraints: Vec<TableConstraint>,
    pub foreign_keys: Vec<ForeignKey>,
    pub audit: Option<Audit>,
    pub service: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub name: Ident,
    /// `varchar(32)`, `decimal(12,2)` 의 인자 — JS 의 Number 라 f64
    pub args: Option<Vec<f64>>,
    pub span: Span,
}

/// `uk [as 이름]`, `index [as 이름]`
#[derive(Debug, Clone)]
pub struct Named {
    pub name: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: Ident,
    pub ty: TypeRef,
    pub nullable: bool,
    pub pk: bool,
    pub uk: Option<Named>,
    pub enc: bool,
    pub enum_values: Option<Vec<EnumValue>>,
    pub index: Option<Named>,
    pub reference: Option<Ref>,
    pub description: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumValue {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    /// `->` FOREIGN KEY
    Physical,
    /// `~>` 앱이 지키는 참조
    Logical,
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub kind: RefKind,
    pub table: Ident,
    pub service: Option<Ident>,
    /// 생략하면 검사기가 대상의 단일 기본 키로 푼다
    pub column: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub columns: Vec<Ident>,
    pub table: Ident,
    pub service: Option<Ident>,
    pub target_columns: Vec<Ident>,
    pub kind: RefKind,
    pub name: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintKind {
    Unique,
    Index,
}

#[derive(Debug, Clone)]
pub struct TableConstraint {
    pub kind: ConstraintKind,
    pub name: Option<Ident>,
    pub columns: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Audit {
    pub method: Ident,
    /// 목록 없이 쓰면 None — 기본 키, created_at, updated_at 을 뺀 모든 컬럼
    pub columns: Option<Vec<Ident>>,
    pub span: Span,
}

/// JS 의 String(number) — 타입 인자는 정수가 대부분이지만 아주 긴 숫자는 지수로 바뀐다
fn js_number(v: f64) -> String {
    if v.fract() == 0. && v.abs() < 1e21 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:e}");
        match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        }
    }
}

/// `varchar(32)` — 쓴 그대로의 타입
pub fn type_text(t: &TypeRef) -> String {
    match &t.args {
        Some(a) => format!(
            "{}({})",
            t.name.text,
            a.iter()
                .map(|n| js_number(*n))
                .collect::<Vec<_>>()
                .join(",")
        ),
        None => t.name.text.clone(),
    }
}

pub fn quote_name(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        name.to_string()
    } else {
        format!("`{name}`")
    }
}

/// 모델 안의 표 이름 — 서비스가 있으면 `서비스.표`. 이름표는 선언한 이름 그대로
pub fn table_id(t: &Table, name: Option<&str>) -> String {
    let name = name.unwrap_or(&t.name.text);
    match &t.service {
        Some(s) => format!("{}.{}", quote_name(&s.text), quote_name(name)),
        None if name.contains('.') => quote_name(name),
        None => name.to_string(),
    }
}

pub struct CompileResult {
    pub doc: Document,
    /// 오류가 하나라도 있으면 None
    pub model: Option<Model>,
    /// 원문 순서
    pub diagnostics: Vec<Diagnostic>,
}

/// 원문 하나를 파싱 → 검사 → (오류가 없으면) 모델로
pub fn compile(source: &str) -> CompileResult {
    let (doc, mut diagnostics) = parser::parse(source);
    let resolution = checker::Resolution::new(&doc);
    // 문법 오류가 있으면 나무가 덜 지어져 있다 — 검사하면 엉뚱한 오류("그런 표 없음")가 붙는다
    if !has_errors(&diagnostics) {
        diagnostics.extend(checker::check(&doc, &resolution));
    }
    let model = (!has_errors(&diagnostics)).then(|| model::lower(&doc, &resolution));
    // 위치 순서로 — 같은 자리는 들어온 순서(안정 정렬)
    diagnostics.sort_by(|a, b| (a.span.line, a.span.col).cmp(&(b.span.line, b.span.col)));
    CompileResult {
        doc,
        model,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn compiles_like_resin() {
        let cases: Value =
            serde_json::from_str(include_str!("../../tests/resin_compile.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert!(cases.len() > 250);
        for c in cases {
            let source = c["source"].as_str().unwrap();
            let r = super::compile(source);
            let diags = serde_json::to_value(&r.diagnostics).unwrap();
            assert_eq!(diags, c["diagnostics"], "진단\n{source}");
            let model = serde_json::to_value(&r.model).unwrap();
            assert_eq!(model, c["model"], "모델\n{source}");
            for n in c["near"].as_array().unwrap() {
                let near = super::neighbors(
                    r.model.as_ref().unwrap(),
                    n["table"].as_str().unwrap(),
                    n["steps"].as_u64().unwrap() as usize,
                );
                assert_eq!(
                    serde_json::to_value(&near).unwrap(),
                    n["model"],
                    "이웃\n{source}"
                );
            }
        }
    }

    #[test]
    fn numbers_print_like_js() {
        assert_eq!(super::js_number(32.), "32");
        assert_eq!(super::js_number(1e20), "100000000000000000000");
        assert_eq!(super::js_number(1e21), "1e+21");
    }
}
