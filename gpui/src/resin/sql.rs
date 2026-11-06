//! 구조화된 스키마 → resin 원문(sql.ts 의 fromSchema). DB 들여다보기(information_schema)가 준 사실을 SQL 가져오기와 같은
//! 규칙으로 쓴다 — 같은 스키마는 어디서든 같은 resin 이 된다(docs/SPEC.md §8). 스키마 입력은 'table' 문장만 만들어서,
//! SQL 쪽의 DROP, ALTER, COMMENT ON, enum 타입, 뷰 건너뛰기, 이름 추론(inferReferences), known 은 이 길에 없다.
//! 타입 글자(`bigint unsigned`, `enum('A','B')`)는 SQL 토크나이저와 타입 읽기(sql-lexer.ts, sql-parser.ts)를 그대로 거친다.
//! JS 처럼 UTF-16 단위로 센다(정렬 폭, 메시지의 첫 글자).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::RefKind;

// ---- 입력과 출력 ----

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaInput {
    pub tables: Vec<SchemaTable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaTable {
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    pub columns: Vec<SchemaColumn>,
    #[serde(default)]
    pub primary_key: Option<Vec<String>>,
    #[serde(default)]
    pub indexes: Option<Vec<SchemaIndex>>,
    #[serde(default)]
    pub foreign_keys: Option<Vec<SchemaForeignKey>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaColumn {
    pub name: String,
    /// 데이터베이스가 쓰는 그대로: `varchar(255)`, `bigint unsigned`, `enum('A','B')`
    #[serde(rename = "type")]
    pub ty: String,
    pub nullable: bool,
    #[serde(default)]
    pub comment: Option<String>,
    /// 타입에 없는 허용 값(값 목록 CHECK)
    #[serde(default)]
    pub values: Option<Vec<String>>,
    #[serde(default)]
    pub encrypted: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaIndex {
    #[serde(default)]
    pub name: Option<String>,
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaForeignKey {
    #[serde(default)]
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    /// 짝지은 대상 컬럼. 빠지면 기본 키를 가리킨다
    #[serde(default)]
    pub references: Option<Vec<String>>,
    #[serde(default)]
    pub kind: Option<RefKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SqlNote {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SqlImport {
    /// resin 원문 — 표가 없으면 빈 글
    pub source: String,
    pub notes: Vec<SqlNote>,
    /// 옮긴 표 수(참조 때문에 더한 외부 표는 빼고)
    pub tables: usize,
}

// ---- JS 흉내 ----

fn u16len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JS 정규식의 \s, String.prototype.trim 이 보는 공백
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(js_space)
}

/// JSON.stringify(문자열)
fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("문자열은 늘 직렬화된다")
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn clean_name(s: &str) -> String {
    let r: String = s
        .chars()
        .map(|c| {
            if matches!(c, '`' | '\r' | '\n') {
                '_'
            } else {
                c
            }
        })
        .collect();
    if r.is_empty() { "_".into() } else { r }
}

/// resin 이름 — 될 수 있으면 맨이름, 아니면 백틱
pub fn resin_name(s: &str) -> String {
    if is_ident(s) {
        s.to_string()
    } else {
        format!("`{}`", clean_name(s))
    }
}

fn resin_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn one_line(s: Option<&str>) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in s.unwrap_or("").chars() {
        if js_space(c) {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// 고정폭 편집기의 칸 — 한글, 한자 같은 넓은 글자는 두 칸(format.ts 의 cells)
fn cells(s: &str) -> usize {
    let wide = s
        .chars()
        .filter(|c| {
            matches!(*c,
                '\u{1100}'..='\u{115f}'
                    | '\u{2e80}'..='\u{a4cf}'
                    | '\u{ac00}'..='\u{d7a3}'
                    | '\u{f900}'..='\u{faff}'
                    | '\u{fe30}'..='\u{fe4f}'
                    | '\u{ff00}'..='\u{ff60}'
                    | '\u{ffe0}'..='\u{ffe6}')
        })
        .count();
    u16len(s) + wide
}

/// 읽기 좋은 이름을 먼저 맡아 둔다 — 앞 이름을 다듬다 뒤 이름의 자리를 빼앗지 않게
struct Names {
    reserved: HashSet<String>,
    used: HashSet<String>,
}

impl Names {
    fn new<'a>(
        names: impl IntoIterator<Item = &'a str>,
        occupied: impl IntoIterator<Item = String>,
    ) -> Self {
        Names {
            reserved: names
                .into_iter()
                .filter(|n| clean_name(n) == *n)
                .map(str::to_string)
                .collect(),
            used: occupied.into_iter().collect(),
        }
    }

    fn take(&mut self, name: &str) -> String {
        let base = clean_name(name);
        let mut result = base.clone();
        if self.used.contains(&result) || (base != name && self.reserved.contains(&base)) {
            let mut suffix = 2;
            loop {
                result = format!("{base}_{suffix}");
                suffix += 1;
                if !self.used.contains(&result) && !self.reserved.contains(&result) {
                    break;
                }
            }
        }
        self.used.insert(result.clone());
        result
    }
}

// ---- 타입(sql-lexer.ts + sql-parser.ts 의 type) ----

#[derive(Debug, Clone, PartialEq)]
pub struct SqlType {
    /// 소문자, resin 이름 하나로: `character varying` → `varchar`
    pub name: String,
    pub args: Option<Vec<f64>>,
    /// MySQL `enum('a', 'b')` 의 값
    pub values: Option<Vec<String>>,
}

fn type_text(t: &SqlType) -> String {
    match &t.args {
        Some(a) => format!(
            "{}({})",
            t.name,
            a.iter()
                .map(|n| super::js_number(*n))
                .collect::<Vec<_>>()
                .join(",")
        ),
        None => t.name.clone(),
    }
}

/// 데이터베이스가 따로 적어 준 타입 하나를 컬럼 타입처럼 정규화한다. 빈 글은 `any`
pub fn parse_type(text: &str) -> SqlType {
    match lexer::tokenize(text).into_iter().next() {
        Some(tokens) if !tokens.is_empty() => read_type(&mut Cursor { tokens, pos: 0 }),
        _ => SqlType {
            name: "any".into(),
            args: None,
            values: None,
        },
    }
}

mod lexer {
    //! SQL 글 → 문장마다 토큰(sql-lexer.ts). 방언을 묻지 않고 한 번에 읽는다. 여기서 쓰는 건 타입 글자 하나지만
    //! 규칙은 그대로 옮긴다 — 이상한 타입 글자도 지금 앱과 같게 읽혀야 한다.

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Kind {
        Word,
        Quoted,
        Str,
        Number,
        Punct,
    }

    #[derive(Debug, Clone)]
    pub struct Token {
        pub kind: Kind,
        pub value: String,
        /// 낱말의 대문자 — 다른 종류는 ""
        pub upper: String,
    }

    const SP: u16 = b' ' as u16;

    fn is_ascii_letter(c: u16) -> bool {
        (65..=90).contains(&c) || (97..=122).contains(&c)
    }
    fn is_digit(c: u16) -> bool {
        (48..=57).contains(&c)
    }
    fn unit_char(c: u16) -> Option<char> {
        char::from_u32(c as u32)
    }
    fn is_word_start(t: &[u16], i: usize) -> bool {
        let c = t[i];
        is_ascii_letter(c)
            || c == 95
            || c == 64
            || (c > 127 && unit_char(c).is_some_and(super::super::is_letter))
    }
    fn is_word_part(t: &[u16], i: usize) -> bool {
        let c = t[i];
        is_ascii_letter(c)
            || is_digit(c)
            || c == 95
            || c == 36
            || c == 35
            || c == 64
            || (c > 127
                && unit_char(c)
                    .is_some_and(|ch| super::super::is_letter(ch) || super::super::is_number(ch)))
    }

    fn eq_ci(a: &[u16], word: &str) -> bool {
        let w: Vec<u16> = word.encode_utf16().collect();
        a.len() == w.len()
            && a.iter()
                .zip(&w)
                .all(|(x, y)| ascii_upper(*x) == ascii_upper(*y))
    }
    fn ascii_upper(c: u16) -> u16 {
        if (97..=122).contains(&c) { c - 32 } else { c }
    }
    fn js_space(c: u16) -> bool {
        unit_char(c).is_some_and(super::js_space)
    }

    /// pg_dump 의 `COPY ... FROM stdin` 뒤 데이터 줄을 비운다(줄 번호는 그대로)
    fn blank_copy_data(source: &str) -> String {
        let starts_copy = |l: &str| {
            l.len() >= 4
                && l[..4].eq_ignore_ascii_case("COPY")
                && !l[4..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
        };
        if !source.split('\n').any(starts_copy) {
            return source.to_string();
        }
        let mut in_data = false;
        source
            .split('\n')
            .map(|line| {
                if in_data {
                    if line.strip_suffix('\r').unwrap_or(line) == "\\." {
                        in_data = false;
                    }
                    String::new()
                } else {
                    let low = line.to_lowercase();
                    if starts_copy(line)
                        && low
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .windows(2)
                            .any(|w| w[0].ends_with("from") && w[1].starts_with("stdin"))
                    {
                        in_data = true;
                    }
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn tokenize(source: &str) -> Vec<Vec<Token>> {
        let text: Vec<u16> = blank_copy_data(source).encode_utf16().collect();
        let n = text.len();
        let backslash_escapes = text.contains(&(b'`' as u16));
        let at = |k: usize| text.get(k).copied();
        let s = |a: usize, b: usize| String::from_utf16_lossy(&text[a..b.min(n)]);

        let mut statements: Vec<Vec<Token>> = Vec::new();
        let mut current: Vec<Token> = Vec::new();
        let mut depth: i32 = 0;
        let end_statement =
            |current: &mut Vec<Token>, depth: &mut i32, statements: &mut Vec<Vec<Token>>| {
                if !current.is_empty() {
                    statements.push(std::mem::take(current));
                }
                *depth = 0;
            };
        let push = |current: &mut Vec<Token>, kind: Kind, value: String| {
            let upper = if kind == Kind::Word {
                value.to_uppercase()
            } else {
                String::new()
            };
            current.push(Token { kind, value, upper });
        };
        let at_line_start = |i: usize| {
            for k in (0..i).rev() {
                let c = text[k];
                if c == b'\n' as u16 {
                    return true;
                }
                if c != SP && c != b'\t' as u16 && c != b'\r' as u16 {
                    return false;
                }
            }
            true
        };
        let line_end = |i: usize| (i..n).find(|&k| text[k] == b'\n' as u16).unwrap_or(n);
        // 닫는 글자로 닫히는 구간 — 겹쳐 쓴 닫는 글자는 그 글자 자체. (값, 끝)
        let quoted = |i: usize, close: u16, backslash: bool| -> (String, usize) {
            let mut value: Vec<u16> = Vec::new();
            let mut j = i + 1;
            while j < n {
                let c = text[j];
                if c == close {
                    if at(j + 1) == Some(close) {
                        value.push(close);
                        j += 2;
                        continue;
                    }
                    return (String::from_utf16_lossy(&value), j + 1);
                }
                if backslash && c == b'\\' as u16 && j + 1 < n {
                    let e = text[j + 1];
                    match unit_char(e) {
                        Some('n') => value.push(b'\n' as u16),
                        Some('r') => value.push(b'\r' as u16),
                        Some('t') => value.push(b'\t' as u16),
                        Some('0' | 'Z') => {}
                        _ => value.push(e),
                    }
                    j += 2;
                    continue;
                }
                value.push(c);
                j += 1;
            }
            (String::from_utf16_lossy(&value), n)
        };

        let mut delimiter: Vec<u16> = vec![b';' as u16];
        let mut versioned = false;
        let mut i = 0usize;
        while i < n {
            let code = text[i];
            let c = unit_char(code);
            if matches!(c, Some(' ' | '\t' | '\n' | '\r' | '\u{c}'))
                || code == 0xfeff
                || code == 0xa0
            {
                i += 1;
                continue;
            }
            if delimiter != [b';' as u16] && text[i..].starts_with(&delimiter) {
                end_statement(&mut current, &mut depth, &mut statements);
                i += delimiter.len();
                continue;
            }
            // 혼자서 뜻이 있는 줄: 배치 구분, DELIMITER, psql 메타 명령, # 주석
            if matches!(c, Some('G' | 'g' | '/' | 'D' | 'd' | '\\' | '#')) && at_line_start(i) {
                let e = line_end(i);
                let mut rest = &text[i..e];
                while rest.last().is_some_and(|&x| js_space(x)) {
                    rest = &rest[..rest.len() - 1];
                }
                let is_go = rest.len() >= 2 && eq_ci(&rest[..2], "GO") && {
                    let tail = &rest[2..];
                    tail.is_empty()
                        || (tail.first().is_some_and(|&x| js_space(x)) && {
                            let digits: Vec<u16> =
                                tail.iter().copied().skip_while(|&x| js_space(x)).collect();
                            !digits.is_empty() && digits.iter().all(|&d| is_digit(d))
                        })
                };
                if matches!(c, Some('\\' | '#')) || is_go || rest == [b'/' as u16] {
                    if !matches!(c, Some('\\' | '#')) {
                        end_statement(&mut current, &mut depth, &mut statements);
                    }
                    i = e;
                    continue;
                }
                if rest.len() > 9 && eq_ci(&rest[..9], "DELIMITER") && js_space(rest[9]) {
                    let arg: Vec<u16> = rest[9..]
                        .iter()
                        .copied()
                        .skip_while(|&x| js_space(x))
                        .collect();
                    if !arg.is_empty() && !arg.iter().any(|&x| js_space(x)) {
                        end_statement(&mut current, &mut depth, &mut statements);
                        delimiter = arg;
                        i = e;
                        continue;
                    }
                }
            }
            if c == Some('-') && at(i + 1) == Some(b'-' as u16) {
                i = line_end(i);
                continue;
            }
            if c == Some('/') && at(i + 1) == Some(b'*' as u16) {
                if at(i + 2) == Some(b'!' as u16) {
                    i += 3;
                    while at(i).is_some_and(is_digit) {
                        i += 1;
                    }
                    versioned = true;
                    continue;
                }
                let close = (i + 2..n.saturating_sub(1))
                    .find(|&k| text[k] == b'*' as u16 && text[k + 1] == b'/' as u16);
                i = close.map_or(n, |e| e + 2);
                continue;
            }
            if versioned && c == Some('*') && at(i + 1) == Some(b'/' as u16) {
                versioned = false;
                i += 2;
                continue;
            }
            if c == Some('\'') {
                let (value, end) = quoted(i, code, backslash_escapes);
                push(&mut current, Kind::Str, value);
                i = end;
                continue;
            }
            if matches!(c, Some('"' | '`')) {
                let (value, end) = quoted(i, code, false);
                push(&mut current, Kind::Quoted, value);
                i = end;
                continue;
            }
            if c == Some('[') && i + 1 < n && is_word_start(&text, i + 1) {
                let (value, end) = quoted(i, b']' as u16, false);
                push(&mut current, Kind::Quoted, value);
                i = end;
                continue;
            }
            if c == Some('$') {
                // PostgreSQL 달러 인용: $$ ... $$, $tag$ ... $tag$
                let mut j = i + 1;
                if at(j).is_some_and(|x| is_ascii_letter(x) || x == 95) {
                    j += 1;
                    while at(j).is_some_and(|x| is_ascii_letter(x) || is_digit(x) || x == 95) {
                        j += 1;
                    }
                }
                if at(j) == Some(b'$' as u16) {
                    let open = &text[i..=j];
                    let after = j + 1;
                    let close = (after..n).find(|&k| text[k..].starts_with(open));
                    let end = close.map_or(n, |k| k + open.len());
                    push(&mut current, Kind::Str, s(after, close.unwrap_or(n)));
                    i = end;
                    continue;
                }
            }
            if is_digit(code) || (c == Some('.') && at(i + 1).is_some_and(is_digit)) {
                // /\d*\.?\d+(?:[eE][+-]?\d+)?/y
                let mut j = i;
                while at(j).is_some_and(is_digit) {
                    j += 1;
                }
                let int_end = j;
                if at(j) == Some(b'.' as u16) && at(j + 1).is_some_and(is_digit) {
                    j += 1;
                    while at(j).is_some_and(is_digit) {
                        j += 1;
                    }
                } else if int_end == i {
                    j = i;
                }
                if matches!(at(j).and_then(unit_char), Some('e' | 'E')) {
                    let mut k = j + 1;
                    if matches!(at(k).and_then(unit_char), Some('+' | '-')) {
                        k += 1;
                    }
                    if at(k).is_some_and(is_digit) {
                        while at(k).is_some_and(is_digit) {
                            k += 1;
                        }
                        j = k;
                    }
                }
                push(&mut current, Kind::Number, s(i, j));
                i = j;
                continue;
            }
            if is_word_start(&text, i) {
                let mut j = i + 1;
                while j < n && is_word_part(&text, j) {
                    j += 1;
                }
                let word = &text[i..j];
                if depth == 0
                    && delimiter == [b';' as u16]
                    && !current.is_empty()
                    && (eq_ci(word, "CREATE") || eq_ci(word, "ALTER"))
                    && at_line_start(i)
                {
                    end_statement(&mut current, &mut depth, &mut statements);
                }
                // 접두사 붙은 문자열: N'...', E'...', B'...', X'...', MySQL 문자셋 _utf8mb4'...'
                let up = String::from_utf16_lossy(word).to_uppercase();
                if at(j) == Some(b'\'' as u16)
                    && (matches!(up.as_str(), "N" | "E" | "B" | "X") || word[0] == 95)
                {
                    let (value, end) = quoted(j, b'\'' as u16, backslash_escapes || up == "E");
                    push(&mut current, Kind::Str, value);
                    i = end;
                    continue;
                }
                push(&mut current, Kind::Word, String::from_utf16_lossy(word));
                i = j;
                continue;
            }
            if c == Some(';') {
                if delimiter == [b';' as u16] {
                    end_statement(&mut current, &mut depth, &mut statements);
                } else {
                    push(&mut current, Kind::Punct, s(i, i + 1));
                }
                i += 1;
                continue;
            }
            if c == Some(':') && at(i + 1) == Some(b':' as u16) {
                push(&mut current, Kind::Punct, "::".into());
                i += 2;
                continue;
            }
            if c == Some('(') {
                depth += 1;
            } else if c == Some(')') {
                depth = (depth - 1).max(0);
            }
            push(&mut current, Kind::Punct, s(i, i + 1));
            i += 1;
        }
        end_statement(&mut current, &mut depth, &mut statements);
        statements
    }
}

use lexer::{Kind, Token};

struct Cursor {
    tokens: Vec<Token>,
    pos: usize,
}

impl Cursor {
    fn peek(&self, k: usize) -> Option<&Token> {
        self.tokens.get(self.pos + k)
    }
    fn done(&self) -> bool {
        self.pos >= self.tokens.len()
    }
    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn word(&self, w: &str, k: usize) -> bool {
        self.peek(k)
            .is_some_and(|t| t.kind == Kind::Word && t.upper == w)
    }
    /// 이 낱말들을 이 순서로 먹는다 — 아니면 아무것도
    fn eat(&mut self, ws: &[&str]) -> bool {
        if !ws.iter().enumerate().all(|(k, w)| self.word(w, k)) {
            return false;
        }
        self.pos += ws.len();
        true
    }
    fn eat_any(&mut self, ws: &[&str]) {
        while ws.iter().any(|w| self.word(w, 0)) {
            self.pos += 1;
        }
    }
    fn punct(&self, p: &str, k: usize) -> bool {
        self.peek(k)
            .is_some_and(|t| t.kind == Kind::Punct && t.value == p)
    }
    fn is_name(&self, k: usize) -> bool {
        self.peek(k)
            .is_some_and(|t| t.kind == Kind::Word || t.kind == Kind::Quoted)
    }
    /// 균형 맞는 (...) 안의 토큰 — `(` 가 아니면 None
    fn group(&mut self) -> Option<Vec<Token>> {
        if !self.punct("(", 0) {
            return None;
        }
        let start = self.pos;
        let mut depth = 0;
        while !self.done() {
            let t = self.next().expect("끝 전");
            if t.kind != Kind::Punct {
                continue;
            }
            if t.value == "(" {
                depth += 1;
            } else if t.value == ")" {
                depth -= 1;
                if depth == 0 {
                    return Some(self.tokens[start + 1..self.pos - 1].to_vec());
                }
            }
        }
        Some(self.tokens[(start + 1).min(self.tokens.len())..].to_vec())
    }
}

const INTERVAL_FIELDS: [&str; 7] = ["YEAR", "MONTH", "DAY", "HOUR", "MINUTE", "SECOND", "TO"];

/// 여러 낱말일 수 있는 타입을 resin 이 쓸 수 있는 꼴로 — 숫자 인자를 가진 이름 하나
fn read_type(e: &mut Cursor) -> SqlType {
    let mut name = e.next().map(|t| t.value.to_lowercase()).unwrap_or_default();
    while e.punct(".", 0) && e.is_name(1) {
        e.next();
        name = e.next().map(|t| t.value.to_lowercase()).unwrap_or_default();
    }
    if name == "double" {
        e.eat(&["PRECISION"]);
    } else if (name == "character" || name == "char") && e.eat(&["VARYING"]) {
        name = "varchar".into();
    } else if name == "national" && (e.eat(&["CHARACTER"]) || e.eat(&["CHAR"])) {
        name = if e.eat(&["VARYING"]) {
            "nvarchar"
        } else {
            "nchar"
        }
        .into();
    } else if name == "nchar" && e.eat(&["VARYING"]) {
        name = "nvarchar".into();
    } else if name == "bit" && e.eat(&["VARYING"]) {
        name = "varbit".into();
    } else if name == "long" && (e.word("RAW", 0) || e.word("VARCHAR", 0) || e.word("VARBINARY", 0))
    {
        name = format!(
            "long_{}",
            e.next().map(|t| t.value.to_lowercase()).unwrap_or_default()
        );
    }

    let mut args: Option<Vec<f64>> = None;
    let mut values: Option<Vec<String>> = None;
    if let Some(group) = e.group() {
        // MySQL 은 enum("a", "b") 도 받는다 — 글자 읽기에선 인용 이름이다
        let strs: Vec<String> = group
            .iter()
            .filter(|t| {
                t.kind == Kind::Str || (t.kind == Kind::Quoted && (name == "enum" || name == "set"))
            })
            .map(|t| t.value.clone())
            .collect();
        if !strs.is_empty() {
            values = Some(strs);
        } else if !group.iter().any(|t| {
            (t.kind == Kind::Word && t.upper == "MAX") || (t.kind == Kind::Punct && t.value == "*")
        }) {
            // varchar(max), number(*,0) 는 길이를 열어 둔다
            args = Some(
                group
                    .iter()
                    .filter(|t| {
                        t.kind == Kind::Number
                            && !t.value.is_empty()
                            && t.value.bytes().all(|b| b.is_ascii_digit())
                    })
                    .map(|t| t.value.parse::<f64>().unwrap_or(f64::NAN))
                    .collect(),
            );
        }
        if args.as_ref().is_some_and(|a| a.is_empty()) {
            args = None;
        }
    }
    if name == "timestamp" || name == "time" {
        if e.eat(&["WITH", "TIME", "ZONE"]) {
            name.push_str("tz");
        } else if e.eat(&["WITH", "LOCAL", "TIME", "ZONE"]) {
            name.push_str("ltz");
        } else {
            e.eat(&["WITHOUT", "TIME", "ZONE"]);
        }
    }
    if name == "interval" {
        while e
            .peek(0)
            .is_some_and(|t| t.kind == Kind::Word && INTERVAL_FIELDS.contains(&t.upper.as_str()))
        {
            e.next();
            e.group();
        }
    }
    let mut array = false;
    while e.punct("[", 0) {
        while !e.done() && !e.punct("]", 0) {
            e.next();
        }
        e.next();
        array = true;
    }
    if e.eat(&["ARRAY"]) {
        array = true;
    }
    e.eat_any(&["UNSIGNED", "SIGNED", "ZEROFILL"]);

    name = match name.as_str() {
        "smallserial" | "serial2" => "smallint".into(),
        "serial" | "serial4" => "integer".into(),
        "bigserial" | "serial8" => "bigint".into(),
        _ => name,
    };
    if matches!(
        name.as_str(),
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint"
    ) {
        args = None;
    }
    // /[^a-z0-9_]/g 는 UTF-16 단위마다 바꾼다
    name = name
        .chars()
        .flat_map(|c| {
            let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
            std::iter::repeat_n(
                if keep { c } else { '_' },
                if keep { 1 } else { c.len_utf16() },
            )
        })
        .collect();
    if !name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        name = format!("_{name}");
    }
    // PostgreSQL 이 배열 타입에 붙이는 이름: text[] 은 _text
    if array {
        name = format!("_{name}");
    }
    SqlType { name, args, values }
}

// ---- 가져오기(importStatements 중 표 문장이 지나는 길) ----

#[derive(Debug, Clone, Copy)]
struct At {
    line: u32,
    col: u32,
}

#[derive(Debug, Clone)]
struct SqlName {
    parts: Vec<String>,
    keys: Vec<String>,
    line: u32,
}

impl SqlName {
    fn object_name(&self) -> &str {
        self.parts.last().expect("이름은 비지 않는다")
    }
    fn object_key(&self) -> &str {
        self.keys.last().expect("이름은 비지 않는다")
    }
    fn schema_name(&self) -> Option<String> {
        (self.parts.len() > 1).then(|| self.parts[..self.parts.len() - 1].join("."))
    }
    fn schema_key(&self) -> Option<Vec<String>> {
        (self.keys.len() > 1).then(|| self.keys[..self.keys.len() - 1].to_vec())
    }
    fn at(&self) -> At {
        At {
            line: self.line,
            col: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct ColRef {
    table: SqlName,
    column: Option<String>,
    key: Option<String>,
    kind: RefKind,
    at: At,
}

#[derive(Debug, Clone)]
struct Col {
    name: String,
    key: String,
    display: String,
    at: At,
    ty: SqlType,
    not_null: bool,
    pk: bool,
    uk: Option<Option<String>>,
    index: Option<Option<String>>,
    reference: Option<ColRef>,
    values: Option<Vec<String>>,
    comment: Option<String>,
    enc: bool,
}

#[derive(Debug, Clone)]
struct FkRef {
    table: SqlName,
    columns: Option<Vec<String>>,
    keys: Option<Vec<String>>,
}

/// 쓸 때 정해지는 참조 글자 — 외부 표 이름은 표를 다 정한 뒤에 정한다
#[derive(Debug, Clone)]
enum RefText {
    Fixed(String),
    Stub { stub: usize, column: usize },
}

#[derive(Debug, Clone)]
enum FkText {
    Fixed(String),
    Stub { stub: usize, columns: Vec<usize> },
}

#[derive(Debug, Clone)]
struct Fk {
    name: Option<String>,
    columns: Vec<usize>,
    reference: FkRef,
    kind: RefKind,
    at: At,
    text: Option<FkText>,
}

#[derive(Debug, Clone)]
struct Tbl {
    name: SqlName,
    display: String,
    key: String,
    schema: Option<Vec<String>>,
    cols: Vec<Col>,
    uniques: Vec<(Option<String>, Vec<usize>)>,
    indexes: Vec<(Option<String>, Vec<usize>)>,
    foreign_keys: Vec<Fk>,
    comment: Option<String>,
    losses: Vec<String>,
    audit: Option<Option<Vec<String>>>,
    refs: Vec<Option<RefText>>,
}

impl Tbl {
    fn col_of(&self, key: &str) -> Option<usize> {
        self.cols.iter().position(|c| c.key == key)
    }
}

struct StubColumn {
    name: String,
    key: String,
    display: String,
    ty: SqlType,
    at: At,
}

struct Stub {
    keys: Vec<String>,
    name: String,
    display: String,
    at: At,
    columns: Vec<StubColumn>,
}

struct Import {
    tables: Vec<Tbl>,
    notes: Vec<SqlNote>,
    stubs: Vec<Stub>,
}

impl Import {
    fn note(&mut self, message: String, at: At) {
        self.notes.push(SqlNote {
            message,
            line: at.line,
            col: at.col,
        });
    }

    fn loss(&mut self, ti: usize, message: String, at: At) {
        let t = &mut self.tables[ti];
        t.losses.push(message.clone());
        // 첫 UTF-16 단위만 소문자로(message[0].toLowerCase())
        let mut cs = message.chars();
        let first = cs
            .next()
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default();
        let name = t.name.object_name().to_string();
        self.note(format!("table `{name}`: {first}{}", cs.as_str()), at);
    }

    /// 이 이름의 표들 — 스키마를 적었으면 그 스키마에서, 아니면 같은 스키마의 것을 먼저
    fn matches(&self, name: &SqlName, context: Option<usize>) -> Vec<usize> {
        let key = name.object_key();
        let live: Vec<usize> = (0..self.tables.len())
            .filter(|&i| self.tables[i].key == key)
            .collect();
        if let Some(schema) = name.schema_key() {
            return live
                .into_iter()
                .filter(|&i| self.tables[i].schema.as_ref() == Some(&schema))
                .collect();
        }
        if let Some(ctx) = context.and_then(|c| self.tables[c].schema.clone()) {
            let local: Vec<usize> = live
                .iter()
                .copied()
                .filter(|&i| self.tables[i].schema.as_ref() == Some(&ctx))
                .collect();
            if !local.is_empty() {
                return local;
            }
        }
        live
    }

    fn find(&self, name: &SqlName) -> Option<usize> {
        let m = self.matches(name, None);
        (m.len() == 1).then(|| m[0])
    }

    /// 이 이름들의 컬럼 — 하나라도 없으면 알리고 None
    fn columns(
        &mut self,
        ti: usize,
        names: &[String],
        keys: &[String],
        what: &str,
        at: At,
    ) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        for (i, name) in names.iter().enumerate() {
            match self.tables[ti].col_of(&keys[i]) {
                Some(c) => out.push(c),
                None => {
                    let t = self.tables[ti].name.object_name().to_string();
                    self.note(
                        format!(
                            "table `{t}`: {what} names `{name}`, which the table does not have"
                        ),
                        at,
                    );
                    return None;
                }
            }
        }
        Some(out)
    }

    fn stub_column(
        &mut self,
        table: &SqlName,
        name: &str,
        key: &str,
        ty: &SqlType,
        at: At,
    ) -> (usize, usize) {
        let si = match self.stubs.iter().position(|s| s.keys == table.keys) {
            Some(i) => i,
            None => {
                self.stubs.push(Stub {
                    keys: table.keys.clone(),
                    name: table.parts.join("."),
                    display: String::new(),
                    at,
                    columns: Vec::new(),
                });
                self.stubs.len() - 1
            }
        };
        let ci = match self.stubs[si].columns.iter().position(|c| c.key == key) {
            Some(ci) => {
                let (old, new) = (type_text(&self.stubs[si].columns[ci].ty), type_text(ty));
                if old != new {
                    let s = self.stubs[si].name.clone();
                    self.note(
                        format!("external column `{s}.{name}` has conflicting inferred types; {old} is kept"),
                        at,
                    );
                }
                ci
            }
            None => {
                self.stubs[si].columns.push(StubColumn {
                    name: name.to_string(),
                    key: key.to_string(),
                    display: String::new(),
                    ty: ty.clone(),
                    at,
                });
                self.stubs[si].columns.len() - 1
            }
        };
        (si, ci)
    }
}

fn name_of(name: &str, schema: Option<&str>, line: u32) -> SqlName {
    let parts: Vec<String> = match schema.filter(|s| !s.is_empty()) {
        Some(s) => vec![s.to_string(), name.to_string()],
        None => vec![name.to_string()],
    };
    SqlName {
        keys: parts.iter().map(|p| lower(p)).collect(),
        parts,
        line,
    }
}

fn same_set(a: &[usize], b: &[usize]) -> bool {
    a.len() == b.len() && a.iter().all(|c| b.contains(c))
}

/// 구조화된 스키마를 resin 으로 — fromSql 과 같은 규칙(docs/SPEC.md §8)
pub fn from_schema(input: &SchemaInput) -> SqlImport {
    let mut im = Import {
        tables: Vec::new(),
        notes: Vec::new(),
        stubs: Vec::new(),
    };

    // ---- 1차: 표 ----
    for (i, st) in input.tables.iter().enumerate() {
        let at = At {
            line: i as u32 + 1,
            col: 0,
        };
        let name = name_of(&st.name, st.schema.as_deref(), at.line);
        if im
            .tables
            .iter()
            .any(|t| t.key == name.object_key() && t.schema == name.schema_key())
        {
            im.note(
                format!(
                    "table `{}` is created twice; the first is kept",
                    name.object_name()
                ),
                at,
            );
            continue;
        }
        let ti = im.tables.len();
        im.tables.push(Tbl {
            display: name.object_name().to_string(),
            key: name.object_key().to_string(),
            schema: name.schema_key(),
            name,
            cols: Vec::new(),
            uniques: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            comment: st.comment.clone(),
            losses: Vec::new(),
            audit: None,
            refs: Vec::new(),
        });
        for (j, c) in st.columns.iter().enumerate() {
            let key = lower(&c.name);
            if im.tables[ti].col_of(&key).is_some() {
                let t = im.tables[ti].name.object_name().to_string();
                im.note(
                    format!(
                        "table `{t}`: column `{}` appears twice; the first is kept",
                        c.name
                    ),
                    At {
                        line: at.line,
                        col: j as u32 + 1,
                    },
                );
                continue;
            }
            let ty = parse_type(&c.ty);
            let values = ty.values.clone().or_else(|| c.values.clone());
            im.tables[ti].cols.push(Col {
                name: c.name.clone(),
                key,
                display: c.name.clone(),
                at: At {
                    line: at.line,
                    col: j as u32 + 1,
                },
                ty,
                not_null: !c.nullable,
                pk: false,
                uk: None,
                index: None,
                reference: None,
                values,
                comment: c.comment.clone(),
                enc: c.encrypted.unwrap_or(false),
            });
        }
        // 제약은 기본 키, 인덱스, FK 순서
        if let Some(pk) = st.primary_key.as_ref().filter(|p| !p.is_empty()) {
            let keys: Vec<String> = pk.iter().map(|k| lower(k)).collect();
            for c in im
                .columns(ti, pk, &keys, "the primary key", at)
                .unwrap_or_default()
            {
                let col = &mut im.tables[ti].cols[c];
                col.pk = true;
                col.not_null = true;
            }
        }
        for ix in st.indexes.iter().flatten() {
            let unique = ix.unique.unwrap_or(false);
            let label = format!(
                "{}{}",
                if unique { "unique index" } else { "index" },
                // 빈 이름은 JS 에서 거짓이다
                ix.name
                    .as_ref()
                    .filter(|n| !n.is_empty())
                    .map(|n| format!(" {n}"))
                    .unwrap_or_default()
            );
            let keys: Vec<String> = ix.columns.iter().map(|k| lower(k)).collect();
            let Some(cs) = im.columns(ti, &ix.columns, &keys, &label, at) else {
                continue;
            };
            if cs.is_empty() {
                continue;
            }
            let t = &mut im.tables[ti];
            if unique {
                t.uniques.push((ix.name.clone(), cs));
            } else {
                t.indexes.push((ix.name.clone(), cs));
            }
        }
        for fk in st.foreign_keys.iter().flatten() {
            let r = FkRef {
                table: name_of(&fk.table, fk.schema.as_deref(), at.line),
                columns: fk.references.clone(),
                keys: fk
                    .references
                    .as_ref()
                    .map(|r| r.iter().map(|k| lower(k)).collect()),
            };
            let kind = fk.kind.unwrap_or(RefKind::Physical);
            let keys: Vec<String> = fk.columns.iter().map(|k| lower(k)).collect();
            if fk.columns.len() != 1 || r.columns.as_ref().is_some_and(|c| c.len() != 1) {
                let list = fk.columns.join(", ");
                let Some(cs) = im.columns(ti, &fk.columns, &keys, "a foreign key", at) else {
                    im.tables[ti].losses.push(format!(
                        "Not converted: foreign key ({list}) has a missing source column"
                    ));
                    continue;
                };
                let distinct = |v: &[usize]| v.iter().collect::<HashSet<_>>().len() == v.len();
                if cs.len() < 2
                    || r.columns.as_ref().is_some_and(|c| c.len() != cs.len())
                    || !distinct(&cs)
                    || r.keys
                        .as_ref()
                        .is_some_and(|k| k.iter().collect::<HashSet<_>>().len() != k.len())
                {
                    im.loss(
                        ti,
                        format!("Not converted: foreign key ({list}) needs equal lists of distinct columns"),
                        at,
                    );
                    continue;
                }
                im.tables[ti].foreign_keys.push(Fk {
                    name: fk.name.clone(),
                    columns: cs,
                    reference: r,
                    kind,
                    at,
                    text: None,
                });
                continue;
            }
            let c = im
                .columns(ti, &fk.columns, &keys, "a foreign key", at)
                .and_then(|cs| cs.first().copied());
            if let Some(c) = c
                && im.tables[ti].cols[c].reference.is_none()
            {
                im.tables[ti].cols[c].reference = Some(ColRef {
                    column: r.columns.as_ref().and_then(|v| v.first().cloned()),
                    key: r.keys.as_ref().and_then(|v| v.first().cloned()),
                    table: r.table,
                    kind,
                    at,
                });
            }
        }
    }

    let all: Vec<usize> = (0..im.tables.len()).collect();

    // Hibernate Envers: revinfo 와 <표>_aud 는 `audit envers` 가 만드는 것이라 그 한 줄이 된다
    let mut audits: Vec<usize> = Vec::new();
    if let Some(revinfo) = all.iter().copied().find(|&i| im.tables[i].key == "revinfo") {
        for &aud in &all {
            let Some(base_key) = im.tables[aud].key.strip_suffix("_aud").map(str::to_string) else {
                continue;
            };
            let base = all.iter().copied().find(|&i| {
                im.tables[i].key == base_key && im.tables[i].schema == im.tables[aud].schema
            });
            let has = |n: &str| im.tables[aud].cols.iter().any(|c| lower(&c.name) == n);
            let Some(base) = base else {
                continue;
            };
            if im.tables[base].audit.is_some()
                || !has("rev")
                || !has("revtype")
                || !im.tables[base].cols.iter().any(|c| c.pk)
            {
                continue;
            }
            let kept: HashSet<String> =
                im.tables[aud].cols.iter().map(|c| lower(&c.name)).collect();
            let b = &im.tables[base];
            let audited: Vec<usize> = (0..b.cols.len())
                .filter(|&i| !b.cols[i].pk && kept.contains(&lower(&b.cols[i].name)))
                .collect();
            let every: Vec<usize> = (0..b.cols.len())
                .filter(|&i| {
                    !b.cols[i].pk
                        && !["created_at", "updated_at"].contains(&b.cols[i].name.as_str())
                })
                .collect();
            let cols = (!same_set(&audited, &every))
                .then(|| audited.iter().map(|&i| b.cols[i].name.clone()).collect());
            im.tables[base].audit = Some(cols);
            audits.push(aud);
        }
        if !audits.is_empty() {
            audits.push(revinfo);
        }
    }
    let drawn: Vec<usize> = all
        .iter()
        .copied()
        .filter(|i| !audits.contains(i))
        .collect();

    // 다른 스키마의 같은 이름 표는 스키마까지 이름에 든다
    for &t in &drawn {
        if im.tables[t].schema.is_some()
            && drawn
                .iter()
                .any(|&o| o != t && im.tables[o].key == im.tables[t].key)
        {
            let n = &im.tables[t].name;
            im.tables[t].display = format!(
                "{}.{}",
                n.schema_name().unwrap_or_default(),
                n.object_name()
            );
        }
    }

    let renamed = |im: &mut Import, what: &str, name: &str, display: &str, at: At| {
        if name != display {
            im.note(
                format!(
                    "{what} {} renamed to {} for resin",
                    quote(name),
                    quote(display)
                ),
                at,
            );
        }
    };
    let displays: Vec<String> = drawn
        .iter()
        .map(|&t| im.tables[t].display.clone())
        .collect();
    let mut table_names = Names::new(displays.iter().map(String::as_str), Vec::new());
    for &t in &drawn {
        let name = im.tables[t].display.clone();
        let display = table_names.take(&name);
        im.tables[t].display = display.clone();
        let at = im.tables[t].name.at();
        renamed(&mut im, "table", &name, &display, at);
        let col_names: Vec<String> = im.tables[t].cols.iter().map(|c| c.name.clone()).collect();
        let mut column_names = Names::new(col_names.iter().map(String::as_str), Vec::new());
        for ci in 0..im.tables[t].cols.len() {
            let c = &im.tables[t].cols[ci];
            let (cname, cat) = (c.name.clone(), c.at);
            let d = column_names.take(&cname);
            im.tables[t].cols[ci].display = d.clone();
            renamed(
                &mut im,
                &format!("column of {}", quote(&display)),
                &cname,
                &d,
                cat,
            );
            for value in im.tables[t].cols[ci].values.clone().unwrap_or_default() {
                if value.is_empty() || value.contains(['`', '\r', '\n']) {
                    im.loss(
                        t,
                        format!(
                            "Not converted: enum value {} of column {}",
                            quote(&value),
                            quote(&cname)
                        ),
                        cat,
                    );
                }
            }
        }
    }

    // ---- 참조 ----
    for t in &mut im.tables {
        t.refs = vec![None; t.cols.len()];
    }
    for &t in &drawn {
        for ci in 0..im.tables[t].cols.len() {
            let Some(r) = im.tables[t].cols[ci].reference.clone() else {
                continue;
            };
            let cname = im.tables[t].cols[ci].name.clone();
            let targets = im.matches(&r.table, Some(t));
            if targets.len() > 1 {
                let list = targets
                    .iter()
                    .map(|&x| im.tables[x].name.parts.join("."))
                    .collect::<Vec<_>>()
                    .join(", ");
                im.loss(
                    t,
                    format!(
                        "Not converted: reference {cname} -> {}, ambiguous target ({list})",
                        r.table.parts.join(".")
                    ),
                    r.at,
                );
                continue;
            }
            match targets.first().copied() {
                Some(target) if audits.contains(&target) => im.loss(
                    t,
                    format!(
                        "Not converted: reference {cname} -> {}, a table that `audit envers` generates",
                        r.table.object_name()
                    ),
                    r.at,
                ),
                Some(target) => {
                    let tt = &im.tables[target];
                    let pks: Vec<usize> = (0..tt.cols.len()).filter(|&i| tt.cols[i].pk).collect();
                    let to = match &r.key {
                        Some(k) => tt.col_of(k),
                        None if pks.len() == 1 => Some(pks[0]),
                        None => None,
                    };
                    let Some(to) = to else {
                        let why = match &r.column {
                            Some(c) => format!("{} has no column {c}", tt.display),
                            None => format!("the primary key of {} is not one column", tt.display),
                        };
                        let shown = match &r.column {
                            Some(c) => format!("{}.{c}", tt.display),
                            None => tt.display.clone(),
                        };
                        im.loss(t, format!("Not converted: reference {cname} -> {shown}, {why}"), r.at);
                        continue;
                    };
                    // 기본 키를 가리키면 컬럼 이름이 필요 없다
                    let text = if pks.len() == 1 && pks[0] == to {
                        resin_name(&tt.display)
                    } else {
                        format!("{}.{}", resin_name(&tt.display), resin_name(&tt.cols[to].display))
                    };
                    im.tables[t].refs[ci] = Some(RefText::Fixed(text));
                }
                None => {
                    // SQL 밖의 표 — 아는 것만, 가리킨 컬럼만 선언한다
                    let name = r.column.clone().unwrap_or_else(|| "id".into());
                    let ty = im.tables[t].cols[ci].ty.clone();
                    let (stub, column) = im.stub_column(&r.table, &name, r.key.as_deref().unwrap_or("id"), &ty, r.at);
                    if r.column.is_none() {
                        let s = im.stubs[stub].name.clone();
                        im.note(
                            format!("table `{s}` is not in the SQL; its primary key is taken to be `id`"),
                            r.at,
                        );
                    }
                    im.tables[t].refs[ci] = Some(RefText::Stub { stub, column });
                }
            }
        }
    }

    for &t in &drawn {
        for fi in 0..im.tables[t].foreign_keys.len() {
            let fk = im.tables[t].foreign_keys[fi].clone();
            let targets = im.matches(&fk.reference.table, Some(t));
            let target = targets.first().copied();
            let label = format!(
                "foreign key ({}) -> {}",
                fk.columns
                    .iter()
                    .map(|&c| im.tables[t].cols[c].name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                fk.reference.table.parts.join(".")
            );
            if targets.len() > 1 || target.is_some_and(|x| audits.contains(&x)) {
                let why = if targets.len() > 1 {
                    "ambiguous target"
                } else {
                    "the target is generated by audit envers"
                };
                im.loss(t, format!("Not converted: {label}, {why}"), fk.at);
                continue;
            }
            if let Some(target) = target {
                let tt = &im.tables[target];
                let to: Vec<Option<usize>> = match &fk.reference.keys {
                    Some(keys) => keys.iter().map(|k| tt.col_of(k)).collect(),
                    None => (0..tt.cols.len())
                        .filter(|&i| tt.cols[i].pk)
                        .map(Some)
                        .collect(),
                };
                if to.len() != fk.columns.len() || to.iter().any(Option::is_none) {
                    im.loss(
                        t,
                        format!("Not converted: {label}, target columns are missing or do not match the source list"),
                        fk.at,
                    );
                    continue;
                }
                let text = format!(
                    "{}({})",
                    resin_name(&tt.display),
                    to.iter()
                        .map(|c| resin_name(&tt.cols[c.expect("모두 있다")].display))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                im.tables[t].foreign_keys[fi].text = Some(FkText::Fixed(text));
            } else if fk.reference.columns.is_none() || fk.reference.keys.is_none() {
                im.loss(
                    t,
                    format!("Not converted: {label}, the target key columns are unknown"),
                    fk.at,
                );
            } else {
                let names = fk.reference.columns.clone().expect("있다");
                let keys = fk.reference.keys.clone().expect("있다");
                let mut stub = 0;
                let mut columns = Vec::new();
                for (i, name) in names.iter().enumerate() {
                    let ty = im.tables[t].cols[fk.columns[i]].ty.clone();
                    let (s, c) = im.stub_column(&fk.reference.table, name, &keys[i], &ty, fk.at);
                    if i == 0 {
                        stub = s;
                    }
                    columns.push(c);
                }
                im.tables[t].foreign_keys[fi].text = Some(FkText::Stub { stub, columns });
            }
        }
    }

    let stub_names: Vec<String> = im.stubs.iter().map(|s| s.name.clone()).collect();
    let occupied: Vec<String> = drawn
        .iter()
        .map(|&t| im.tables[t].display.clone())
        .collect();
    let mut names = Names::new(stub_names.iter().map(String::as_str), occupied);
    for si in 0..im.stubs.len() {
        let name = im.stubs[si].name.clone();
        let display = names.take(&name);
        im.stubs[si].display = display.clone();
        let at = im.stubs[si].at;
        renamed(&mut im, "external table", &name, &display, at);
        let col_names: Vec<String> = im.stubs[si]
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let mut column_names = Names::new(col_names.iter().map(String::as_str), Vec::new());
        for ci in 0..im.stubs[si].columns.len() {
            let (cname, cat) = (
                im.stubs[si].columns[ci].name.clone(),
                im.stubs[si].columns[ci].at,
            );
            let d = column_names.take(&cname);
            im.stubs[si].columns[ci].display = d.clone();
            renamed(
                &mut im,
                &format!("column of {}", quote(&display)),
                &cname,
                &d,
                cat,
            );
        }
    }

    // ---- 쓰기 ----
    let mut out: Vec<String> = Vec::new();
    for stub in &im.stubs {
        let pk = stub.columns.len() == 1;
        out.push(format!("external table {} {{", resin_name(&stub.display)));
        out.extend(align(
            &stub
                .columns
                .iter()
                .map(|x| {
                    [
                        resin_name(&x.display),
                        type_text(&x.ty),
                        if pk { "pk".into() } else { String::new() },
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        out.push("}".into());
        out.push(String::new());
    }
    // 둘 이상의 스키마 — 스키마마다 그 이름의 service 에, 처음 나온 자리에서
    let mut schemas: Vec<Vec<String>> = Vec::new();
    for &t in &drawn {
        if let Some(s) = &im.tables[t].schema
            && !schemas.contains(s)
        {
            schemas.push(s.clone());
        }
    }
    let split = schemas.len() >= 2;
    // Map 생성: 순서는 처음 나온 자리, 값은 마지막 것
    let mut schema_names: Vec<(Vec<String>, String)> = Vec::new();
    for &t in &drawn {
        let Some(s) = im.tables[t].schema.clone() else {
            continue;
        };
        let n = im.tables[t].name.schema_name().expect("스키마가 있다");
        match schema_names.iter_mut().find(|(k, _)| *k == s) {
            Some(e) => e.1 = n,
            None => schema_names.push((s, n)),
        }
    }
    let originals: Vec<String> = schema_names.iter().map(|(_, n)| n.clone()).collect();
    let mut service_names = Names::new(originals.iter().map(String::as_str), Vec::new());
    for i in 0..schema_names.len() {
        let name = schema_names[i].1.clone();
        let display = service_names.take(&name);
        schema_names[i].1 = display.clone();
        let first = drawn
            .iter()
            .copied()
            .find(|&t| im.tables[t].schema.as_ref() == Some(&schema_names[i].0))
            .expect("그 스키마의 표");
        let at = im.tables[first].name.at();
        renamed(&mut im, "service", &name, &display, at);
    }
    let mut written: Vec<Vec<String>> = Vec::new();
    for &t in &drawn {
        match im.tables[t].schema.clone() {
            Some(schema) if split => {
                if written.contains(&schema) {
                    continue;
                }
                written.push(schema.clone());
                let members: Vec<usize> = drawn
                    .iter()
                    .copied()
                    .filter(|&x| im.tables[x].schema.as_ref() == Some(&schema))
                    .collect();
                let mut body: Vec<String> = Vec::new();
                for (k, &x) in members.iter().enumerate() {
                    if k > 0 {
                        body.push(String::new());
                    }
                    body.extend(write_table(&mut im, x));
                }
                let service = schema_names
                    .iter()
                    .find(|(k, _)| *k == schema)
                    .map(|(_, n)| n.clone())
                    .expect("서비스 이름");
                out.push(format!("service {} {{", resin_name(&service)));
                out.extend(
                    body.into_iter()
                        .map(|l| if l.is_empty() { l } else { format!("  {l}") }),
                );
                out.push("}".into());
                out.push(String::new());
            }
            _ => {
                out.extend(write_table(&mut im, t));
                out.push(String::new());
            }
        }
    }
    im.notes
        .sort_by(|a, b| (a.line, a.col).cmp(&(b.line, b.col)));
    let source = if drawn.is_empty() {
        String::new()
    } else {
        format!("{}\n", js_trim_end(&out.join("\n")))
    };
    SqlImport {
        source,
        notes: im.notes,
        tables: drawn.len(),
    }
}

fn write_table(im: &mut Import, ti: usize) -> Vec<String> {
    let pk_count = im.tables[ti].cols.iter().filter(|c| c.pk).count();
    let pks: Vec<usize> = (0..im.tables[ti].cols.len())
        .filter(|&i| im.tables[ti].cols[i].pk)
        .collect();
    // SQL 이름은 옮긴 뒤에도 서로 달라야 한다 — resin 이 원래 철자를 못 써도
    let t = &im.tables[ti];
    let all: Vec<String> = t
        .cols
        .iter()
        .filter_map(|c| c.uk.clone().flatten())
        .chain(t.uniques.iter().filter_map(|u| u.0.clone()))
        .chain(t.indexes.iter().filter_map(|u| u.0.clone()))
        .chain(t.foreign_keys.iter().filter_map(|f| f.name.clone()))
        .collect();
    let mut names = Names::new(all.iter().map(String::as_str), Vec::new());
    let display = t.display.clone();
    let at = t.name.at();
    let mut claim = |im: &mut Import, n: Option<String>| -> Option<String> {
        let n = n?;
        let d = names.take(&n);
        if d != n {
            im.note(
                format!(
                    "index name {} renamed to {} in table {}",
                    quote(&n),
                    quote(&d),
                    quote(&display)
                ),
                at,
            );
        }
        Some(d)
    };
    for ci in 0..im.tables[ti].cols.len() {
        // 하나뿐인 기본 키 컬럼의 UNIQUE 는 더 말하는 게 없다
        let c = &im.tables[ti].cols[ci];
        if c.uk.is_some() && c.pk && pk_count == 1 {
            im.tables[ti].cols[ci].uk = None;
        }
        if let Some(n) = im.tables[ti].cols[ci].uk.clone() {
            let d = claim(im, n);
            im.tables[ti].cols[ci].uk = Some(d);
        }
    }
    let mut uniques: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for (uname, ucols) in im.tables[ti].uniques.clone() {
        if ucols.len() == 1 {
            let c = ucols[0];
            if im.tables[ti].cols[c].pk && pk_count == 1 {
                continue;
            }
            match im.tables[ti].cols[c].uk.clone() {
                None => {
                    let d = claim(im, uname);
                    im.tables[ti].cols[c].uk = Some(d);
                }
                Some(None) => {
                    let d = claim(im, uname);
                    im.tables[ti].cols[c].uk = Some(d);
                }
                Some(Some(_)) => {}
            }
        } else if !same_set(&ucols, &pks) && !uniques.iter().any(|x| same_set(&x.1, &ucols)) {
            let d = claim(im, uname);
            uniques.push((d, ucols));
        }
    }
    let mut indexes: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for (iname, icols) in im.tables[ti].indexes.clone() {
        if icols.len() == 1 {
            let c = icols[0];
            // 같은 컬럼의 두 번째 인덱스는 그림에 더하는 게 없다
            if im.tables[ti].cols[c].index.is_none() {
                let d = claim(im, iname);
                im.tables[ti].cols[c].index = Some(d);
            }
        } else if !indexes.iter().any(|x| x.1 == icols) {
            let d = claim(im, iname);
            indexes.push((d, icols));
        }
    }

    let t = &im.tables[ti];
    let rows: Vec<[String; 3]> = (0..t.cols.len())
        .map(|ci| {
            let c = &t.cols[ci];
            let mut mods: Vec<String> = Vec::new();
            if c.pk {
                mods.push("pk".into());
            }
            if let Some(u) = &c.uk {
                mods.push(match u {
                    Some(n) => format!("uk as {}", resin_name(n)),
                    None => "uk".into(),
                });
            }
            if let (Some(r), Some(cr)) = (&t.refs[ci], &c.reference) {
                let arrow = if cr.kind == RefKind::Physical {
                    "->"
                } else {
                    "~>"
                };
                mods.push(format!("{arrow} {}", ref_text(im, r)));
            }
            if let Some(v) = enum_values(c.values.as_deref()) {
                mods.push(format!("enum({v})"));
            }
            if let Some(ix) = &c.index {
                mods.push(match ix {
                    Some(n) => format!("index as {}", resin_name(n)),
                    None => "index".into(),
                });
            }
            if c.enc {
                mods.push("enc".into());
            }
            let comment = one_line(c.comment.as_deref());
            if !comment.is_empty() {
                mods.push(resin_string(&comment));
            }
            [
                resin_name(&c.display),
                format!("{}{}", type_text(&c.ty), if c.not_null { "" } else { "?" }),
                mods.join("  "),
            ]
        })
        .collect();
    let list = |cs: &[usize]| {
        cs.iter()
            .map(|&c| resin_name(&t.cols[c].display))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let as_ = |n: &Option<String>| {
        n.as_ref()
            .map(|n| format!(" as {}", resin_name(n)))
            .unwrap_or_default()
    };
    let comment = one_line(t.comment.as_deref());
    let audit = match &t.audit {
        None => String::new(),
        Some(None) => " audit envers".into(),
        Some(Some(cols)) => format!(
            " audit envers({})",
            cols.iter()
                .map(|name| resin_name(
                    &t.cols
                        .iter()
                        .find(|c| &c.name == name)
                        .expect("감사 컬럼")
                        .display
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let mut lines = vec![format!(
        "table {}{} {{",
        resin_name(&t.display),
        if comment.is_empty() {
            String::new()
        } else {
            format!(" {}", resin_string(&comment))
        }
    )];
    lines.extend(align(&rows));
    lines.extend(
        uniques
            .iter()
            .map(|u| format!("  unique({}){}", list(&u.1), as_(&u.0))),
    );
    lines.extend(
        indexes
            .iter()
            .map(|ix| format!("  index({}){}", list(&ix.1), as_(&ix.0))),
    );
    let fks = t.foreign_keys.clone();
    let losses = t.losses.clone();
    let audit_line = format!("}}{audit}");
    for fk in &fks {
        let Some(text) = &fk.text else {
            continue;
        };
        let arrow = if fk.kind == RefKind::Physical {
            "->"
        } else {
            "~>"
        };
        let text = fk_text(im, text);
        let cols = {
            let t = &im.tables[ti];
            fk.columns
                .iter()
                .map(|&c| resin_name(&t.cols[c].display))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let name = claim(im, fk.name.clone());
        lines.push(format!("  foreign({cols}) {arrow} {text}{}", as_(&name)));
    }
    lines.extend(losses.iter().map(|l| format!("  %% {}", one_line(Some(l)))));
    lines.push(audit_line);
    lines
}

fn ref_text(im: &Import, r: &RefText) -> String {
    match r {
        RefText::Fixed(s) => s.clone(),
        // 컬럼 하나뿐인 외부 표만 그 컬럼을 기본 키로 본다 — 그때는 컬럼 이름이 필요 없다
        RefText::Stub { stub, column } => {
            let s = &im.stubs[*stub];
            if s.columns.len() == 1 {
                resin_name(&s.display)
            } else {
                format!(
                    "{}.{}",
                    resin_name(&s.display),
                    resin_name(&s.columns[*column].display)
                )
            }
        }
    }
}

fn fk_text(im: &Import, f: &FkText) -> String {
    match f {
        FkText::Fixed(s) => s.clone(),
        FkText::Stub { stub, columns } => {
            let s = &im.stubs[*stub];
            format!(
                "{}({})",
                resin_name(&s.display),
                columns
                    .iter()
                    .map(|&c| resin_name(&s.columns[c].display))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

/// 예제처럼 이름, 타입, 수식어를 각각 줄 맞춘다. 넓은 글자는 두 칸
fn align(rows: &[[String; 3]]) -> Vec<String> {
    let pad = |s: &str, width: usize| format!("{s}{}", " ".repeat(width.saturating_sub(cells(s))));
    let name = rows.iter().map(|r| cells(&r[0])).max().unwrap_or(0);
    let ty = rows.iter().map(|r| cells(&r[1])).max().unwrap_or(0);
    rows.iter()
        .map(|[n, t, mods]| {
            js_trim_end(&format!("  {}  {}  {mods}", pad(n, name), pad(t, ty))).to_string()
        })
        .collect()
}

/// resin 이 쓸 수 있는 값 — 이름과 숫자는 맨으로, 나머지는 백틱. 남는 게 없으면 None
fn enum_values(values: Option<&[String]>) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for v in values? {
        let text = if is_ident(v) || (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())) {
            Some(v.clone())
        } else if !v.is_empty() && !v.contains(['`', '\r', '\n']) {
            Some(format!("`{v}`"))
        } else {
            None
        };
        if let Some(t) = text
            && !out.contains(&t)
        {
            out.push(t);
        }
    }
    (!out.is_empty()).then(|| out.join(", "))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn writes_like_resin_from_schema() {
        let cases: Value =
            serde_json::from_str(include_str!("../../tests/resin_from_schema.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert!(cases.len() >= 10);
        for c in cases {
            let input: super::SchemaInput = serde_json::from_value(c["input"].clone()).unwrap();
            let got = serde_json::to_value(super::from_schema(&input)).unwrap();
            if got != c["output"] {
                panic!(
                    "{}\n--- 기대\n{}\n--- 결과\n{}\n--- 메모 기대 {}\n--- 메모 결과 {}",
                    c["name"],
                    c["output"]["source"].as_str().unwrap(),
                    got["source"].as_str().unwrap(),
                    c["output"]["notes"],
                    got["notes"]
                );
            }
        }
    }

    #[test]
    fn types_normalize_like_resin() {
        let cases: Value =
            serde_json::from_str(include_str!("../../tests/resin_from_schema.json")).unwrap();
        // 타입만 따로 모은 줄 — { text, type }
        for t in cases
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["types"].as_array().cloned().unwrap_or_default())
        {
            let got = super::parse_type(t["text"].as_str().unwrap());
            let want = &t["type"];
            assert_eq!(got.name, want["name"].as_str().unwrap(), "{}", t["text"]);
            let args: Option<Vec<f64>> = serde_json::from_value(want["args"].clone()).unwrap();
            assert_eq!(got.args, args, "{}", t["text"]);
            let values: Option<Vec<String>> =
                serde_json::from_value(want["values"].clone()).unwrap();
            assert_eq!(got.values, values, "{}", t["text"]);
        }
    }
}
