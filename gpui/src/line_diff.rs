// 줄 단위 변경점 — 지금 앱 DiffView.tsx 가 쓰는 jsdiff 9 의 diffLines 를 그대로 옮겼다(node_modules/diff/libesm/diff/base.js,
// line.js). 같은 Myers 라도 갈림길에서 어느 쪽을 고르느냐로 결과가 달라진다 — 그래서 라이브러리를 쓰지 않고
// 알고리즘을 한 줄씩 따랐다(대각선을 버리는 가지치기, 지우기/더하기 고르는 조건, 같은 종류끼리 이어 붙이기까지).
// tools/jsdiff-fixture.mjs 가 진짜 jsdiff 로 만든 픽스처(tests/jsdiff_lines.json)와 대 본다.
//
// 그 위의 diff_rows 는 DiffView.tsx 의 buildRows — 변경 없는 긴 구간(8줄 넘게)을 접고 앞뒤 3줄만 남긴다.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub added: bool,
    pub removed: bool,
    pub count: usize,
    pub value: String,
}

/// line.js 의 tokenize — `/(\n|\r\n)/` 로 나눠 줄 내용과 줄 끝을 한 토큰으로. 끝의 빈 토큰은 버린다
fn tokenize(value: &str) -> Vec<&str> {
    let b = value.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let sep = match b[i] {
            b'\n' => 1,
            b'\r' if b.get(i + 1) == Some(&b'\n') => 2,
            _ => 0,
        };
        if sep > 0 {
            out.push(&value[start..i + sep]);
            i += sep;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < value.len() {
        out.push(&value[start..]);
    }
    // removeEmpty — 위 규칙으로는 빈 토큰이 생기지 않지만 원본과 같게
    out.retain(|t| !t.is_empty());
    out
}

#[derive(Clone, Copy)]
struct Comp {
    count: usize,
    added: bool,
    removed: bool,
    prev: Option<usize>,
}

#[derive(Clone, Copy)]
struct Path {
    old_pos: i64,
    last: Option<usize>,
}

struct Builder {
    comps: Vec<Comp>,
}

impl Builder {
    fn push(&mut self, c: Comp) -> usize {
        self.comps.push(c);
        self.comps.len() - 1
    }

    /// base.js addToPath — 마지막 조각과 종류가 같으면 늘리고, 아니면 새 조각
    fn add_to_path(&mut self, p: Path, added: bool, removed: bool, inc: i64) -> Path {
        let comp = match p.last.map(|i| self.comps[i]) {
            Some(last) if last.added == added && last.removed == removed => Comp {
                count: last.count + 1,
                added,
                removed,
                prev: last.prev,
            },
            _ => Comp {
                count: 1,
                added,
                removed,
                prev: p.last,
            },
        };
        Path {
            old_pos: p.old_pos + inc,
            last: Some(self.push(comp)),
        }
    }

    /// base.js extractCommon — 대각선을 따라 같은 줄을 먹는다. 새 쪽 위치를 돌려준다
    fn extract_common(&mut self, p: &mut Path, new: &[&str], old: &[&str], diag: i64) -> i64 {
        let (nl, ol) = (new.len() as i64, old.len() as i64);
        let mut old_pos = p.old_pos;
        let mut new_pos = old_pos - diag;
        let mut common = 0;
        while new_pos + 1 < nl
            && old_pos + 1 < ol
            && old[(old_pos + 1) as usize] == new[(new_pos + 1) as usize]
        {
            new_pos += 1;
            old_pos += 1;
            common += 1;
        }
        if common > 0 {
            p.last = Some(self.push(Comp {
                count: common,
                added: false,
                removed: false,
                prev: p.last,
            }));
        }
        p.old_pos = old_pos;
        new_pos
    }

    fn build(&self, last: Option<usize>, new: &[&str], old: &[&str]) -> Vec<Part> {
        let mut chain = Vec::new();
        let mut at = last;
        while let Some(i) = at {
            chain.push(self.comps[i]);
            at = self.comps[i].prev;
        }
        chain.reverse();
        let (mut np, mut op) = (0, 0);
        chain
            .into_iter()
            .map(|c| {
                let value = if !c.removed {
                    let v = new[np..np + c.count].concat();
                    np += c.count;
                    if !c.added {
                        op += c.count;
                    }
                    v
                } else {
                    let v = old[op..op + c.count].concat();
                    op += c.count;
                    v
                };
                Part {
                    added: c.added,
                    removed: c.removed,
                    count: c.count,
                    value,
                }
            })
            .collect()
    }
}

/// jsdiff diffLines(old, new) — 옵션 없이
pub fn diff_lines(old_str: &str, new_str: &str) -> Vec<Part> {
    let old = tokenize(old_str);
    let new = tokenize(new_str);
    let (nl, ol) = (new.len() as i64, old.len() as i64);
    let mut b = Builder { comps: Vec::new() };
    let mut best: HashMap<i64, Path> = HashMap::new();
    let mut start = Path {
        old_pos: -1,
        last: None,
    };
    let new_pos = b.extract_common(&mut start, &new, &old, 0);
    if start.old_pos + 1 >= ol && new_pos + 1 >= nl {
        return b.build(start.last, &new, &old);
    }
    best.insert(0, start);
    let (mut min_diag, mut max_diag) = (i64::MIN, i64::MAX);
    let max_edit = nl + ol;
    let mut edit = 1;
    while edit <= max_edit {
        let mut diag = min_diag.max(-edit);
        while diag <= max_diag.min(edit) {
            let remove = best.remove(&(diag - 1));
            let add = best.get(&(diag + 1)).copied();
            let can_add = add.is_some_and(|a| {
                let np = a.old_pos - diag;
                0 <= np && np < nl
            });
            let can_remove = remove.is_some_and(|r| r.old_pos + 1 < ol);
            if !can_add && !can_remove {
                best.remove(&diag);
                diag += 2;
                continue;
            }
            let mut base =
                if !can_remove || (can_add && remove.unwrap().old_pos < add.unwrap().old_pos) {
                    b.add_to_path(add.unwrap(), true, false, 0)
                } else {
                    b.add_to_path(remove.unwrap(), false, true, 1)
                };
            let new_pos = b.extract_common(&mut base, &new, &old, diag);
            if base.old_pos + 1 >= ol && new_pos + 1 >= nl {
                return b.build(base.last, &new, &old);
            }
            best.insert(diag, base);
            if base.old_pos + 1 >= ol {
                max_diag = max_diag.min(diag - 1);
            }
            if new_pos + 1 >= nl {
                min_diag = min_diag.max(diag + 1);
            }
            diag += 2;
        }
        edit += 1;
    }
    Vec::new()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Add(String),
    Del(String),
    Ctx(String),
    /// 접힌 무변경 줄 수
    Fold(usize),
}

const CONTEXT: usize = 3;
const FOLD_MIN: usize = 8;

/// DiffView.tsx 의 buildRows — (줄들, 더한 줄 수, 지운 줄 수)
pub fn diff_rows(old: &str, new: &str) -> (Vec<Row>, usize, usize) {
    let mut raw: Vec<Row> = Vec::new();
    let (mut add, mut del) = (0, 0);
    for p in diff_lines(old, new) {
        let v = p.value.strip_suffix('\n').unwrap_or(&p.value);
        for ln in v.split('\n') {
            let ln = ln.to_string();
            raw.push(if p.added {
                add += 1;
                Row::Add(ln)
            } else if p.removed {
                del += 1;
                Row::Del(ln)
            } else {
                Row::Ctx(ln)
            });
        }
    }
    let mut rows = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if !matches!(raw[i], Row::Ctx(_)) {
            rows.push(raw[i].clone());
            i += 1;
            continue;
        }
        let mut j = i;
        while j < raw.len() && matches!(raw[j], Row::Ctx(_)) {
            j += 1;
        }
        let run = &raw[i..j];
        let (at_start, at_end) = (i == 0, j == raw.len());
        if run.len() > FOLD_MIN {
            let head = if at_start { 0 } else { CONTEXT };
            let tail = if at_end { 0 } else { CONTEXT };
            if !at_start {
                rows.extend_from_slice(&run[..head]);
            }
            let folded = run.len() - head - tail;
            if folded > 0 {
                rows.push(Row::Fold(folded));
            }
            if !at_end {
                rows.extend_from_slice(&run[run.len() - tail..]);
            }
        } else {
            rows.extend_from_slice(run);
        }
        i = j;
    }
    (rows, add, del)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_jsdiff_on_the_recorded_cases() {
        let raw = include_str!("../tests/jsdiff_lines.json");
        let cases: serde_json::Value = serde_json::from_str(raw).unwrap();
        let cases = cases.as_array().unwrap();
        assert!(cases.len() >= 200, "픽스처가 비었다");
        for (n, c) in cases.iter().enumerate() {
            let got = diff_lines(c["old"].as_str().unwrap(), c["new"].as_str().unwrap());
            let want: Vec<Part> = c["parts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Part {
                    added: p["added"].as_bool().unwrap_or(false),
                    removed: p["removed"].as_bool().unwrap_or(false),
                    count: p["count"].as_u64().unwrap() as usize,
                    value: p["value"].as_str().unwrap().to_string(),
                })
                .collect();
            assert_eq!(got, want, "case {n}: {:?} -> {:?}", c["old"], c["new"]);
        }
    }

    #[test]
    fn long_unchanged_runs_fold() {
        let old: String = (0..20).map(|i| format!("l{i}\n")).collect();
        let new = old.replace("l10\n", "L10\n");
        let (rows, add, del) = diff_rows(&old, &new);
        assert_eq!((add, del), (1, 1));
        // 맨 앞 10줄: 앞 문맥 없이 7줄 접고 3줄. 맨 뒤 9줄: 3줄 남기고 6줄 접는다
        assert_eq!(rows[0], Row::Fold(7));
        assert_eq!(
            rows[1..4],
            [
                Row::Ctx("l7".into()),
                Row::Ctx("l8".into()),
                Row::Ctx("l9".into())
            ]
        );
        assert_eq!(rows[4..6], [Row::Del("l10".into()), Row::Add("L10".into())]);
        assert_eq!(rows.last(), Some(&Row::Fold(6)));
        assert_eq!(rows.len(), 10);
    }
}
