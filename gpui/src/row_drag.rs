// 할 일 줄 끌어 옮기기 — 지금 앱 TodoView.tsx 의 startDrag. 세로 = 자리, 가로 = 깊이(24px 마다 한 단).
// 집은 줄은 하위째 목록에서 빠지고, 놓일 자리에 한 줄 높이만큼 틈이 벌어진다. 틈 가운데의 선이
// 고른 깊이만큼 들여 써져 어디로 갈지 보여 준다. 놓으면 tree::resolve_drop 이 새 부모와 형제 순서를 정한다.
// 틈은 0.35s 에 걸쳐 벌어진다(.todo-listing.reordering .todo-row 의 transform) — 커서를 따라 내내 다시 겨냥되는
// 추종 모션이라 줄마다 지금 자리에서 새 목표로 간다. 레이아웃은 그대로(밀린 줄은 그려진 자리만 내려간다).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use amber_gpui::todos;
use amber_gpui::tree::{self, DropSlot, Node, TreeRow};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::theme::Tokens;

/// 줄마다 잰 자리 — 틈을 뺀(틈이 없을 때의) 위치다. 그래야 틈이 슬롯 계산을 흔들지 않는다
pub type RowBounds = Rc<RefCell<HashMap<i64, Bounds<Pixels>>>>;
pub type ListBounds = Rc<Cell<Bounds<Pixels>>>;

/// 밀림과 삽입선의 시간 — 상한(0.3s)의 예외: 짧으면 목적지에 바로 닿아 홱홱 끊긴다
const GLIDE: Duration = Duration::from_millis(350);
/// 삽입선이 나타나는 페이드(.todo-drop-line.on — 0.15s ease-out)
const LINE_FADE: Duration = Duration::from_millis(150);

/// CSS cubic-bezier(x1, y1, x2, y2) 의 t(시간 비율) → 진행 비율
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let b = |a: f32, c: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * c + s * s * s
    };
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..24 {
        let m = (lo + hi) / 2.;
        if b(x1, x2, m) < t {
            lo = m;
        } else {
            hi = m;
        }
    }
    b(y1, y2, (lo + hi) / 2.)
}

/// 화면 안 이동 — 지금 자리에서 목표로 0.35s, --ease-in-out(cubic-bezier(0.77, 0, 0.175, 1))
#[derive(Clone, Copy)]
pub(crate) struct Glide {
    from: Pixels,
    to: Pixels,
    t0: Instant,
}

impl Glide {
    fn still(at: Pixels, now: Instant) -> Self {
        Glide {
            from: at,
            to: at,
            t0: now,
        }
    }

    pub fn at(&self, now: Instant) -> Pixels {
        let p =
            (now.saturating_duration_since(self.t0).as_secs_f32() / GLIDE.as_secs_f32()).min(1.);
        self.from + (self.to - self.from) * bezier(0.77, 0., 0.175, 1., p)
    }

    /// 새 목표 — 가는 중이면 지금 자리에서 다시 출발한다(움직임을 줄이면 바로 간다)
    fn aim(&mut self, to: Pixels, now: Instant, reduce: bool) {
        if to == self.to {
            return;
        }
        *self = if reduce {
            Glide::still(to, now)
        } else {
            Glide {
                from: self.at(now),
                to,
                t0: now,
            }
        };
    }

    fn moving(&self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.t0) < GLIDE
    }
}

/// 들고 있는 것 — GPUI 의 끌기 값
pub struct DragRow {
    pub id: i64,
}

pub(crate) struct RowDrag {
    pub id: i64,
    start: Point<Pixels>,
    src_depth: i64,
    others: Vec<TreeRow>,
    pub gap: Pixels,
    slot: Option<DropSlot>,
    /// 줄마다 밀림(그려진 자리만 — 슬롯 아래 줄은 틈만큼 내려간다)
    shifts: HashMap<i64, Glide>,
    /// 삽입선의 세로 자리(창 좌표)와 나타난 때. 깊이(들여쓰기)는 바로 바뀐다
    line: Option<(Glide, Instant)>,
    reduce: bool,
}

impl RowDrag {
    /// 그 줄이 지금 그려질 만큼 내려간 거리 — 잰 자리에서 이만큼 뺀다
    pub fn shift(&self, id: i64, now: Instant) -> Pixels {
        self.shifts.get(&id).map_or(px(0.), |g| g.at(now))
    }

    /// 아직 움직이는 중인가(다음 그림을 청한다)
    pub fn moving(&self, now: Instant) -> bool {
        self.shifts.values().any(|g| g.moving(now))
            || self.line.as_ref().is_some_and(|(g, at)| {
                g.moving(now) || now.saturating_duration_since(*at) < LINE_FADE
            })
    }
}

/// 커서를 따라오는 줄 복제(.todo-drag-overlay) — 하위가 있으면 +N 칩
#[derive(Default)]
pub struct RowGhost {
    label: SharedString,
    kids: usize,
    width: Pixels,
    /// 집은 자리를 지킨다: 끌기 뷰는 손잡이 기준이라 줄 기준으로 옮긴다
    shift: Point<Pixels>,
}

impl Render for RowGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let k = Tokens::get(cx);
        div()
            .ml(self.shift.x)
            .mt(self.shift.y)
            .w(self.width)
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(8.))
            .bg(k.surface)
            .border_1()
            .border_color(k.border_strong)
            .rounded(px(8.))
            .shadow(k.shadow())
            .text_size(px(14.))
            .text_color(k.text)
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label.clone()),
            )
            .when(self.kids > 0, |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(7.))
                        .py(px(1.))
                        .border_1()
                        .border_color(k.border_strong)
                        .rounded_full()
                        .text_size(px(11.))
                        .text_color(k.text_3)
                        .child(format!("+{}", self.kids)),
                )
            })
    }
}

/// 깊이 한 단의 들여쓰기 — day_card 의 INDENT 와 같다
const INDENT: f32 = 24.;

impl Spike {
    /// 끌기가 시작될 때(2px 넘게 움직였을 때) 한 번. 집은 줄을 빼고 남은 줄로 슬롯 후보를 만든다
    pub(crate) fn begin_row_drag(&mut self, id: i64, offset: Point<Pixels>) -> RowGhost {
        let cur = self.rows().to_vec();
        let node = cur.iter().find(|r| r.id() == id);
        let label: SharedString = node
            .and_then(|r| r.field("content"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
            .into();
        let rows = tree::flatten_subset(&cur);
        let src_depth = rows.iter().find(|r| r.id == id).map_or(0, |r| r.depth);
        let subtree: HashSet<i64> = tree::subtree_ids(&cur, id).into_iter().collect();
        let map = self.row_bounds.borrow();
        let src = map.get(&id).copied().unwrap_or_default();
        let others: Vec<TreeRow> = rows
            .into_iter()
            .filter(|r| !subtree.contains(&r.id) && map.contains_key(&r.id))
            .collect();
        drop(map);
        let start = self.grip_down.take().unwrap_or(src.origin);
        let grab = start - src.origin;
        self.row_drag = Some(RowDrag {
            id,
            start,
            src_depth,
            others,
            // 하위는 칩(+N)으로 접히므로 틈은 한 줄 높이
            gap: src.size.height,
            slot: None,
            shifts: HashMap::new(),
            line: None,
            reduce: false,
        });
        RowGhost {
            label,
            kids: subtree.len() - 1,
            width: src.size.width.min(px(420.)),
            shift: offset - grab,
        }
    }

    /// 커서가 움직일 때마다 슬롯을 다시 고른다. 바뀌었으면 true(다시 그린다) — 줄과 삽입선은 새 목표로 미끄러진다
    pub(crate) fn row_drag_move(&mut self, pos: Point<Pixels>, reduce: bool) -> bool {
        let map = self.row_bounds.borrow();
        let list = self.list_bounds.get();
        let Some(d) = self.row_drag.as_mut() else {
            return false;
        };
        d.reduce = reduce;
        // 세로: 가운데를 지난 줄 수
        let mut idx: usize = 0;
        for r in &d.others {
            match map.get(&r.id) {
                Some(b) if pos.y > b.origin.y + b.size.height / 2. => idx += 1,
                _ => break,
            }
        }
        let above = idx.checked_sub(1).and_then(|i| d.others.get(i));
        let below = d.others.get(idx);
        // 가로: 집은 자리에서 한 단(24px)씩
        let desired = d.src_depth + ((pos.x - d.start.x) / px(INDENT)).round() as i64;
        let depth = tree::clamp_drop_depth(desired, above, below);
        let changed = d.slot.is_none_or(|s| s.idx != idx || s.depth != depth);
        d.slot = Some(DropSlot { idx, depth });
        if changed {
            let now = Instant::now();
            for (j, r) in d.others.iter().enumerate() {
                let to = if j >= idx { d.gap } else { px(0.) };
                d.shifts
                    .entry(r.id)
                    .or_insert_with(|| Glide::still(px(0.), now))
                    .aim(to, now, reduce);
            }
            let y = line_y(&map, &list, d, idx);
            match d.line.as_mut() {
                Some((g, _)) => g.aim(y, now, reduce),
                None => d.line = Some((Glide::still(y, now), now)),
            }
        }
        changed
    }

    /// 놓았다 — 제자리이거나 규칙상 안 되는 자리면 아무 일도 없다
    pub(crate) fn drop_row(&mut self) {
        let Some(d) = self.row_drag.take() else {
            return;
        };
        let Some(slot) = d.slot else {
            return;
        };
        let cur = self.rows().to_vec();
        let ids = |key: &str| -> HashSet<i64> {
            cur.iter().filter(|r| r.is(key, 1)).map(Node::id).collect()
        };
        // 지워진 기록 밑으로는 못 간다. 넘긴 줄(고스트)은 순서 재부여에서 뺀다 — 그 sort_order 는 도착 날짜의 것이다
        let (gone, carried) = (ids("gone"), ids("carried"));
        let Some(r) = tree::resolve_drop(&cur, &d.others, d.id, slot, Some(&gone)) else {
            return;
        };
        let old = cur.iter().find(|x| x.id() == d.id).and_then(Node::parent);
        let live: Vec<i64> = r
            .ordered_sibling_ids
            .into_iter()
            .filter(|id| !carried.contains(id))
            .collect();
        let db = &self.db;
        let moved = r.new_parent_id != old;
        let res = (|| {
            if moved {
                todos::reparent_todo(db, d.id, r.new_parent_id)?;
            }
            todos::reorder_todos(db, &live)?;
            // 완료 다시 세기: 떠난 묶음(마지막 미완료가 빠졌을 수 있다) + 새 묶음
            todos::recompute_chain_from(db, old)?;
            if moved {
                todos::recompute_chain_from(db, r.new_parent_id)?;
            }
            Ok::<_, rusqlite::Error>(())
        })();
        if let Err(e) = res {
            eprintln!("drop: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    /// 삽입선 — 세로는 틈 가운데(줄과 같은 모션으로 미끄러진다), 왼쪽은 고른 깊이만큼 들여 쓴다(바로).
    /// 처음엔 0.15s 에 걸쳐 나타난다. 창 좌표로 그린다(뿌리 판의 절대 위치)
    pub(crate) fn drop_line(&self, k: &Tokens) -> Option<Div> {
        let d = self.row_drag.as_ref()?;
        let slot = d.slot?;
        let list = self.list_bounds.get();
        let (glide, born) = d.line?;
        let now = Instant::now();
        let y = glide.at(now);
        let fade = if d.reduce {
            1.
        } else {
            // ease-out(0.23, 1, 0.32, 1)
            let p = now.saturating_duration_since(born).as_secs_f32() / LINE_FADE.as_secs_f32();
            bezier(0.23, 1., 0.32, 1., p.min(1.))
        };
        // .todo-listing 의 안쪽 여백(옆 16)
        let pad_x = px(16.);
        let lx = pad_x + px(10. + slot.depth as f32 * INDENT);
        let w = (list.size.width - pad_x - lx - px(8.)).max(px(0.));
        Some(
            div()
                .absolute()
                .opacity(fade)
                .left(list.origin.x + lx)
                .top(y - px(1.))
                .w(w)
                .h(px(2.))
                .rounded(px(1.))
                .bg(k.text)
                .child(
                    div()
                        .absolute()
                        .left(px(-4.))
                        .top(px(-2.))
                        .size(px(6.))
                        .rounded_full()
                        .bg(k.text),
                ),
        )
    }
}

/// 삽입선의 목표 세로 자리 — 슬롯 아래 줄의 (틈 없는) 자리 + 틈의 반, 끝이면 마지막 줄 아래
fn line_y(
    map: &HashMap<i64, Bounds<Pixels>>,
    list: &Bounds<Pixels>,
    d: &RowDrag,
    idx: usize,
) -> Pixels {
    // .todo-listing 의 안쪽 여백(위 10)
    let pad_t = px(10.);
    let above = idx.checked_sub(1).and_then(|i| d.others.get(i));
    let below = d.others.get(idx);
    match (
        below.and_then(|b| map.get(&b.id)),
        above.and_then(|a| map.get(&a.id)),
    ) {
        (Some(b), _) => b.origin.y + d.gap / 2.,
        (None, Some(a)) => a.bottom() + px(4.),
        _ => list.origin.y + pad_t + px(2.),
    }
}

#[cfg(test)]
mod tests {
    use super::bezier;

    #[test]
    fn ease_in_out_starts_slow_and_ends_at_one() {
        let e = |t| bezier(0.77, 0., 0.175, 1., t);
        assert!(e(0.).abs() < 1e-3 && (e(1.) - 1.).abs() < 1e-3);
        assert!(e(0.2) < 0.1, "처음은 느리다: {}", e(0.2));
        assert!((e(0.5) - 0.5).abs() < 0.1);
    }
}
