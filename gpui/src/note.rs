// 노트 본문을 글 덩어리와 그림으로 가른다. 그림은 ```svg / ```mermaid 펜스(지금 앱의 mdSvg, mdMermaid 와 같은 표기),
// 나머지 펜스(코드)는 글 덩어리에 그대로 남겨 마크다운 렌더러가 그린다.
// 글 덩어리는 제목(#~###)마다 새로 시작한다 — 절 링크가 스크롤해 갈 제목이 판의 직계 자식이어야 해서다.

use std::borrow::Cow;

pub enum Segment {
    Markdown(String),
    Svg(String),
    Mermaid(String),
}

pub fn split(md: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut text = String::new();
    // 그림이 아닌 코드 펜스 안인지 — 여는 펜스의 문자와 길이(mdSections.ts 와 같은 닫기 판정)
    let mut code: Option<(char, usize)> = None;
    let mut lines = md.lines();
    while let Some(line) = lines.next() {
        let lang = line.trim_start().strip_prefix("```").map(str::trim);
        if let (None, Some(lang @ ("svg" | "mermaid"))) = (code, lang) {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            if !text.trim().is_empty() {
                out.push(Segment::Markdown(std::mem::take(&mut text)));
            }
            text.clear();
            out.push(if lang == "svg" {
                Segment::Svg(body)
            } else {
                Segment::Mermaid(body)
            });
            continue;
        }
        let was_code = code.is_some();
        match (fence(line), code) {
            (Some((c, n, _)), None) => code = Some((c, n)),
            (Some((c, n, info)), Some((open, len))) if c == open && n >= len && info.is_empty() => {
                code = None
            }
            (None, None) if heading(line).is_some() && !text.trim().is_empty() => {
                out.push(Segment::Markdown(std::mem::take(&mut text)));
            }
            _ => {}
        }
        let line = if was_code || code.is_some() {
            Cow::Borrowed(line)
        } else {
            link_sec_refs(line)
        };
        text.push_str(&line);
        text.push('\n');
    }
    if !text.trim().is_empty() {
        out.push(Segment::Markdown(text));
    }
    out
}

/// 글 덩어리가 번호 단 제목으로 시작하면 그 번호 — 절 링크의 목적지다
pub fn section_of(md: &str) -> Option<&str> {
    let first = md.lines().find(|l| !l.trim().is_empty())?;
    section(heading(first)?)
}

/// 덩어리 첫 줄이 `#`~`###` 제목이면 (단계, 보이는 글자) — 목차가 쓴다(지금 앱은 h1~h3 의 textContent).
/// 강조, 코드 표식과 링크 주소는 떼고 글자만 남긴다
pub fn heading_of(md: &str) -> Option<(u8, String)> {
    let first = md.lines().find(|l| !l.trim().is_empty())?;
    let title = heading(first)?;
    let level = (first.len() - first.trim_start_matches('#').len()) as u8;
    static LINK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let link = LINK.get_or_init(|| regex::Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
    let text = link.replace_all(title, "$1");
    let text: String = text.chars().filter(|c| !matches!(c, '*' | '`')).collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some((level, text))
}

/// 코드 펜스 줄이면 (문자, 길이, 정보 문자열) — mdSections.ts 의 FENCE
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.len() - t.trim_start_matches(c).len();
    (n >= 3).then(|| (c, n, t[n..].trim()))
}

/// `#`~`###` 제목 줄이면 제목 글자 — mdSections.ts 의 HEADING
fn heading(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    let rest = &line[hashes..];
    let title = rest.trim_start_matches(' ');
    let ok = (1..=3).contains(&hashes)
        && title.len() < rest.len()
        && title.starts_with(|c: char| !c.is_whitespace());
    ok.then_some(title)
}

/// 제목 앞의 절 번호 — "1-2. 커밋은 스칼라" → "1-2" (mdSecRefs.ts 의 headingSection)
fn section(title: &str) -> Option<&str> {
    let t = title.trim_start();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(t.len());
    let num = &t[..end];
    (number(num) && t[end..].trim_start().starts_with('.')).then_some(num)
}

/// 1, 1-2, 1-2-3 꼴인지
fn number(s: &str) -> bool {
    !s.is_empty()
        && s.split('-')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `[[1-2]]`, `[[1-2|앞 절]]` 을 `[앞 절](#1-2)` 로 바꾼다(mdSecRefs.ts 의 표기). 지금 앱처럼 `코드` 안은 그대로 둔다
fn link_sec_refs(line: &str) -> Cow<'_, str> {
    if !line.contains("[[") {
        return Cow::Borrowed(line);
    }
    let parts: Vec<String> = line
        .split('`')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                return part.to_string();
            }
            let (mut out, mut rest) = (String::new(), part);
            while let Some(at) = rest.find("[[") {
                let after = &rest[at + 2..];
                let hit = after
                    .find("]]")
                    .and_then(|end| sec_ref(&after[..end]).map(|r| (end, r)));
                match hit {
                    Some((end, (sec, label))) => {
                        out.push_str(&rest[..at]);
                        out.push_str(&format!("[{label}](#{sec})"));
                        rest = &after[end + 2..];
                    }
                    None => {
                        out.push_str(&rest[..at + 1]);
                        rest = &rest[at + 1..];
                    }
                }
            }
            out.push_str(rest);
            out
        })
        .collect();
    Cow::Owned(parts.join("`"))
}

/// `1-2` 또는 `1-2|앞 절` → (번호, 보일 글자). 별칭이 비면 번호가 보인다
fn sec_ref(inner: &str) -> Option<(&str, &str)> {
    if inner.contains(']') {
        return None;
    }
    let (num, label) = match inner.split_once('|') {
        Some((num, label)) if !label.is_empty() && !label.contains('|') => (num, Some(label)),
        Some(_) => return None,
        None => (inner, None),
    };
    let num = num.trim();
    if !number(num) {
        return None;
    }
    let label = label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or(num);
    Some((num, label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(md: &str) -> Vec<String> {
        split(md)
            .into_iter()
            .filter_map(|s| match s {
                Segment::Markdown(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn splits_at_headings_but_not_inside_code() {
        let md = "서문\n## 1-1. 첫 절\n본문\n```bash\n# 설치\n```\n### 1-1-1. 아래\n끝\n";
        let t = texts(md);
        assert_eq!(t.len(), 3);
        assert!(t[1].contains("# 설치"));
        assert_eq!(section_of(&t[1]), Some("1-1"));
        assert_eq!(section_of(&t[2]), Some("1-1-1"));
        assert_eq!(section_of(&t[0]), None);
    }

    #[test]
    fn longer_fence_is_not_closed_by_shorter() {
        let md = "````md\n```\n## 안\n````\n## 2. 밖\n";
        let t = texts(md);
        assert_eq!(t.len(), 2);
        assert_eq!(section_of(&t[1]), Some("2"));
    }

    #[test]
    fn heading_rules_match_md_sections() {
        assert_eq!(heading("## 1-2. 제목"), Some("1-2. 제목"));
        assert_eq!(heading("#### 넷"), None);
        assert_eq!(heading("#붙음"), None);
        assert_eq!(section("1-2. 커밋"), Some("1-2"));
        assert_eq!(section("들어가기 전"), None);
        assert_eq!(section("1-. 깨짐"), None);
        assert_eq!(section("2026 회고"), None);
    }

    #[test]
    fn links_sec_refs_like_md_sec_refs() {
        assert_eq!(
            link_sec_refs("[[1-4|앞 절]]에서 본"),
            "[앞 절](#1-4)에서 본"
        );
        assert_eq!(
            link_sec_refs("[[ 2-3 ]]과 [[3|  ]]"),
            "[2-3](#2-3)과 [3](#3)"
        );
        assert_eq!(link_sec_refs("`[[1-2]]` 는 코드"), "`[[1-2]]` 는 코드");
        assert_eq!(link_sec_refs("[[[1-2]]"), "[[1-2](#1-2)");
        assert_eq!(
            link_sec_refs("[[메모]] [[1-2|]] [[1|a|b]]"),
            "[[메모]] [[1-2|]] [[1|a|b]]"
        );
    }

    #[test]
    fn sec_refs_stay_in_code_blocks() {
        let t = texts("```\n[[1-2]]\n```\n[[1-2]]\n");
        assert_eq!(t[0], "```\n[[1-2]]\n```\n[1-2](#1-2)\n");
    }
}
