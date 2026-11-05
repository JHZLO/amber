// 노트 AI 의 순수 계산 — 지금 앱 lib/mdSections.ts, lib/aiInstruction.ts, lib/aiActivity.ts, lib/aiWait.ts 의 waitLine,
// lib/noteAiRun.ts 의 spliceSpans. 오프셋은 Rust 안에서는 바이트, 하네스 경계(call)에서만 UTF-16 으로 바꾼다
// (JS 문자열 좌표가 UTF-16 이다).

use serde_json::{Value as Json, json};

use crate::i18n::{t, t_with};

fn js_ws(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(js_ws)
}

/// JS 의 `.` 이 못 넘는 줄 끝(\n \r 은 줄을 나눌 때 이미 빠졌다)
fn has_line_sep(s: &str) -> bool {
    s.contains(['\u{2028}', '\u{2029}'])
}

// ---- mdSections.ts ----

#[derive(Debug, Clone, PartialEq)]
pub struct MdSection {
    /// 번호 글머리(`## 2-3. 제목` → "2-3"). 번호가 없으면 None
    pub sec: Option<String>,
    pub level: usize,
    pub title: String,
    /// 소스의 [start, end) — 바이트. 제목 줄부터 다음 제목 직전까지
    pub start: usize,
    pub end: usize,
}

/// mdSecRefs.ts 의 headingSection — `^\s*(\d+(?:-\d+)*)\s*\.`
pub fn heading_section(title: &str) -> Option<String> {
    let t = title.trim_start_matches(js_ws);
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(t.len());
    // 숫자-숫자 꼴로 끝나는 가장 긴 앞부분 — 뒤에 남은 '-' 는 정규식이 되짚어 버린다
    let mut num = &t[..end];
    while num.ends_with('-') {
        num = &num[..num.len() - 1];
    }
    let ok = !num.is_empty()
        && num
            .split('-')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    (ok && t[num.len()..].trim_start_matches(js_ws).starts_with('.')).then(|| num.to_string())
}

/// `^(\s*)(`{3,}|~{3,})(.*)$` → (마커, 정보 문자열)
fn fence_of(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start_matches(js_ws);
    let c = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = rest.len() - rest.trim_start_matches(c).len();
    if n < 3 {
        return None;
    }
    let info = &rest[n..];
    // 앞 공백이 마커를 다 먹지 않게 — `\s*` 는 공백만이라 마커 문자와 겹치지 않는다
    (!has_line_sep(info)).then_some((&rest[..n], info))
}

/// `^(#{1,3}) +(\S.*)$` → (단계, 제목)
fn heading_of(line: &str) -> Option<(usize, &str)> {
    let level = line.len() - line.trim_start_matches('#').len();
    if !(1..=3).contains(&level) {
        return None;
    }
    let after = &line[level..];
    let text = after.trim_start_matches(' ');
    if text.len() == after.len() || has_line_sep(text) {
        return None;
    }
    text.chars().next().filter(|c| !js_ws(*c))?;
    Some((level, text))
}

/// 제목(`#`~`###`) 기준으로 절을 나눈다. 펜스 안의 `#` 은 제목이 아니다. 첫 제목 앞 서문은 절이 아니다
pub fn split_sections(md: &str) -> Vec<MdSection> {
    let mut heads: Vec<(usize, String, usize)> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    let bytes = md.as_bytes();
    let mut i = 0;
    while i <= md.len() {
        // 다음 줄 끝(\r\n | \r | \n)
        let end = md[i..].find(['\r', '\n']).map_or(md.len(), |j| i + j);
        let line = &md[i..end];
        let sep = match bytes.get(end) {
            Some(b'\r') if bytes.get(end + 1) == Some(&b'\n') => 2,
            Some(_) => 1,
            None => 0,
        };
        if let Some((marker, info)) = fence_of(line) {
            let ch = marker.chars().next().unwrap_or('`');
            match fence {
                None => fence = Some((ch, marker.len())),
                Some((c, len)) if ch == c && marker.len() >= len && js_trim(info).is_empty() => {
                    fence = None
                }
                _ => {}
            }
        } else if fence.is_none()
            && let Some((level, text)) = heading_of(line)
        {
            heads.push((level, js_trim(text).to_string(), offset));
        }
        offset += line.len() + sep;
        if sep == 0 {
            break;
        }
        i = end + sep;
    }
    heads
        .iter()
        .enumerate()
        .map(|(k, (level, title, start))| MdSection {
            sec: heading_section(title),
            level: *level,
            title: title.clone(),
            start: *start,
            end: heads.get(k + 1).map_or(md.len(), |h| h.2),
        })
        .collect()
}

/// 맞닿거나 겹치는 구간을 한 덩어리로 — (start, end, 원래 순번들). 시작 순으로 정렬한다(같으면 원래 순서)
pub fn merge_runs(spans: &[(usize, usize)]) -> Vec<(usize, usize, Vec<usize>)> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| spans[i].0);
    let mut runs: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for i in order {
        let (s, e) = spans[i];
        match runs.last_mut() {
            Some(last) if s <= last.1 => {
                last.1 = last.1.max(e);
                last.2.push(i);
            }
            _ => runs.push((s, e, vec![i])),
        }
    }
    runs
}

/// [start, end) 를 갈아끼운다 — 원래 조각의 앞뒤 공백은 그대로, 안쪽만
pub fn splice_span(md: &str, start: usize, end: usize, replacement: &str) -> String {
    let span = &md[start..end];
    let lead = span.len() - span.trim_start_matches(js_ws).len();
    let tail = span.len() - span.trim_end_matches(js_ws).len();
    // 공백뿐인 조각은 lead 와 tail 이 같은 글자를 두 번 센다 — 그땐 껍데기를 버린다
    let body = js_trim(replacement);
    let mid = if lead + tail <= span.len() {
        format!("{}{body}{}", &span[..lead], &span[span.len() - tail..])
    } else {
        body.to_string()
    };
    format!("{}{mid}{}", &md[..start], &md[end..])
}

/// noteAiRun.spliceSpans — 조각들을 뒤에서부터 되끼운다(앞을 먼저 바꾸면 뒤 오프셋이 밀린다). 없는 조각은 건너뛴다
pub fn splice_spans(body: &str, spans: &[(usize, usize)], texts: &[Option<String>]) -> String {
    let mut next = body.to_string();
    for (i, &(s, e)) in spans.iter().enumerate().rev() {
        if let Some(Some(text)) = texts.get(i) {
            next = splice_span(&next, s, e, text);
        }
    }
    next
}

// ---- aiInstruction.ts ----

/// 내가 친 말 → 저장 프롬프트 → 빠른 지시, 빈 조각은 버리고 빈 줄로 잇는다
pub fn compose_instruction(typed: &str, extras: &[String]) -> String {
    std::iter::once(typed)
        .chain(extras.iter().map(String::as_str))
        .map(js_trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 잘린 결과 이어 쓰기 지시 — 끝 조각을 그대로 두고 뒤를 잇게 해서 앞부분이 한 글자도 바뀌지 않는다
pub const CONTINUE_INSTRUCTION: &str = "This fragment is the tail of a note whose generation was cut off by an output limit. \
Return the fragment unchanged, then continue writing from exactly where it stops until the note reaches a natural end: \
finish the current section, then any sections the note's structure clearly still needs, then the closing. \
Do not repeat or restate earlier parts and do not add a preamble.";

pub const TAIL_CHARS: usize = 700;

/// 끝 조각 — 마지막 ~max 자(UTF-16)를 줄 시작에 맞춰 잡는다
pub fn tail_span(text: &str, max: usize) -> String {
    let u: Vec<u16> = text.encode_utf16().collect();
    if u.len() <= max {
        return text.to_string();
    }
    let from = u.len() - max;
    // lastIndexOf("\n", from) — from 자리까지 포함해 뒤에서 찾는다
    let nl = u[..=from].iter().rposition(|&c| c == u16::from(b'\n'));
    let start = match nl {
        None | Some(0) => from,
        Some(i) => i + 1,
    };
    String::from_utf16_lossy(&u[start..])
}

// ---- aiActivity.ts ----

/// 긴 경로는 뒤 두 마디만, 경로가 아니면 60자까지
pub fn short_target(target: &str, keep: usize) -> String {
    let s = js_trim(target);
    if !s.contains('/') {
        let u: Vec<u16> = s.encode_utf16().collect();
        return if u.len() > 60 {
            format!("\u{2026}{}", String::from_utf16_lossy(&u[u.len() - 59..]))
        } else {
            s.to_string()
        };
    }
    let parts: Vec<&str> = s
        .trim_end_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let tail = parts[parts.len().saturating_sub(keep)..].join("/");
    if parts.len() > keep {
        format!("\u{2026}/{tail}")
    } else {
        tail
    }
}

/// 도구 호출 → 한 줄. 대상이 아직 안 왔으면 무엇을 하는지만
pub fn describe_activity(tool: &str, target: Option<&str>) -> String {
    let target = target
        .filter(|t| !t.is_empty())
        .map(|t| short_target(t, 2))
        .unwrap_or_default();
    if target.is_empty() && ["Read", "Glob", "Grep", "LS"].contains(&tool) {
        return t("common.ai.activity.files");
    }
    let with = |k: &str| t_with(k, &[("target", &target)]);
    match tool {
        "Read" => with("common.ai.activity.read"),
        "Glob" | "Grep" | "search" => with("common.ai.activity.search"),
        "LS" => with("common.ai.activity.list"),
        "Write" | "Edit" => with("common.ai.activity.write"),
        "command" | "Bash" => with("common.ai.activity.run"),
        // 지금 앱의 구분자(U+00B7)를 그대로 — 화면 문구가 같아야 한다
        _ if !target.is_empty() => format!("{tool} \u{b7} {target}"),
        _ => t_with("common.ai.activity.tool", &[("tool", tool)]),
    }
}

// ---- aiWait.ts ----

const ROTATE_MS: i64 = 4_000;
const LONG_MS: i64 = 60_000;
const LONGER_MS: i64 = 180_000;

/// 도구가 아닌 단계 표식 — 스트림 이벤트에서 온다
pub const PHASE_TOOLS: [&str; 3] = ["thinking", "thought", "writing"];

fn pool(phase: &str) -> &'static [&'static str] {
    match phase {
        "thinking" => &[
            "common.ai.wait.think1",
            "common.ai.wait.think2",
            "common.ai.wait.think3",
            "common.ai.wait.think4",
        ],
        "thought" => &["common.ai.wait.thought"],
        "writing" => &["common.ai.wait.write"],
        _ => &["common.ai.wait.connect1", "common.ai.wait.connect2"],
    }
}

/// 지금 보일 대기 한 줄 — 도구 호출은 그대로, 단계는 4초마다 돌린다. 1분, 3분이 지나면 꼬리를 단다
pub fn wait_line(
    started_at: i64,
    activity: Option<(&str, Option<&str>)>,
    activity_at: i64,
    has_ref_dirs: bool,
    now: i64,
) -> String {
    let line = match activity {
        Some((tool, target)) if !PHASE_TOOLS.contains(&tool) => describe_activity(tool, target),
        _ => {
            let phase = activity.map_or("connecting", |a| a.0);
            let p = pool(phase);
            let since = if activity.is_some() {
                activity_at
            } else {
                started_at
            };
            let idx = ((now - since).max(0) / ROTATE_MS) as usize % p.len();
            t(p[idx])
        }
    };
    let elapsed = now - started_at;
    if elapsed >= LONGER_MS {
        let k = if has_ref_dirs {
            "common.ai.wait.longerRef"
        } else {
            "common.ai.wait.longer"
        };
        return format!("{line} \u{b7} {}", t(k));
    }
    if elapsed >= LONG_MS {
        return format!("{line} \u{b7} {}", t("common.ai.wait.long"));
    }
    line
}

// ---- 하네스 ----

fn u16_to_byte(s: &str, u: usize) -> usize {
    let mut n = 0;
    for (b, c) in s.char_indices() {
        if n >= u {
            return b;
        }
        n += c.len_utf16();
    }
    s.len()
}

fn byte_to_u16(s: &str, b: usize) -> usize {
    s[..b].encode_utf16().count()
}

fn spans_arg(md: &str, v: Option<&Json>) -> Vec<(usize, usize)> {
    v.and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(|x| {
            let n = |k: &str| x[k].as_u64().unwrap_or(0) as usize;
            (u16_to_byte(md, n("start")), u16_to_byte(md, n("end")))
        })
        .collect()
}

pub fn call(op: &str, args: &[Json]) -> Option<Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let n = |i: usize, d: usize| args.get(i).and_then(Json::as_u64).map_or(d, |v| v as usize);
    let out = match op {
        "mdSections.splitSections" => {
            let md = s(0);
            json!(
                split_sections(md)
                    .into_iter()
                    .map(|x| json!({
                        "sec": x.sec, "level": x.level, "title": x.title,
                        "start": byte_to_u16(md, x.start), "end": byte_to_u16(md, x.end),
                    }))
                    .collect::<Vec<_>>()
            )
        }
        // 좌표만 다룬다 — 하네스는 숫자 구간을 넘기고, 묶인 항목은 원래 객체 그대로 돌려받는다
        "mdSections.mergeRuns" => {
            let items = args
                .first()
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let spans: Vec<(usize, usize)> = items
                .iter()
                .map(|x| {
                    (
                        x["start"].as_u64().unwrap_or(0) as usize,
                        x["end"].as_u64().unwrap_or(0) as usize,
                    )
                })
                .collect();
            json!(
                merge_runs(&spans)
                    .into_iter()
                    .map(|(s, e, ix)| json!({
                        "start": s, "end": e,
                        "items": ix.into_iter().map(|i| items[i].clone()).collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>()
            )
        }
        "mdSections.spliceSpan" => {
            let md = s(0);
            json!(splice_span(
                md,
                u16_to_byte(md, n(1, 0)),
                u16_to_byte(md, n(2, 0)),
                s(3)
            ))
        }
        "noteAiRun.spliceSpans" => {
            let md = s(0);
            let texts: Vec<Option<String>> = args
                .get(2)
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .map(|v| v.as_str().map(str::to_string))
                .collect();
            json!(splice_spans(md, &spans_arg(md, args.get(1)), &texts))
        }
        "aiInstruction.composeInstruction" => {
            let extras: Vec<String> = args
                .get(1)
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            json!(compose_instruction(s(0), &extras))
        }
        "aiInstruction.tailSpan" => json!(tail_span(s(0), n(1, TAIL_CHARS))),
        "aiActivity.shortTarget" => json!(short_target(s(0), n(1, 2))),
        "aiActivity.describeActivity" => {
            let a = args.first().cloned().unwrap_or(Json::Null);
            json!(describe_activity(
                a["tool"].as_str().unwrap_or_default(),
                a["target"].as_str()
            ))
        }
        "aiWait.waitLine" => {
            let w = args.first().cloned().unwrap_or(Json::Null);
            let act = w["activity"].as_object().map(|a| {
                (
                    a.get("tool").and_then(Json::as_str).unwrap_or_default(),
                    a.get("target").and_then(Json::as_str),
                )
            });
            json!(wait_line(
                w["startedAt"].as_i64().unwrap_or(0),
                act,
                w["activityAt"].as_i64().unwrap_or(0),
                w["hasRefDirs"].as_bool().unwrap_or(false),
                args.get(1).and_then(Json::as_i64).unwrap_or(0),
            ))
        }
        _ => return None,
    };
    Some(Ok(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_skip_fenced_headings_and_keep_crlf_offsets() {
        let md = "intro\r\n# 1. A\r\n```bash\r\n# not\r\n```\r\n## 1-2. B\r\ntext\r\n";
        let s = split_sections(md);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].sec.as_deref(), Some("1"));
        assert_eq!(&md[s[0].start..s[0].start + 8], "# 1. A\r\n");
        assert_eq!(s[1].title, "1-2. B");
        assert_eq!(s[0].end, s[1].start);
        assert_eq!(s[1].end, md.len());
    }

    #[test]
    fn splice_keeps_the_blank_lines_around_the_span() {
        let md = "a\n\n## X\nold\n\n## Y\n";
        let s = split_sections(md);
        let out = splice_span(md, s[0].start, s[0].end, "## X\nnew");
        assert_eq!(out, "a\n\n## X\nnew\n\n## Y\n");
        assert_eq!(
            splice_span("a   b", 1, 4, " z "),
            "azb",
            "공백뿐인 조각은 껍데기를 버린다"
        );
    }

    #[test]
    fn tail_starts_at_a_line() {
        // 자르는 자리(from)가 든 줄의 처음으로 물러난다 — 그래서 max 보다 길 수 있다
        let text = format!("head\n{}\nlast", "x".repeat(30));
        assert_eq!(tail_span(&text, 10), format!("{}\nlast", "x".repeat(30)));
        assert_eq!(tail_span("short", 700), "short");
        assert_eq!(
            tail_span(&"y".repeat(30), 10),
            "y".repeat(10),
            "줄바꿈이 없으면 그냥 끝 max 자"
        );
    }

    #[test]
    fn heading_sections_like_the_regex() {
        assert_eq!(heading_section("1-2. x").as_deref(), Some("1-2"));
        assert_eq!(heading_section(" 3 . x").as_deref(), Some("3"));
        assert_eq!(heading_section("1-2-. x"), None);
        assert_eq!(heading_section("1-. x"), None);
        assert_eq!(heading_section("x 1."), None);
    }
}
