// 코드 창의 문법 색 — 지금 앱은 highlight.js 가 붙인 클래스에 여섯 색만 입힌다(styles.css 의 .hljs-*).
// 같은 여섯 갈래(키워드, 문자열, 숫자, 함수, 타입, 주석)만 가르는 작은 토크나이저다. 문법 전체를 이해하지 않고
// 언어별 키워드 표와 주석 표기만 다르게 둔다 — 코드가 무지개가 되지 않게 색 수를 묶는 게 목적이라 이걸로 충분하다.

use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Keyword,
    Str,
    Num,
    Func,
    Type,
    Comment,
}

struct Lang {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comment: &'static [&'static str],
    block_comment: bool,
    /// 대문자로 시작하는 이름을 타입으로 본다(클래스, 구조체 관례)
    caps_are_types: bool,
    /// `이름!` 을 매크로(내장)로 본다 — rust
    macros: bool,
    single_quote_strings: bool,
    backtick_strings: bool,
}

const LITERALS: &[&str] = &[
    "true",
    "false",
    "null",
    "None",
    "nil",
    "undefined",
    "Some",
    "Ok",
    "Err",
    "self",
    "this",
    "super",
    "Self",
    "True",
    "False",
];

const RUST: Lang = Lang {
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "static", "struct", "trait", "type", "unsafe", "use", "where",
        "while",
    ],
    types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box",
    ],
    line_comment: &["//"],
    block_comment: true,
    caps_are_types: true,
    macros: true,
    single_quote_strings: false,
    backtick_strings: false,
};

const C_LIKE: Lang = Lang {
    keywords: &[
        "abstract",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "data",
        "default",
        "defer",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "final",
        "finally",
        "for",
        "from",
        "fun",
        "func",
        "function",
        "go",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "is",
        "let",
        "new",
        "object",
        "open",
        "override",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "struct",
        "suspend",
        "switch",
        "throw",
        "throws",
        "try",
        "type",
        "typeof",
        "val",
        "var",
        "void",
        "when",
        "while",
        "yield",
        "chan",
        "map",
        "range",
        "select",
    ],
    types: &[
        "int", "long", "short", "byte", "float", "double", "boolean", "bool", "char", "string",
        "number", "any", "unknown", "never", "String", "Int", "Long", "Boolean", "Unit", "error",
        "int64", "int32", "uint", "float64",
    ],
    line_comment: &["//"],
    block_comment: true,
    caps_are_types: true,
    macros: false,
    single_quote_strings: true,
    backtick_strings: true,
};

const PYTHON: Lang = Lang {
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield",
    ],
    types: &[
        "int", "float", "str", "bool", "list", "dict", "set", "tuple", "bytes", "object",
    ],
    line_comment: &["#"],
    block_comment: false,
    caps_are_types: true,
    macros: false,
    single_quote_strings: true,
    backtick_strings: false,
};

const SHELL: Lang = Lang {
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "until", "case",
        "esac", "function", "return", "export", "local", "set", "unset", "source", "echo", "cd",
        "exit",
    ],
    types: &[],
    line_comment: &["#"],
    block_comment: false,
    caps_are_types: false,
    macros: false,
    single_quote_strings: true,
    backtick_strings: true,
};

const SQL: Lang = Lang {
    keywords: &[
        "select",
        "from",
        "where",
        "and",
        "or",
        "not",
        "insert",
        "into",
        "values",
        "update",
        "set",
        "delete",
        "create",
        "table",
        "index",
        "on",
        "join",
        "left",
        "right",
        "inner",
        "outer",
        "group",
        "by",
        "order",
        "having",
        "limit",
        "offset",
        "as",
        "distinct",
        "union",
        "all",
        "primary",
        "key",
        "foreign",
        "references",
        "default",
        "null",
        "is",
        "in",
        "exists",
        "case",
        "when",
        "then",
        "else",
        "end",
        "alter",
        "add",
        "drop",
        "with",
        "recursive",
        "asc",
        "desc",
        "unique",
        "if",
        "begin",
        "commit",
    ],
    types: &[
        "int",
        "integer",
        "bigint",
        "smallint",
        "text",
        "varchar",
        "char",
        "boolean",
        "real",
        "double",
        "decimal",
        "numeric",
        "date",
        "datetime",
        "timestamp",
        "blob",
        "json",
    ],
    line_comment: &["--"],
    block_comment: true,
    caps_are_types: false,
    macros: false,
    single_quote_strings: true,
    backtick_strings: true,
};

const DATA: Lang = Lang {
    keywords: &[],
    types: &[],
    line_comment: &["#"],
    block_comment: false,
    caps_are_types: false,
    macros: false,
    single_quote_strings: true,
    backtick_strings: false,
};

fn lang_of(name: &str) -> Option<&'static Lang> {
    Some(match name.to_ascii_lowercase().as_str() {
        "rust" | "rs" => &RUST,
        "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx" | "java" | "kotlin" | "kt"
        | "go" | "golang" | "c" | "cpp" | "c++" | "h" | "swift" | "scala" | "dart" | "cs"
        | "csharp" => &C_LIKE,
        "py" | "python" => &PYTHON,
        "sh" | "bash" | "zsh" | "shell" | "console" => &SHELL,
        "sql" | "mysql" | "postgres" | "sqlite" => &SQL,
        "json" | "yaml" | "yml" | "toml" | "ini" => &DATA,
        _ => return None,
    })
}

/// 칠할 구간들(바이트 범위). 모르는 언어나 언어 표시가 없으면 칠하지 않는다(지금 앱의 highlight.js 도 같다)
pub fn highlight(lang: Option<&str>, code: &str) -> Vec<(Range<usize>, Kind)> {
    let Some(l) = lang.and_then(lang_of) else {
        return Vec::new();
    };
    let sql = std::ptr::eq(l, &SQL);
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // 직전 키워드가 fn/def/function 이면 다음 이름은 함수 정의
    let mut after_def = false;
    while i < b.len() {
        let c = b[i];
        let rest = &code[i..];
        // 주석
        if let Some(lc) = l.line_comment.iter().find(|p| rest.starts_with(**p)) {
            // 셸의 `$#`, 문자열 안은 이미 건너뛴 상태다. `#` 는 낱말 중간(`a#b`)이면 주석이 아니다
            if !(lc == &"#" && i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'$')) {
                let end = rest.find('\n').map_or(b.len(), |n| i + n);
                out.push((i..end, Kind::Comment));
                i = end;
                continue;
            }
        }
        if l.block_comment && rest.starts_with("/*") {
            let end = rest[2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            out.push((i..end, Kind::Comment));
            i = end;
            continue;
        }
        // 문자열
        let quote = match c {
            b'"' => true,
            b'\'' => l.single_quote_strings,
            b'`' => l.backtick_strings,
            _ => false,
        };
        if quote {
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                if b[j] == b'\\' {
                    j += 1;
                }
                // 따옴표 하나짜리가 줄을 넘지 않게(백틱은 넘을 수 있다)
                if b[j.min(b.len() - 1)] == b'\n' && c != b'`' {
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(b.len());
            out.push((i..end, Kind::Str));
            i = end;
            continue;
        }
        // 숫자 — 낱말 중간(x1)은 아니다
        if c.is_ascii_digit() && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'.' || b[j] == b'_') {
                j += 1;
            }
            out.push((i..j, Kind::Num));
            i = j;
            continue;
        }
        if is_ident_start(c) {
            let mut j = i + 1;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let word = &code[i..j];
            let lower = word.to_ascii_lowercase();
            let key = if sql { lower.as_str() } else { word };
            let next = b.get(j).copied();
            let kind = if after_def {
                Some(Kind::Func)
            } else if l.keywords.contains(&key) || LITERALS.contains(&word) {
                Some(Kind::Keyword)
            } else if l.types.contains(&key) {
                Some(Kind::Type)
            } else if l.macros && next == Some(b'!') {
                Some(Kind::Type)
            } else if next == Some(b'(') && !sql {
                Some(Kind::Func)
            } else if l.caps_are_types && word.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                Some(Kind::Type)
            } else {
                None
            };
            after_def = matches!(word, "fn" | "def" | "function" | "func" | "fun");
            let end = if l.macros && next == Some(b'!') {
                j + 1
            } else {
                j
            };
            if let Some(k) = kind {
                out.push((i..end, k));
            }
            i = end;
            continue;
        }
        if !c.is_ascii_whitespace() {
            after_def = false;
        }
        // 여러 바이트 글자(한글 등)는 글자 단위로 넘긴다
        i += utf8_len(c);
    }
    out
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn utf8_len(c: u8) -> usize {
    match c {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, highlight};

    fn kinds(lang: &str, code: &str) -> Vec<(String, Kind)> {
        highlight(Some(lang), code)
            .into_iter()
            .map(|(r, k)| (code[r].to_string(), k))
            .collect()
    }

    #[test]
    fn rust_sample() {
        let got = kinds(
            "rust",
            "fn main() {\n    let x = 42; // answer\n    println!(\"{x}\");\n}\n",
        );
        assert_eq!(
            got,
            vec![
                ("fn".into(), Kind::Keyword),
                ("main".into(), Kind::Func),
                ("let".into(), Kind::Keyword),
                ("42".into(), Kind::Num),
                ("// answer".into(), Kind::Comment),
                ("println!".into(), Kind::Type),
                ("\"{x}\"".into(), Kind::Str),
            ]
        );
    }

    #[test]
    fn unknown_language_is_plain_and_korean_is_safe() {
        assert!(highlight(None, "let x = 1").is_empty());
        assert!(highlight(Some("brainfuck"), "+++").is_empty());
        let got = kinds("python", "# 한글 주석\nprint('안녕')\n");
        assert_eq!(got[0], ("# 한글 주석".into(), Kind::Comment));
        assert_eq!(got[2], ("'안녕'".into(), Kind::Str));
    }

    #[test]
    fn sql_is_case_insensitive() {
        let got = kinds("sql", "SELECT id FROM todos WHERE done = 1 -- open");
        assert_eq!(got[0], ("SELECT".into(), Kind::Keyword));
        assert_eq!(got.last().unwrap(), &("-- open".into(), Kind::Comment));
    }
}
