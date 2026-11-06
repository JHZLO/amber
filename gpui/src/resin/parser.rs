//! 토큰 → 구문 나무(parser.ts). 손으로 쓴 재귀 하강 — 오류 메시지 하나하나가 resin 과 같아야 한다.
//! 한 줄(컬럼, 제약)이 틀리면 그 줄 끝까지 건너뛰고 다음 줄로 간다(LineError = Err). 틀린 줄의 노드는 나무에 넣지 않는다.

use super::lexer::{Tk, Token, lex};
use super::{
    Audit, Column, ConstraintKind, Diagnostic, Document, EnumValue, ForeignKey, Ident, Named, Ref,
    RefKind, Service, Span, Table, TableConstraint, TypeRef, error,
};

/// 그 줄을 버린다 — 진단은 이미 남겼다
struct LineError;

type R<T> = Result<T, LineError>;

const COLUMN_MODIFIERS: [&str; 8] = [
    "pk",
    "uk",
    "enc",
    "enum",
    "index",
    "->",
    "~>",
    "\"description\"",
];
const MODIFIER_WORDS: [&str; 6] = ["pk", "uk", "enc", "enum", "index", "as"];

pub fn parse(source: &str) -> (Document, Vec<Diagnostic>) {
    let (tokens, mut diagnostics) = lex(source);
    let mut p = Parser {
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let doc = p.document();
    diagnostics.extend(p.diagnostics);
    (doc, diagnostics)
}

fn describe(t: &Token) -> String {
    match t.kind {
        Tk::Str => "a string".into(),
        Tk::Newline => "end of line".into(),
        Tk::Eof => "end of file".into(),
        _ => format!("`{}`", t.value),
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    // ---- 토큰 ----

    fn peek(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }
    fn next(&mut self) -> Token {
        let t = self.peek(0).clone();
        if t.kind != Tk::Eof {
            self.pos += 1;
        }
        t
    }
    fn at(&self, kind: Tk) -> bool {
        self.peek(0).kind == kind
    }
    fn at_n(&self, kind: Tk, offset: usize) -> bool {
        self.peek(offset).kind == kind
    }
    /// 이 키워드인가 — 백틱 이름은 키워드가 아니다
    fn at_word(&self, word: &str, offset: usize) -> bool {
        let t = self.peek(offset);
        t.kind == Tk::Ident && !t.quoted && t.value == word
    }
    fn skip_newlines(&mut self) {
        while self.at(Tk::Newline) {
            self.next();
        }
    }
    /// 줄 끝까지 버린다(줄바꿈은 남긴다). `}` 에서도 멈춘다 — 블록 끝을 먹으면 복구가 나빠진다
    fn skip_line(&mut self) {
        while !self.at(Tk::Newline) && !self.at(Tk::Eof) && !self.at(Tk::RBrace) {
            self.next();
        }
    }

    fn fail<T>(&mut self, message: String, span: Span, hint: Option<String>) -> R<T> {
        self.diagnostics.push(error(message, span, hint));
        Err(LineError)
    }
    fn expect(&mut self, kind: Tk, what: &str) -> R<Token> {
        let t = self.peek(0).clone();
        if t.kind == kind {
            return Ok(self.next());
        }
        self.fail(
            format!("expected {what}, found {}", describe(&t)),
            t.span,
            None,
        )
    }
    /// 이름 — 맨이름이나 백틱
    fn name(&mut self, what: &str) -> R<Ident> {
        let t = self.expect(Tk::Ident, what)?;
        Ok(Ident {
            text: t.value,
            span: t.span,
            quoted: t.quoted,
        })
    }

    // ---- 문서 ----

    fn document(&mut self) -> Document {
        let mut tables = Vec::new();
        let mut services = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(Tk::Eof) {
                break;
            }
            let r = if self.at_service(0) {
                self.service(&mut tables).map(|s| services.push(s))
            } else if self.at_table() {
                self.table_or_external(None).map(|t| tables.push(t))
            } else if self.at_word("erd", 0) {
                let sp = self.peek(0).span;
                self.fail(
                    "the `erd` header was removed in v0.2".into(),
                    sp,
                    Some("delete this line".into()),
                )
            } else {
                let t = self.peek(0).clone();
                self.fail(
                    format!("unexpected {}", describe(&t)),
                    t.span,
                    Some(
                        "only `service`, `table` and `external table` can appear at the top level"
                            .into(),
                    ),
                )
            };
            if r.is_err() {
                self.recover_top_level();
            }
        }
        Document { tables, services }
    }

    /// 꼭대기 오류 뒤 — `service`, `table`, `external` 로 시작하는 다음 줄까지
    fn recover_top_level(&mut self) {
        while !self.at(Tk::Eof) {
            if self.at(Tk::Newline)
                && (self.at_service(1) || self.at_word("table", 1) || self.at_word("external", 1))
            {
                return;
            }
            self.next();
        }
    }

    fn at_table(&self) -> bool {
        self.at_word("table", 0) || self.at_word("external", 0)
    }

    /// `group` 은 하루 동안 `service` 의 이름이었다 — 아직 그렇게 읽고 오류로 알려 준다
    fn at_service(&self, offset: usize) -> bool {
        self.at_word("service", offset) || self.at_word("group", offset)
    }

    fn table_or_external(&mut self, service: Option<Ident>) -> R<Table> {
        if self.at_word("table", 0) {
            return self.table(None, service);
        }
        let ext = self.next();
        if !self.at_word("table", 0) {
            let sp = self.peek(0).span;
            self.fail::<()>(
                "expected `table` after `external`".into(),
                sp,
                Some("write `external table name { ... }`".into()),
            )?;
        }
        self.table(Some(ext), service)
    }

    // ---- 서비스 ----

    fn service(&mut self, tables: &mut Vec<Table>) -> R<Service> {
        let kw = self.next();
        if kw.value == "group" {
            let next = self.peek(0);
            let hint = if next.kind == Tk::Ident {
                format!("write `service {} {{ ... }}`", next.value)
            } else {
                "write `service name { ... }`".into()
            };
            self.diagnostics.push(error(
                "`group` is now `service`".into(),
                kw.span,
                Some(hint),
            ));
        }
        let name = self.name("a service name")?;
        let description = self.at(Tk::Str).then(|| self.next().value);
        self.expect(Tk::LBrace, "`{`")?;
        loop {
            self.skip_newlines();
            if self.at(Tk::RBrace) {
                break;
            }
            if self.at(Tk::Eof) {
                self.diagnostics.push(error(
                    format!("service `{}` is missing its closing `}}`", name.text),
                    name.span,
                    None,
                ));
                return Ok(Service {
                    name,
                    description,
                    span: kw.span,
                });
            }
            let r = if self.at_table() {
                self.table_or_external(Some(name.clone()))
                    .map(|t| tables.push(t))
            } else if self.at_service(0) {
                let sp = self.peek(0).span;
                self.diagnostics.push(error(
                    "services cannot be nested".into(),
                    sp,
                    Some(format!(
                        "close `{}` before opening another service",
                        name.text
                    )),
                ));
                self.skip_block();
                Ok(())
            } else {
                let t = self.peek(0).clone();
                self.fail(
                    format!(
                        "expected `table` or `external table` in service `{}`, found {}",
                        name.text,
                        describe(&t)
                    ),
                    t.span,
                    None,
                )
            };
            if r.is_err() {
                self.skip_line();
            }
        }
        self.next(); // }
        self.end_of_statement()?;
        Ok(Service {
            name,
            description,
            span: kw.span,
        })
    }

    /// 한 줄을 건너뛰고, 블록을 열었으면 블록 통째로
    fn skip_block(&mut self) {
        let mut depth: i32 = 0;
        while !self.at(Tk::Eof) {
            let t = self.next();
            match t.kind {
                Tk::LBrace => depth += 1,
                Tk::RBrace => {
                    depth -= 1;
                    if depth <= 0 {
                        return;
                    }
                }
                Tk::Newline if depth == 0 => return,
                _ => {}
            }
        }
    }

    fn end_of_statement(&mut self) -> R<()> {
        if self.at(Tk::Newline) || self.at(Tk::Eof) {
            return Ok(());
        }
        let t = self.peek(0).clone();
        self.fail(
            format!("expected end of line, found {}", describe(&t)),
            t.span,
            None,
        )
    }

    // ---- 표 ----

    fn table(&mut self, external: Option<Token>, service: Option<Ident>) -> R<Table> {
        let kw = self.next(); // table
        let span = external.as_ref().map_or(kw.span, |e| e.span);
        let name = self.name("a table name")?;
        let description = self.at(Tk::Str).then(|| self.next().value);
        self.expect(Tk::LBrace, "`{`")?;

        let mut columns = Vec::new();
        let mut constraints = Vec::new();
        let mut foreign_keys = Vec::new();
        let done = |columns, constraints, foreign_keys, audit, name: Ident| Table {
            name,
            external: external.is_some(),
            description: description.clone(),
            columns,
            constraints,
            foreign_keys,
            audit,
            service: service.clone(),
            span,
        };
        loop {
            self.skip_newlines();
            if self.at(Tk::RBrace) {
                break;
            }
            if self.at(Tk::Eof) {
                self.diagnostics.push(error(
                    format!("table `{}` is missing its closing `}}`", name.text),
                    name.span,
                    None,
                ));
                return Ok(done(columns, constraints, foreign_keys, None, name));
            }
            let r = if self.at_word("foreign", 0) && self.at_n(Tk::LParen, 1) {
                self.foreign_key().map(|f| foreign_keys.push(f))
            } else if self.at_constraint() {
                self.constraint().map(|k| constraints.push(k))
            } else {
                self.reject_old_constraint()
                    .and_then(|_| self.column())
                    .map(|c| columns.push(c))
            };
            if r.is_err() {
                self.skip_line();
            }
        }
        self.next(); // }
        let audit = if self.at_word("audit", 0) {
            Some(self.audit()?)
        } else {
            None
        };
        self.end_of_statement()?;
        Ok(done(columns, constraints, foreign_keys, audit, name))
    }

    /// `unique(` 나 `index(` — 키워드 바로 뒤가 `(` 일 때만 제약이다(컬럼 이름이 `index` 일 수 있다)
    fn at_constraint(&self) -> bool {
        (self.at_word("unique", 0) || self.at_word("index", 0)) && self.at_n(Tk::LParen, 1)
    }

    /// v0.1 의 `unique name(a, b)` — 괄호 안이 숫자가 아니라 이름이면 옛 제약이다
    fn reject_old_constraint(&mut self) -> R<()> {
        if (self.at_word("unique", 0) || self.at_word("index", 0))
            && self.at_n(Tk::Ident, 1)
            && self.at_n(Tk::LParen, 2)
            && self.at_n(Tk::Ident, 3)
        {
            let kw = self.peek(0).clone();
            let hint = format!(
                "write `{}({}, ...) as {}`",
                kw.value,
                self.peek(3).value,
                self.peek(1).value
            );
            return self.fail(
                "constraint names go after the column list since v0.2".into(),
                kw.span,
                Some(hint),
            );
        }
        Ok(())
    }

    fn constraint(&mut self) -> R<TableConstraint> {
        let kw = self.next();
        let columns = self.list("a column name")?;
        if columns.is_empty() {
            self.fail::<()>("list at least one column".into(), kw.span, None)?;
        }
        let mut name = None;
        if self.at_word("as", 0) {
            self.next();
            name = Some(self.name("a constraint name")?);
        }
        self.end_of_member()?;
        Ok(TableConstraint {
            kind: if kw.value == "unique" {
                ConstraintKind::Unique
            } else {
                ConstraintKind::Index
            },
            name,
            columns,
            span: kw.span,
        })
    }

    fn audit(&mut self) -> R<Audit> {
        let a = self.next();
        if self.at(Tk::LParen) || self.at(Tk::Newline) || self.at(Tk::Eof) {
            self.fail::<()>(
                "`audit` needs a method".into(),
                a.span,
                Some("write `audit envers(a, b)` or `audit envers` (changed in v0.2)".into()),
            )?;
        }
        let method = self.name("an audit method")?;
        let columns = if self.at(Tk::LParen) {
            Some(self.list("a column name to audit")?)
        } else {
            None
        };
        Ok(Audit {
            method,
            columns,
            span: a.span,
        })
    }

    fn foreign_key(&mut self) -> R<ForeignKey> {
        let kw = self.next();
        let columns = self.list("a source column name")?;
        let arrow = self.peek(0).clone();
        if !self.at(Tk::Arrow) && !self.at(Tk::TildeArrow) {
            self.fail::<()>("expected `->` or `~>`".into(), arrow.span, None)?;
        }
        self.next();
        let mut table = self.name("a referenced table name")?;
        let mut service = None;
        if self.at(Tk::Dot) {
            self.next();
            service = Some(table);
            table = self.name("a referenced table name")?;
        }
        let target_columns = self.list("a target column name")?;
        let name = self.optional_as("a foreign key name")?;
        self.end_of_member()?;
        Ok(ForeignKey {
            columns,
            table,
            service,
            target_columns,
            name,
            kind: if arrow.kind == Tk::Arrow {
                RefKind::Physical
            } else {
                RefKind::Logical
            },
            span: kw.span,
        })
    }

    // ---- 컬럼 ----

    fn column(&mut self) -> R<Column> {
        let name = self.name("a column name")?;
        if self.at(Tk::Newline) || self.at(Tk::RBrace) || self.at(Tk::Eof) {
            self.fail::<()>(
                format!("column `{}` has no type", name.text),
                name.span,
                Some("a column is written `name type`, e.g. `id bigint pk`".into()),
            )?;
        }
        let ty = self.type_ref()?;
        let nullable = self.at(Tk::Question);
        if nullable {
            self.next();
        }
        let mut col = Column {
            span: name.span,
            name,
            ty,
            nullable,
            pk: false,
            uk: None,
            enc: false,
            enum_values: None,
            index: None,
            reference: None,
            description: None,
        };
        // 앞 수식어가 이름을 받을 수 있었다(uk, index) — 뒤에 맨이름이 오면 고치는 법을 알려 준다
        let mut named: Option<&str> = None;
        while !self.at(Tk::Newline) && !self.at(Tk::Eof) && !self.at(Tk::RBrace) {
            let t = self.peek(0).clone();
            let after = named.take();
            let dup =
                |p: &mut Self, what: &str| p.fail::<()>(format!("duplicate {what}"), t.span, None);
            if t.kind == Tk::Str {
                if col.description.is_some() {
                    dup(self, "description")?;
                }
                col.description = Some(self.next().value);
            } else if t.kind == Tk::Arrow || t.kind == Tk::TildeArrow {
                if col.reference.is_some() {
                    dup(self, "reference")?;
                }
                col.reference = Some(self.reference()?);
            } else if self.at_word("pk", 0) {
                if col.pk {
                    dup(self, "`pk`")?;
                }
                self.next();
                col.pk = true;
            } else if self.at_word("enc", 0) {
                if col.enc {
                    dup(self, "`enc`")?;
                }
                self.next();
                col.enc = true;
            } else if self.at_word("uk", 0) {
                if col.uk.is_some() {
                    dup(self, "`uk`")?;
                }
                self.next();
                let name = self.optional_as("a unique constraint name")?;
                if name.is_none() {
                    named = Some("uk");
                }
                col.uk = Some(Named { name, span: t.span });
            } else if self.at_word("index", 0) {
                if col.index.is_some() {
                    dup(self, "`index`")?;
                }
                self.next();
                if self.at(Tk::LParen) {
                    let (sp, v) = (self.peek(0).span, self.peek(1).value.clone());
                    self.fail::<()>(
                        "index names go after `as` since v0.2".into(),
                        sp,
                        Some(format!("write `index as {v}`")),
                    )?;
                }
                let name = self.optional_as("an index name")?;
                if name.is_none() {
                    named = Some("index");
                }
                col.index = Some(Named { name, span: t.span });
            } else if self.at_word("enum", 0) {
                if col.enum_values.is_some() {
                    dup(self, "`enum`")?;
                }
                self.next();
                let values = self.values("an enum value")?;
                if values.is_empty() {
                    self.fail::<()>("list at least one enum value".into(), t.span, None)?;
                }
                col.enum_values = Some(values);
            } else if self.at_word("as", 0) {
                self.fail::<()>("`as` can only follow `uk` or `index`".into(), t.span, None)?;
            } else if let Some(after) =
                after.filter(|_| t.kind == Tk::Ident && !MODIFIER_WORDS.contains(&t.value.as_str()))
            {
                self.fail::<()>(
                    format!("unknown modifier {}", describe(&t)),
                    t.span,
                    Some(format!("to name it, write `{after} as {}`", t.value)),
                )?;
            } else {
                self.fail::<()>(
                    format!("unknown modifier {}", describe(&t)),
                    t.span,
                    Some(format!("a column accepts {}", COLUMN_MODIFIERS.join(", "))),
                )?;
            }
        }
        Ok(col)
    }

    fn type_ref(&mut self) -> R<TypeRef> {
        let t = self.peek(0).clone();
        if t.kind == Tk::Ident && t.quoted {
            self.fail::<()>("a type cannot be a backtick name".into(), t.span, None)?;
        }
        let name = self.name("a type")?;
        let mut args = None;
        let mut end = name.span.col + name.span.len;
        if self.at(Tk::LParen) {
            self.next();
            let mut a = Vec::new();
            loop {
                let n = self.peek(0).clone();
                if n.kind != Tk::Number {
                    self.fail::<()>(
                        format!("type arguments must be numbers, found {}", describe(&n)),
                        n.span,
                        Some("e.g. `varchar(32)`, `decimal(12,2)`".into()),
                    )?;
                }
                // Number("0042") — 자릿수가 많으면 f64 로 굳는다
                a.push(self.next().value.parse::<f64>().unwrap_or(f64::NAN));
                if self.at(Tk::Comma) {
                    self.next();
                    continue;
                }
                let close = self.expect(Tk::RParen, "`)`")?;
                end = close.span.col + close.span.len;
                break;
            }
            args = Some(a);
        }
        Ok(TypeRef {
            span: Span {
                line: name.span.line,
                col: name.span.col,
                len: end - name.span.col,
            },
            name,
            args,
        })
    }

    /// `as` 뒤의 이름 — `as` 가 없으면 None
    fn optional_as(&mut self, what: &str) -> R<Option<Ident>> {
        if !self.at_word("as", 0) {
            return Ok(None);
        }
        self.next();
        Ok(Some(self.name(what)?))
    }

    fn reference(&mut self) -> R<Ref> {
        let arrow = self.next();
        let mut table = self.name("a referenced table name")?;
        let mut service = None;
        let mut column = None;
        if self.at(Tk::Dot) {
            self.next();
            column = Some(self.name("a referenced column name")?);
            if self.at(Tk::Dot) {
                self.next();
                service = Some(table);
                table = column.take().expect("방금 읽었다");
                column = Some(self.name("a referenced column name")?);
            }
        }
        Ok(Ref {
            kind: if arrow.kind == Tk::Arrow {
                RefKind::Physical
            } else {
                RefKind::Logical
            },
            table,
            service,
            column,
            span: arrow.span,
        })
    }

    // ---- 공용 ----

    fn end_of_member(&mut self) -> R<()> {
        if self.at(Tk::Newline) || self.at(Tk::Eof) || self.at(Tk::RBrace) {
            return Ok(());
        }
        let t = self.peek(0).clone();
        self.fail(
            format!("expected end of line, found {}", describe(&t)),
            t.span,
            None,
        )
    }

    /// `( a, b, c )` — 괄호 안 줄바꿈은 무시, 끝의 쉼표 허용
    fn list(&mut self, what: &str) -> R<Vec<Ident>> {
        self.parenthesized(|p| p.name(what))
    }

    /// enum 값 — 이름이나 숫자
    fn values(&mut self, what: &str) -> R<Vec<EnumValue>> {
        self.parenthesized(|p| {
            let t = p.peek(0).clone();
            if t.kind == Tk::Number || t.kind == Tk::Ident {
                let v = p.next();
                return Ok(EnumValue {
                    text: v.value,
                    span: t.span,
                });
            }
            p.fail(
                format!("expected {what}, found {}", describe(&t)),
                t.span,
                None,
            )
        })
    }

    fn parenthesized<T>(&mut self, mut item: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        self.expect(Tk::LParen, "`(`")?;
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(Tk::RParen) {
                break;
            }
            items.push(item(self)?);
            self.skip_newlines();
            if self.at(Tk::Comma) {
                self.next();
                continue;
            }
            if !self.at(Tk::RParen) {
                let t = self.peek(0).clone();
                self.fail::<()>(
                    format!("expected `,` or `)`, found {}", describe(&t)),
                    t.span,
                    None,
                )?;
            }
        }
        self.next(); // )
        Ok(items)
    }
}
