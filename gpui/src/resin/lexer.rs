//! 원문 → 토큰(lexer.ts). 키워드는 따로 없다 — 자리를 아는 파서가 정한다(컬럼 이름이 `index` 일 수 있다).
//! JS 문자열처럼 UTF-16 단위로 걷는다: 열과 길이, `\p{L}` 판정이 단위 하나씩이다.

use std::sync::LazyLock;

use regex::Regex;

use super::{Diagnostic, Span, error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tk {
    Ident,
    Number,
    Str,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Comma,
    Dot,
    Question,
    /// `->`
    Arrow,
    /// `~>`
    TildeArrow,
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Tk,
    /// 이름(백틱 뺀 것), 문자열은 푼 값, 나머지는 원문
    pub value: String,
    pub span: Span,
    pub quoted: bool,
}

static LETTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{L}$").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_]$").unwrap());

/// 한 단위가 정규식에 맞나 — 홀로 선 대리 쌍 반쪽은 글자가 아니다(JS 의 /u 판정도 그렇다)
fn unit_is(re: &Regex, u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(|c| re.is_match(c.encode_utf8(&mut [0; 4])))
}

fn ascii(u: u16) -> Option<u8> {
    (u < 128).then_some(u as u8)
}

pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let src: Vec<u16> = source.encode_utf16().collect();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let (mut i, mut line, mut line_start) = (0usize, 1u32, 0usize);
    let at = |k: usize| src.get(k).copied().and_then(ascii);
    let text = |a: usize, b: usize| String::from_utf16_lossy(&src[a..b]);

    while i < src.len() {
        let span = |start: usize, end: usize, line: u32, line_start: usize| Span {
            line,
            col: (start - line_start + 1) as u32,
            len: (end - start) as u32,
        };
        let c = at(i);
        let mut push = |kind: Tk, value: String, start: usize, end: usize| {
            tokens.push(Token {
                kind,
                value,
                span: span(start, end, line, line_start),
                quoted: false,
            })
        };
        match c {
            Some(b' ' | b'\t' | b'\r') => {
                i += 1;
                continue;
            }
            Some(b'\n') => {
                push(Tk::Newline, "\n".into(), i, i + 1);
                i += 1;
                line += 1;
                line_start = i;
                continue;
            }
            Some(b'%') if at(i + 1) == Some(b'%') => {
                while i < src.len() && at(i) != Some(b'\n') {
                    i += 1;
                }
                continue;
            }
            Some(b'-') if at(i + 1) == Some(b'>') => {
                push(Tk::Arrow, "->".into(), i, i + 2);
                i += 2;
                continue;
            }
            Some(b'~') if at(i + 1) == Some(b'>') => {
                push(Tk::TildeArrow, "~>".into(), i, i + 2);
                i += 2;
                continue;
            }
            _ => {}
        }
        let single = match c {
            Some(b'{') => Some(Tk::LBrace),
            Some(b'}') => Some(Tk::RBrace),
            Some(b'(') => Some(Tk::LParen),
            Some(b')') => Some(Tk::RParen),
            Some(b',') => Some(Tk::Comma),
            Some(b'.') => Some(Tk::Dot),
            Some(b'?') => Some(Tk::Question),
            _ => None,
        };
        if let Some(kind) = single {
            push(kind, text(i, i + 1), i, i + 1);
            i += 1;
            continue;
        }
        if c.is_some_and(|c| c.is_ascii_digit()) {
            let start = i;
            while at(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            push(Tk::Number, text(start, i), start, i);
            continue;
        }
        if c == Some(b'`') {
            let start = i;
            i += 1;
            while i < src.len() && at(i) != Some(b'`') && at(i) != Some(b'\n') {
                i += 1;
            }
            let closed = at(i) == Some(b'`');
            let value = text(start + 1, i);
            if closed {
                i += 1;
            }
            let sp = span(start, i, line, line_start);
            if !closed {
                diagnostics.push(error(
                    "unterminated backtick name".into(),
                    sp,
                    Some("close the name with a backtick on the same line".into()),
                ));
            } else if value.is_empty() {
                diagnostics.push(error("empty backtick name".into(), sp, None));
            }
            tokens.push(Token {
                kind: Tk::Ident,
                value,
                span: sp,
                quoted: true,
            });
            continue;
        }
        if c.is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            let start = i;
            while at(i).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                i += 1;
            }
            push(Tk::Ident, text(start, i), start, i);
            continue;
        }
        if c == Some(b'"') {
            let start = i;
            i += 1;
            let mut value: Vec<u16> = Vec::new();
            let mut closed = false;
            while i < src.len() && at(i) != Some(b'\n') {
                let ch = src[i];
                if ch == b'"' as u16 {
                    closed = true;
                    i += 1;
                    break;
                }
                if ch == b'\\' as u16 && matches!(at(i + 1), Some(b'"' | b'\\')) {
                    value.push(src[i + 1]);
                    i += 2;
                    continue;
                }
                value.push(ch);
                i += 1;
            }
            if !closed {
                diagnostics.push(error(
                    "unterminated string".into(),
                    span(start, i, line, line_start),
                    Some("close the string with `\"` on the same line".into()),
                ));
            }
            tokens.push(Token {
                kind: Tk::Str,
                value: String::from_utf16_lossy(&value),
                span: span(start, i, line, line_start),
                quoted: false,
            });
            continue;
        }
        // 한글 같은 비 ASCII 이름 — 글자마다가 아니라 통째로 한 번 알리고, 뒤따르는 오류("표 이름이 필요하다")가 쌓이지 않게 이름으로 낸다
        if unit_is(&LETTER, src[i]) {
            let start = i;
            while i < src.len() && unit_is(&WORD, src[i]) {
                i += 1;
            }
            let value = text(start, i);
            let sp = span(start, i, line, line_start);
            diagnostics.push(error(
                "names with non-ASCII letters must be wrapped in backticks".into(),
                sp,
                Some(format!("write it as `{value}`")),
            ));
            tokens.push(Token {
                kind: Tk::Ident,
                value,
                span: sp,
                quoted: true,
            });
            continue;
        }
        diagnostics.push(error(
            format!("unexpected character `{}`", text(i, i + 1)),
            span(i, i + 1, line, line_start),
            None,
        ));
        i += 1;
    }
    tokens.push(Token {
        kind: Tk::Eof,
        value: String::new(),
        span: Span {
            line,
            col: (i - line_start + 1) as u32,
            len: 0,
        },
        quoted: false,
    });
    (tokens, diagnostics)
}
