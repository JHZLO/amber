// src/lib/ttLayout.ts 와 timeBlocks.ts 의 순수 함수를 옮긴 것 — 한 날짜 칸 안에서 블록의 가로 위치를 정한다.
//   포함(09–18 안의 10–11) = 들여쓰고 위에 얹는다(칸을 먹지 않는다)
//   교차(09–12 와 11–14) = 칸을 똑같이 나눈다
// 칸 오른쪽 GUTTER 는 늘 맨 바탕으로 남겨 '빈 곳을 세로로 끌어 만들기'가 닿게 한다.
// 부동소수 꼬리까지 지금 앱과 같도록 계산 순서를 그대로 둔다(하네스가 숫자를 글자 단위로 견준다).

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

pub const GUTTER: f64 = 12.;
pub const CASCADE_INSET: f64 = 14.;
pub const MAX_DEPTH: i64 = 3;
pub const MIN_LANE: f64 = 56.;
pub const BLOCK_GAP: f64 = 2.;
pub const GHOST_ID: i64 = -1;

/// 컬럼 폭은 %, 거터와 들여쓰기는 px 이라 둘을 함께 든다 — calc(pctL% + pxL px)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Box {
    pub pct_l: f64,
    pub px_l: f64,
    pub pct_w: f64,
    pub px_w: f64,
}

/// 블록은 DB 행(JSON) 그대로 들고 다닌다 — 배치 결과에도 받은 행이 그대로 나간다
fn id(b: &Json) -> i64 {
    b["id"].as_i64().unwrap_or_default()
}
fn start(b: &Json) -> f64 {
    b["start_min"].as_f64().unwrap_or_default()
}
fn end(b: &Json) -> f64 {
    b["end_min"].as_f64().unwrap_or_default()
}

/// 드래그 중 위치. id 가 None 이면 '만드는 중'(아직 DB 에 없음)
#[derive(Deserialize, Clone, Debug)]
pub struct GhostAt {
    pub id: Option<i64>,
    pub date: String,
    pub start: f64,
    pub end: f64,
}

/// 드래그 중인 위치를 반영한 미리보기 목록 — 침범당한 블록이 놓기 전에 밀린다
pub fn with_ghost(blocks: &[Json], ghost: Option<&GhostAt>) -> Vec<Json> {
    let Some(g) = ghost else {
        return blocks.to_vec();
    };
    match g.id {
        None => {
            let mut out = blocks.to_vec();
            out.push(json!({
                "id": GHOST_ID, "date": g.date, "start_min": g.start, "end_min": g.end,
                "title": "", "todo_id": null, "created_at": 0, "updated_at": 0,
            }));
            out
        }
        Some(gid) => blocks
            .iter()
            .map(|b| {
                let mut b = b.clone();
                if id(&b) == gid {
                    b["date"] = json!(g.date);
                    b["start_min"] = json!(g.start);
                    b["end_min"] = json!(g.end);
                }
                b
            })
            .collect(),
    }
}

fn slice_lane(b: Box, lane: f64, lanes: f64) -> Box {
    Box {
        pct_l: b.pct_l + (b.pct_w * lane) / lanes,
        px_l: b.px_l + (b.px_w * lane) / lanes,
        pct_w: b.pct_w / lanes,
        px_w: b.px_w / lanes,
    }
}

fn inset_box(b: Box) -> Box {
    Box {
        px_l: b.px_l + CASCADE_INSET,
        px_w: b.px_w - CASCADE_INSET,
        ..b
    }
}

fn box_px(b: Box, grid_w: f64) -> f64 {
    (grid_w * b.pct_w) / 100. + b.px_w
}

/// JS 의 Math.round — 절반은 +∞ 쪽으로(Rust 의 round 는 0 에서 먼 쪽이라 음수 절반에서 갈린다)
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1. } else { f }
}

/// 소수 넷째 자리로 — JS 의 숫자→글자와 같게(-0 은 0)
fn num(n: f64) -> String {
    let r = js_round(n * 1e4) / 1e4 + 0.;
    format!("{r}")
}

fn signed(px: f64) -> String {
    if px < 0. {
        format!("- {}px", num(-px))
    } else {
        format!("+ {}px", num(px))
    }
}

pub fn box_left(b: Box) -> String {
    format!("calc({}% {})", num(b.pct_l), signed(b.px_l))
}

pub fn box_width(b: Box, gap: f64) -> String {
    format!("calc({}% {})", num(b.pct_w), signed(b.px_w - gap))
}

/// a 가 b 를 진짜로(같은 범위는 빼고) 감싸는가 — 같은 범위끼리는 교차로 보고 나란히 놓는다
fn swallows(a: &Json, b: &Json) -> bool {
    start(a) <= start(b) && end(b) <= end(a) && (start(a) < start(b) || end(b) < end(a))
}

struct Lane {
    b: Json,
    lane: usize,
    lanes: usize,
    over: usize,
}

/// 서로 포함하지 않는 블록들의 칸 배치. 칸이 모자라면 새 칸 대신 가장 먼저 비는 칸 위에 얹는다(숨기지 않는다)
fn lane_split(blocks: &[Json], max_lanes: usize) -> Vec<Lane> {
    let mut sorted = blocks.to_vec();
    sorted.sort_by(|a, b| {
        (start(a) - start(b))
            .partial_cmp(&0.)
            .unwrap()
            .then_with(|| (end(b) - end(a)).partial_cmp(&0.).unwrap())
            .then_with(|| id(a).cmp(&id(b)))
    });
    let mut out = Vec::new();
    let mut cluster: Vec<Lane> = Vec::new();
    let mut lane_ends: Vec<f64> = Vec::new();
    let mut over_ends: Vec<f64> = Vec::new();
    let mut cluster_end = -1.;
    let flush = |cluster: &mut Vec<Lane>,
                 lane_ends: &mut Vec<f64>,
                 over_ends: &mut Vec<f64>,
                 out: &mut Vec<Lane>| {
        let n = lane_ends.len();
        for mut item in cluster.drain(..) {
            item.lanes = n;
            out.push(item);
        }
        lane_ends.clear();
        over_ends.clear();
    };
    for b in sorted {
        if !cluster.is_empty() && start(&b) >= cluster_end {
            flush(&mut cluster, &mut lane_ends, &mut over_ends, &mut out);
            cluster_end = -1.;
        }
        let mut over = 0;
        let lane = match lane_ends.iter().position(|e| *e <= start(&b)) {
            Some(l) => l,
            None if lane_ends.len() < max_lanes => {
                lane_ends.push(0.);
                lane_ends.len() - 1
            }
            None => {
                let step = match over_ends.iter().position(|e| *e <= start(&b)) {
                    Some(s) => s,
                    None => {
                        over_ends.push(0.);
                        over_ends.len() - 1
                    }
                };
                over_ends[step] = end(&b);
                over = step + 1;
                // 가장 먼저 비는 칸(같으면 앞 칸)
                (0..lane_ends.len()).fold(0, |m, i| if lane_ends[i] < lane_ends[m] { i } else { m })
            }
        };
        lane_ends[lane] = lane_ends[lane].max(end(&b));
        cluster_end = f64::max(cluster_end, end(&b));
        cluster.push(Lane {
            b,
            lane,
            lanes: 1,
            over,
        });
    }
    flush(&mut cluster, &mut lane_ends, &mut over_ends, &mut out);
    out
}

#[derive(Serialize, Debug)]
pub struct Placed {
    pub b: Json,
    #[serde(rename = "box")]
    pub bx: Box,
    pub depth: i64,
}

/// 한 날짜 칸의 블록 배치. root = 거터를 뺀 칸 안 상자, grid_w = 그리드 실폭(칸 하한 판정에만)
pub fn layout_column(blocks: &[Json], root: Box, grid_w: f64) -> Vec<Placed> {
    let dur = |b: &Json| end(b) - start(b);
    // 부모 = 자기를 감싸는 것 중 가장 작은 것(같으면 id 가 작은 것). 최상위는 None
    let mut kids: Vec<(Option<i64>, Vec<Json>)> = Vec::new();
    for b in blocks {
        let mut best: Option<&Json> = None;
        for a in blocks {
            if id(a) == id(b) || !swallows(a, b) {
                continue;
            }
            if best.is_none_or(|x| dur(a) < dur(x) || (dur(a) == dur(x) && id(a) < id(x))) {
                best = Some(a);
            }
        }
        let key = best.map(id);
        match kids.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(b.clone()),
            None => kids.push((key, vec![b.clone()])),
        }
    }
    fn place(
        group: &[Json],
        bx: Box,
        depth: i64,
        grid_w: f64,
        kids: &[(Option<i64>, Vec<Json>)],
        out: &mut Vec<Placed>,
    ) {
        let lanes = (box_px(bx, grid_w) / MIN_LANE).floor().max(1.) as usize;
        for item in lane_split(group, lanes) {
            let mut mine = slice_lane(bx, item.lane as f64, item.lanes as f64);
            let mut d = depth;
            for _ in 0..item.over {
                d += 1;
                if d <= MAX_DEPTH {
                    mine = inset_box(mine);
                }
            }
            let bid = id(&item.b);
            out.push(Placed {
                b: item.b,
                bx: mine,
                depth: d,
            });
            if let Some((_, children)) = kids.iter().find(|(k, _)| *k == Some(bid)) {
                place(
                    children,
                    if d < MAX_DEPTH { inset_box(mine) } else { mine },
                    d + 1,
                    grid_w,
                    kids,
                    out,
                );
            }
        }
    }
    let mut out = Vec::new();
    let top = kids
        .iter()
        .find(|(k, _)| k.is_none())
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    place(&top, root, 0, grid_w, &kids, &mut out);
    out
}

/// from_min 이후로 dur 분이 들어가는 첫 빈 자리. 끝까지 없으면 from_min(겹침 허용 — 칸 나누기가 맡는다)
pub fn find_free_slot(blocks: &[Json], from_min: f64, dur: f64) -> f64 {
    let latest = 1440. - dur;
    let mut cur = from_min.min(latest);
    let mut sorted = blocks.to_vec();
    sorted.sort_by(|a, b| start(a).partial_cmp(&start(b)).unwrap());
    for b in &sorted {
        if end(b) <= cur {
            continue;
        }
        if start(b) >= cur + dur {
            return cur;
        }
        cur = end(b).min(latest);
    }
    cur
}

/// 앱 시계(ms)의 로컬 자정 기준 분
pub fn now_minute(now_ms: i64) -> i64 {
    use chrono::{Local, TimeZone, Timelike};
    Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map_or(0, |d| (d.hour() * 60 + d.minute()) as i64)
}

/// 하네스가 부르는 이름 → 결과
pub fn call(op: &str, args: &[Json], now_ms: i64) -> Option<Result<Json, String>> {
    let blocks = || {
        args.first()
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let de_box = |i: usize| -> Option<Box> { serde_json::from_value(args.get(i)?.clone()).ok() };
    let f = |i: usize| args.get(i).and_then(Json::as_f64);
    let ok = |v: Json| Some(Ok(v));
    match op {
        "ttLayout.withGhost" => {
            let ghost: Option<GhostAt> = args
                .get(1)
                .and_then(|g| serde_json::from_value(g.clone()).ok());
            ok(json!(with_ghost(&blocks(), ghost.as_ref())))
        }
        "ttLayout.boxLeft" => ok(json!(box_left(de_box(0)?))),
        "ttLayout.boxWidth" => ok(json!(box_width(de_box(0)?, f(1).unwrap_or(BLOCK_GAP)))),
        "ttLayout.layoutColumn" => ok(json!(layout_column(&blocks(), de_box(1)?, f(2)?))),
        "timeBlocks.nowMinute" => ok(json!(now_minute(now_ms))),
        "timeBlocks.findFreeSlot" => ok(json!(find_free_slot(&blocks(), f(1)?, f(2)?))),
        _ => None,
    }
}
