// 노트 본문을 글 덩어리와 그림으로 가른다. 그림은 ```svg / ```mermaid 펜스(지금 앱의 mdSvg, mdMermaid 와 같은 표기),
// 나머지 펜스(코드)는 글 덩어리에 그대로 남겨 마크다운 렌더러가 그린다.
// 글 덩어리는 제목(#~###)마다 새로 시작한다 — 절 링크가 스크롤해 갈 제목이 판의 직계 자식이어야 해서다.

use std::borrow::Cow;
use std::ops::Range;

/// 줄 번호를 단 줄들 — split 이 덩어리마다 원래 몇째 줄이었는지 남긴다
type Lines<'a> = std::iter::Peekable<std::iter::Enumerate<std::str::Lines<'a>>>;

pub enum Segment {
    Markdown(String),
    Svg(String),
    Mermaid(String),
    /// `<details><summary>제목</summary> … </details>` — 접고 펴는 카드(lib/mdDetails.ts).
    /// 안쪽은 같은 규칙으로 다시 쪼갠다. 닫는 태그가 없으면 끝까지 접힌다(브라우저와 같다)
    Toggle {
        summary: Option<String>,
        open: bool,
        body: String,
    },
}

/// `<details>` 로 시작하는 줄이면 (open 속성, 태그 뒤의 나머지) — `<details-panel>` 같은 다른 태그는 아니다
fn details_open(line: &str) -> Option<(bool, &str)> {
    let t = line.trim_start();
    let rest = t.strip_prefix("<details")?;
    let end = rest.find('>')?;
    let attrs = &rest[..end];
    if !(attrs.is_empty() || attrs.starts_with(char::is_whitespace)) {
        return None;
    }
    let open = attrs
        .split_whitespace()
        .any(|a| a == "open" || a.starts_with("open="));
    Some((open, &rest[end + 1..]))
}

/// `<summary>제목</summary>` 의 제목 — 안의 태그는 걷어낸다
fn summary_of(s: &str) -> Option<String> {
    let start = s.find("<summary")?;
    let after = &s[start..];
    let body = &after[after.find('>')? + 1..];
    let end = body.find("</summary")?;
    static TAG: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let tag = TAG.get_or_init(|| regex::Regex::new(r"<[^>]*>").unwrap());
    Some(tag.replace_all(&body[..end], "").trim().to_string())
}

/// 토글 하나를 거둔다 — 첫 줄(여는 태그) 다음부터 짝 맞는 `</details>` 까지. 펜스 안의 태그는 세지 않는다
fn take_toggle(first_rest: &str, lines: &mut Lines<'_>) -> (Option<String>, String, Vec<usize>) {
    let mut summary = summary_of(first_rest);
    let mut body = String::new();
    let mut idx = Vec::new();
    let mut depth = 1;
    let mut code: Option<(char, usize)> = None;
    // 제목은 여는 태그 바로 뒤(빈 줄 전)에서만 — 본문 중간의 <summary> 는 글로 둔다
    let mut head = summary.is_none();
    for (i, line) in lines.by_ref() {
        if code.is_none() {
            if head && summary.is_none() {
                if let Some(t) = summary_of(line) {
                    summary = Some(t);
                    head = false;
                    continue;
                }
                if !line.trim().is_empty() {
                    head = false;
                }
            }
            let t = line.trim();
            if details_open(t).is_some() {
                depth += 1;
            } else if t.starts_with("</details") {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
        match (fence(line), code) {
            (Some((c, n, _)), None) => code = Some((c, n)),
            (Some((c, n, info)), Some((open, len))) if c == open && n >= len && info.is_empty() => {
                code = None
            }
            _ => {}
        }
        body.push_str(line);
        body.push('\n');
        idx.push(i);
    }
    (summary, body, idx)
}

pub fn split(md: &str) -> Vec<Segment> {
    split_lines(md).into_iter().map(|(s, _)| s).collect()
}

/// split 과 같고, 덩어리마다 그 글의 줄이 md 의 몇째 줄이었는지(글 덩어리는 글의 줄, 토글은 본문의 줄, 그림은 빈 목록).
/// 줄 안의 글은 바뀌어도(절 링크) 줄은 하나씩 대응한다 — 읽기 화면의 소스 좌표를 노트 좌표로 되돌리는 데 쓴다
pub fn split_lines(md: &str) -> Vec<(Segment, Vec<usize>)> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut idx: Vec<usize> = Vec::new();
    // 그림이 아닌 코드 펜스 안인지 — 여는 펜스의 문자와 길이(mdSections.ts 와 같은 닫기 판정)
    let mut code: Option<(char, usize)> = None;
    let mut lines = md.lines().enumerate().peekable();
    while let Some((i, line)) = lines.next() {
        if code.is_none()
            && let Some((open, rest)) = details_open(line)
        {
            let (summary, body, body_idx) = take_toggle(rest, &mut lines);
            if !text.trim().is_empty() {
                out.push((
                    Segment::Markdown(std::mem::take(&mut text)),
                    std::mem::take(&mut idx),
                ));
            }
            text.clear();
            idx.clear();
            out.push((
                Segment::Toggle {
                    summary,
                    open,
                    body,
                },
                body_idx,
            ));
            continue;
        }
        let lang = line.trim_start().strip_prefix("```").map(str::trim);
        if let (None, Some(lang @ ("svg" | "mermaid"))) = (code, lang) {
            let mut body = String::new();
            for (_, inner) in lines.by_ref() {
                if inner.trim() == "```" {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            if !text.trim().is_empty() {
                out.push((
                    Segment::Markdown(std::mem::take(&mut text)),
                    std::mem::take(&mut idx),
                ));
            }
            text.clear();
            idx.clear();
            out.push((
                if lang == "svg" {
                    Segment::Svg(body)
                } else {
                    Segment::Mermaid(body)
                },
                Vec::new(),
            ));
            continue;
        }
        let was_code = code.is_some();
        match (fence(line), code) {
            (Some((c, n, _)), None) => code = Some((c, n)),
            (Some((c, n, info)), Some((open, len))) if c == open && n >= len && info.is_empty() => {
                code = None
            }
            (None, None) if heading(line).is_some() && !text.trim().is_empty() => {
                out.push((
                    Segment::Markdown(std::mem::take(&mut text)),
                    std::mem::take(&mut idx),
                ));
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
        idx.push(i);
    }
    if !text.trim().is_empty() {
        out.push((Segment::Markdown(text), idx));
    }
    out
}

/// str::lines 의 줄마다 md 안의 자리(줄바꿈 \n, \r\n 은 빼고)
pub fn line_ranges(md: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in md.split_inclusive('\n') {
        let content = match piece.strip_suffix('\n') {
            Some(c) => c.strip_suffix('\r').unwrap_or(c),
            None => piece,
        };
        out.push(start..start + content.len());
        start += piece.len();
    }
    out
}

/// 그 자리를 품은 가장 안쪽 '고칠 수 있는 블록'(mdBlocks.ts 의 srcAttrs + closest) — 문단, 제목(#~###), 목록 항목,
/// 표, 인용/알림, 코드 펜스 가운데 줄 머리에서 시작해 줄 끝에서 끝나는 것. 인용 안의 문단은 `> ` 뒤에서 시작해
/// 그 구간만 갈아 끼우면 접두사가 남아 깨지므로 건너뛰고 바깥 인용이 대상이 된다
pub fn block_span(md: &str, at: usize) -> Option<Range<usize>> {
    use markdown::mdast::Node as N;
    fn walk(n: &N, md: &[u8], at: usize, best: &mut Option<Range<usize>>) {
        if let Some(p) = n.position() {
            let (s, mut e) = (p.start.offset, p.end.offset);
            if at < s || at >= e {
                return;
            }
            // markdown-rs 는 마지막 목록 항목의 끝을 줄바꿈 뒤로 잡는다(remark 는 글 끝) — 갈아 끼울 때 빈 줄을 먹지 않게
            while e > s && matches!(md.get(e - 1), Some(b'\n' | b'\r')) {
                e -= 1;
            }
            let tagged = match n {
                N::Paragraph(_) | N::ListItem(_) | N::Table(_) | N::Blockquote(_) | N::Code(_) => {
                    true
                }
                N::Heading(h) => h.depth <= 3,
                _ => false,
            };
            let line_start = s == 0 || md.get(s - 1) == Some(&b'\n');
            let line_end = e >= md.len() || md.get(e) == Some(&b'\n');
            if tagged && line_start && line_end && e > s {
                *best = Some(s..e);
            }
        }
        for c in n.children().into_iter().flatten() {
            walk(c, md, at, best);
        }
    }
    let tree = markdown::to_mdast(md, &markdown::ParseOptions::gfm()).ok()?;
    let mut best = None;
    walk(&tree, md.as_bytes(), at, &mut best);
    best
}

/// 고른 소스 구간이 걸친 블록들을 아우르는 구간(mdBlocks.ts 의 blockRangeFromSelection) — 못 찾으면 None
pub fn selection_block(md: &str, sel: Range<usize>) -> Option<Range<usize>> {
    let a = block_span(md, sel.start);
    let b = block_span(md, sel.end.saturating_sub(1).max(sel.start));
    match (a, b) {
        (Some(a), Some(b)) => Some(a.start.min(b.start)..a.end.max(b.end)),
        (a, b) => a.or(b),
    }
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
    fn details_become_toggles_with_nested_bodies() {
        let md = "앞\n\n<details open>\n<summary>제목 <b>굵게</b></summary>\n\n본문\n\n<details>\n<summary>안</summary>\n\n깊은 글\n\n</details>\n\n</details>\n\n뒤\n";
        let seg = split(md);
        assert_eq!(seg.len(), 3);
        let Segment::Toggle {
            summary,
            open,
            body,
        } = &seg[1]
        else {
            panic!("토글")
        };
        assert_eq!(summary.as_deref(), Some("제목 굵게"));
        assert!(*open);
        let inner = split(body);
        assert!(
            matches!(&inner[1], Segment::Toggle { summary: Some(s), open: false, .. } if s == "안")
        );
        // 펜스 안의 태그는 토글이 아니다
        let code = "```html\n<details>\n```\n";
        assert!(matches!(split(code).as_slice(), [Segment::Markdown(_)]));
        // 닫는 태그가 없으면 끝까지
        let open_end = split("<details>\n<summary>t</summary>\n\n남은 글\n");
        assert!(matches!(&open_end[0], Segment::Toggle { body, .. } if body.contains("남은 글")));
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

    // mdBlocks.test.ts 의 srcAttrs — 바이트 자리로
    #[test]
    fn blocks_that_start_and_end_on_line_boundaries_are_editable() {
        use super::block_span;
        let md = "# 제목\n\n문단.\n\n> [!NOTE]\n> 알림.\n";
        let h = "# 제목".len();
        assert_eq!(block_span(md, 0), Some(0..h));
        let p = md.find("문단").unwrap();
        assert_eq!(block_span(md, p + 3), Some(p..p + "문단.".len()));
        // 인용 안 문단은 `> ` 뒤에서 시작 — 바깥 인용(알림)이 대상
        let q = md.find("> [!NOTE]").unwrap();
        let inner = md.find("알림").unwrap();
        assert_eq!(block_span(md, inner), Some(q..md.len() - 1));
        // 빈 줄(블록 사이)은 없다
        assert_eq!(block_span(md, h + 1), None);
    }

    #[test]
    fn list_items_tables_code_and_shallow_headings_only() {
        use super::{block_span, selection_block};
        let md = "#### 넷째\n\n- 하나\n- 둘\n\n```rs\nlet x;\n```\n";
        assert_eq!(block_span(md, 2), None, "#### 는 고칠 블록이 아니다");
        let one = md.find("- 하나").unwrap();
        let two = md.find("- 둘").unwrap();
        assert_eq!(block_span(md, one + 3), Some(one..one + "- 하나".len()));
        // 두 항목에 걸친 선택은 둘을 아우른다
        assert_eq!(
            selection_block(md, one + 3..two + 5),
            Some(one..two + "- 둘".len())
        );
        let code = md.find("```rs").unwrap();
        assert_eq!(block_span(md, code + 7), Some(code..md.len() - 1));
    }

    #[test]
    fn split_lines_keeps_each_line_number_through_toggles_and_figures() {
        use super::{Segment, line_ranges, split_lines};
        let md = "# A\n[[1]] x\n\n<details>\n<summary>T</summary>\n\nin\n</details>\n```svg\n<svg/>\n```\ntail\n";
        let segs = split_lines(md);
        let idx: Vec<&Vec<usize>> = segs.iter().map(|(_, i)| i).collect();
        assert_eq!(
            idx[0],
            &vec![0, 1, 2],
            "글 덩어리 — 절 링크로 글이 바뀐 줄도 한 줄"
        );
        assert!(matches!(segs[1].0, Segment::Toggle { .. }));
        assert_eq!(idx[1], &vec![5, 6], "토글 본문(제목 줄은 빠진다)");
        assert!(matches!(segs[2].0, Segment::Svg(_)) && idx[2].is_empty());
        assert_eq!(idx[3], &vec![11]);
        let lines = line_ranges("a\r\nbb\n\nc");
        assert_eq!(lines, vec![0..1, 3..5, 6..6, 7..8]);
    }
}
