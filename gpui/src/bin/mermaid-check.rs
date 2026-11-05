// 코퍼스의 mermaid 를 crate::mermaid 로 그려 지금 앱(mermaid.js)이 그린 정답과 견준다. 그림마다 넷을 본다:
//  - 그려지나 (정답은 그려졌는데 여기서 실패)
//  - 글자가 같나 — 라벨 글자를 공백 빼고 다중집합으로 비교한다(아래 chars)
//  - 글자끼리 겹치나 — 작은 상자 넓이의 20% 넘게 겹치는 쌍이 정답보다 많으면 문제
//  - 글자가 뒤에 그린 도형에 가려지나
//
// 코퍼스: node gpui/tools/mermaid-corpus.mjs <vault> <corpus.json>
// 정답:   브라우저 하네스에서 앱의 renderMermaid 로 코퍼스를 그린 결과(로컬 전용 .claude/harness/mermaid-ref.*)
//
//   cargo run --release --bin mermaid-check -- <corpus.json> <reference.json> <out-dir> [--png]

use std::collections::HashMap;
use std::sync::Arc;

use amber_gpui::mermaid::to_svg;
use resvg::{tiny_skia, usvg};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Entry {
    id: String,
    kind: String,
    source: String,
}

#[derive(Deserialize)]
struct Reference {
    id: String,
    ok: bool,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    overlaps: usize,
}

struct Rect {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

impl Rect {
    fn area(&self) -> f32 {
        (self.r - self.l).max(0.0) * (self.b - self.t).max(0.0)
    }
    fn overlap(&self, o: &Rect) -> f32 {
        let w = self.r.min(o.r) - self.l.max(o.l);
        let h = self.b.min(o.b) - self.t.max(o.t);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }
}

fn rect(r: usvg::Rect) -> Rect {
    Rect {
        l: r.left(),
        t: r.top(),
        r: r.right(),
        b: r.bottom(),
    }
}

/// 그리는 순서대로 글자와 칠한 도형을 모은다
fn collect(
    g: &usvg::Group,
    order: &mut usize,
    texts: &mut Vec<(usize, String, Rect)>,
    fills: &mut Vec<(usize, Rect)>,
) {
    for n in g.children() {
        *order += 1;
        match n {
            usvg::Node::Group(g) => collect(g, order, texts, fills),
            usvg::Node::Text(t) => {
                let s = t
                    .chunks()
                    .iter()
                    .map(|c| c.text())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !s.trim().is_empty() {
                    texts.push((*order, s, rect(t.abs_bounding_box())));
                }
            }
            usvg::Node::Path(p) => {
                if p.is_visible() && p.fill().is_some_and(|f| f.opacity().get() > 0.0) {
                    fills.push((*order, rect(p.abs_bounding_box())));
                }
            }
            _ => {}
        }
    }
}

/// 공백을 뺀 글자 다중집합. 브라우저는 한글을 음절 사이에서도 줄바꿈하고 innerText 에는 그 줄바꿈이 안 보인다 —
/// 낱말로 견주면 줄바꿈 위치만 달라도 틀린 것으로 잡혀서, 글자 단위로 본다(빠진 라벨, 남는 따옴표는 그대로 잡힌다)
fn chars<'a>(labels: impl Iterator<Item = &'a str>) -> HashMap<char, i32> {
    let mut m = HashMap::new();
    for l in labels {
        for c in l.chars().filter(|c| !c.is_whitespace()) {
            *m.entry(c).or_insert(0) += 1;
        }
    }
    m
}

/// a 에 b 보다 많은 글자들
fn surplus(a: &HashMap<char, i32>, b: &HashMap<char, i32>) -> String {
    let mut out: Vec<char> = a
        .iter()
        .flat_map(|(c, n)| {
            std::iter::repeat_n(*c, (n - b.get(c).copied().unwrap_or(0)).max(0) as usize)
        })
        .collect();
    out.sort();
    out.into_iter().collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, corpus, reference, out, ..] = args.as_slice() else {
        eprintln!("usage: mermaid-check <corpus.json> <reference.json> <out-dir> [--png]");
        std::process::exit(2);
    };
    let png = args.iter().any(|a| a == "--png");
    let corpus: Vec<Entry> =
        serde_json::from_str(&std::fs::read_to_string(corpus).unwrap()).unwrap();
    let reference: HashMap<String, Reference> =
        serde_json::from_str::<Vec<Reference>>(&std::fs::read_to_string(reference).unwrap())
            .unwrap()
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
    std::fs::create_dir_all(out).unwrap();

    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    let options = usvg::Options {
        fontdb: Arc::new(db),
        font_family: "Apple SD Gothic Neo".into(),
        ..Default::default()
    };

    let mut rows = Vec::new();
    let mut tally: HashMap<&str, [usize; 6]> = HashMap::new(); // 그림, 실패, 글자 다름, 겹침, 가려짐, 깨끗
    for (i, e) in corpus.iter().enumerate() {
        let Some(r) = reference.get(&e.id) else {
            continue;
        };
        if !r.ok {
            continue; // 지금 앱도 못 그리는 것은 기준이 없다
        }
        let t = tally.entry(e.kind.as_str()).or_default();
        t[0] += 1;
        let svg = match to_svg(&e.source) {
            Ok(s) => s,
            Err(err) => {
                t[1] += 1;
                rows.push(json!({"id": e.id, "kind": e.kind, "error": err}));
                continue;
            }
        };
        let tree = match usvg::Tree::from_str(&svg, &options) {
            Ok(t) => t,
            Err(err) => {
                t[1] += 1;
                rows.push(json!({"id": e.id, "kind": e.kind, "error": format!("svg: {err}")}));
                continue;
            }
        };
        let (mut texts, mut fills, mut order) = (Vec::new(), Vec::new(), 0);
        collect(tree.root(), &mut order, &mut texts, &mut fills);

        let want = chars(r.labels.iter().map(String::as_str));
        let got = chars(texts.iter().map(|(_, s, _)| s.as_str()));
        let missing = surplus(&want, &got);
        let extra = surplus(&got, &want);

        let mut overlaps = 0;
        for a in 0..texts.len() {
            for b in a + 1..texts.len() {
                let small = texts[a].2.area().min(texts[b].2.area());
                if small > 0.0 && texts[a].2.overlap(&texts[b].2) / small > 0.2 {
                    overlaps += 1;
                }
            }
        }
        let occluded: Vec<String> = texts
            .iter()
            .filter(|(o, _, tr)| {
                fills.iter().any(|(fo, fr)| {
                    fo > o
                        && fr.area() < tr.area() * 30.0
                        && tr.overlap(fr) / tr.area().max(1.0) > 0.3
                })
            })
            .map(|(_, s, _)| s.clone())
            .collect();

        let label_bad = !missing.is_empty() || !extra.is_empty();
        let overlap_bad = overlaps > r.overlaps;
        if label_bad {
            t[2] += 1;
        }
        if overlap_bad {
            t[3] += 1;
        }
        if !occluded.is_empty() {
            t[4] += 1;
        }
        if !label_bad && !overlap_bad && occluded.is_empty() {
            t[5] += 1;
        }
        rows.push(json!({
            "id": e.id, "kind": e.kind,
            "missing": missing, "extra": extra,
            "overlaps": overlaps, "ref_overlaps": r.overlaps,
            "occluded": occluded,
        }));

        if png {
            let size = tree.size();
            if let Some(mut pm) = tiny_skia::Pixmap::new(
                (size.width() * 2.0) as u32 + 1,
                (size.height() * 2.0) as u32 + 1,
            ) {
                pm.fill(tiny_skia::Color::WHITE);
                resvg::render(
                    &tree,
                    tiny_skia::Transform::from_scale(2.0, 2.0),
                    &mut pm.as_mut(),
                );
                let _ = pm.save_png(format!("{out}/{i:03}.png"));
            }
        }
    }

    std::fs::write(
        format!("{out}/report.json"),
        serde_json::to_string_pretty(&rows).unwrap(),
    )
    .unwrap();
    let mut kinds: Vec<_> = tally.iter().collect();
    kinds.sort_by_key(|(k, _)| *k);
    println!(
        "{:<18} {:>4} {:>4} {:>6} {:>4} {:>6} {:>4}",
        "종류", "그림", "실패", "글자", "겹침", "가려짐", "깨끗"
    );
    let mut sum = [0usize; 6];
    for (k, t) in kinds {
        println!(
            "{:<18} {:>4} {:>4} {:>6} {:>4} {:>6} {:>4}",
            k, t[0], t[1], t[2], t[3], t[4], t[5]
        );
        for i in 0..6 {
            sum[i] += t[i];
        }
    }
    println!(
        "{:<18} {:>4} {:>4} {:>6} {:>4} {:>6} {:>4}",
        "합계", sum[0], sum[1], sum[2], sum[3], sum[4], sum[5]
    );
}
