// 타임테이블 블록 조작 — 지금 앱 DayTimetable.tsx 의 onGridDown, onBlockDown, onResizeDown, 제목 고치기, 지우기.
// 빈 곳 세로 끌기 = 만들기(15분 단위, 누른 칸의 날짜), 블록 끌기 = 옮기기(주 뷰에선 가로로 다른 날까지),
// 아래 가장자리 = 늘이기(날짜 고정, 최소 15분). 끄는 동안엔 미리보기만 바꾸고 놓을 때 DB 에 한 번 쓴다.
// ⌥ 를 누르고 시작하면 블록 위에서도 만들기다(겹쳐 만들기). 블록을 누르기만 하면 제목을 고친다.

use std::cell::Cell;
use std::rc::Rc;

use amber_gpui::blocks;
use amber_gpui::dates;
use amber_gpui::timetable::GhostAt;
use amber_gpui::tree::Node;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::Spike;
use crate::tt_card::{HOUR_H, View};
use amber_gpui::i18n::t;

pub type GridBounds = Rc<Cell<Bounds<Pixels>>>;

const SNAP: i64 = 15;
const MIN_DUR: i64 = 15;
const DAY_MIN: i64 = 1440;

/// 들고 있는 것 — GPUI 의 끌기 값
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TtGrab {
    Create,
    Move(i64),
    Resize(i64),
}

pub(crate) struct TtDrag {
    pub grab: TtGrab,
    /// 칸(날짜) 번호와 범위(자정부터 분)
    pub day: usize,
    pub start: i64,
    pub end: i64,
    /// 만들기: 누른 칸의 시작. 옮기기: 블록 안에서 잡은 자리(분)와 길이
    anchor: i64,
    offset: f64,
    dur: i64,
}

/// 블록 끌기에는 따라다니는 복제가 없다 — 미리보기는 그리드 안에서 그린다
pub struct Nothing;

impl Render for Nothing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn snap_floor(m: f64) -> i64 {
    (m / SNAP as f64).floor() as i64 * SNAP
}

/// JS 의 Math.round 와 같게(절반은 위로)
fn snap_round(m: f64) -> i64 {
    (m / SNAP as f64 + 0.5).floor() as i64 * SNAP
}

impl Spike {
    /// 시간 그리드의 칸 — 일 = 고른 날, 주 = 일~토
    pub(crate) fn tt_days(&self) -> Vec<String> {
        match self.tt_view {
            View::Week => dates::week_days(&dates::week_start_of(&self.date)),
            _ => vec![self.date.clone()],
        }
    }

    /// 창 좌표 → (자정부터 분, 칸 번호)
    fn grid_pos(&self, p: Point<Pixels>) -> (f64, usize) {
        let g = self.tt_grid.get();
        let n = self.tt_days().len();
        let min = f32::from(p.y - g.origin.y) as f64 / HOUR_H as f64 * 60.;
        let w = f32::from(g.size.width).max(1.) as f64;
        let day = ((f32::from(p.x - g.origin.x) as f64 / w) * n as f64).floor();
        (min, day.clamp(0., n as f64 - 1.) as usize)
    }

    /// (날짜, 시작, 끝)
    fn block_at(&self, id: i64) -> Option<(String, i64, i64)> {
        let b = self.tt_blocks.iter().find(|b| b.id() == id)?;
        let num = |k: &str| b.field(k).and_then(|v| v.as_i64()).unwrap_or(0);
        Some((
            b.field("date")?.as_str()?.to_string(),
            num("start_min"),
            num("end_min"),
        ))
    }

    /// 끌기가 시작될 때 한 번 — 누른 자리(그리드의 mouse down)에서 미리보기를 연다
    pub(crate) fn begin_tt(&mut self, grab: TtGrab) {
        let Some((press, alt)) = self.tt_press else {
            return;
        };
        let grab = if alt { TtGrab::Create } else { grab };
        let days = self.tt_days();
        let (min, day) = self.grid_pos(press);
        let col_of = |date: &str| days.iter().position(|d| d == date).unwrap_or(day);
        self.tt_drag = match grab {
            TtGrab::Create => {
                let anchor = snap_floor(min).clamp(0, DAY_MIN - SNAP);
                Some(TtDrag {
                    grab,
                    day,
                    start: anchor,
                    end: anchor + SNAP,
                    anchor,
                    offset: 0.,
                    dur: SNAP,
                })
            }
            TtGrab::Move(id) | TtGrab::Resize(id) => {
                self.block_at(id).map(|(date, start, end)| TtDrag {
                    grab,
                    day: col_of(&date),
                    start,
                    end,
                    anchor: start,
                    offset: min - start as f64,
                    dur: end - start,
                })
            }
        };
    }

    /// 커서를 따라 미리보기를 고친다. 바뀌었으면 true
    pub(crate) fn tt_move(&mut self, p: Point<Pixels>) -> bool {
        let (min, day) = self.grid_pos(p);
        let Some(d) = self.tt_drag.as_mut() else {
            return false;
        };
        let before = (d.day, d.start, d.end);
        match d.grab {
            TtGrab::Create => {
                let cur = snap_floor(min).clamp(0, DAY_MIN - SNAP);
                d.start = d.anchor.min(cur);
                d.end = d.anchor.max(cur) + SNAP;
            }
            TtGrab::Move(_) => {
                d.start = snap_round(min - d.offset).clamp(0, DAY_MIN - d.dur);
                d.end = d.start + d.dur;
                d.day = day;
            }
            TtGrab::Resize(_) => {
                d.end = snap_round(min).clamp(d.start + MIN_DUR, DAY_MIN);
            }
        }
        before != (d.day, d.start, d.end)
    }

    /// 배치에 넣을 미리보기 — 만드는 중이면 새 칸(GHOST_ID), 옮기는 중이면 그 블록의 새 자리
    pub(crate) fn tt_ghost(&self) -> Option<GhostAt> {
        let d = self.tt_drag.as_ref()?;
        Some(GhostAt {
            id: match d.grab {
                TtGrab::Create => None,
                TtGrab::Move(id) | TtGrab::Resize(id) => Some(id),
            },
            date: self.tt_days().get(d.day)?.clone(),
            start: d.start as f64,
            end: d.end as f64,
        })
    }

    /// 놓았다 — 제자리면 아무것도 쓰지 않는다. 새로 만든 블록은 바로 제목을 받는다
    pub(crate) fn tt_drop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tt_drag.take() else {
            return;
        };
        let days = self.tt_days();
        let Some(date) = days.get(d.day).cloned() else {
            return;
        };
        let db = &self.db;
        let r = match d.grab {
            TtGrab::Create => blocks::create_block(db, &date, d.start, d.end, "", None).map(Some),
            TtGrab::Move(id) => match self.block_at(id) {
                Some((bd, bs, _)) if bs == d.start && bd == date => Ok(None),
                Some(_) => blocks::update_block_time(db, id, &date, d.start, d.end).map(|_| None),
                None => Ok(None),
            },
            TtGrab::Resize(id) => match self.block_at(id) {
                Some((_, _, be)) if be == d.end => Ok(None),
                Some((bd, bs, _)) => {
                    blocks::update_block_time(db, id, &bd, bs, d.end).map(|_| None)
                }
                None => Ok(None),
            },
        };
        self.reload_blocks();
        match r {
            Ok(Some(id)) => self.start_tt_edit(id, "", window, cx),
            Ok(None) => {}
            Err(e) => eprintln!("timeBlocks: {e}"),
        }
    }

    /// 제목 고치기 — Enter 와 바깥 클릭은 저장, Esc 는 취소
    pub(crate) fn start_tt_edit(
        &mut self,
        id: i64,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(t("todos.tt.titlePlaceholder"));
            s.set_value(title.to_string(), window, cx);
            s
        });
        cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                this.save_tt_edit(cx);
                cx.notify();
            }
            _ => {}
        })
        .detach();
        state.update(cx, |s, cx| s.focus(window, cx));
        self.tt_editing = Some((id, state));
    }

    /// 바뀌지 않았으면 쓰지 않는다(빈 제목은 그대로 '제목 없음'이다)
    pub(crate) fn save_tt_edit(&mut self, cx: &mut Context<Self>) {
        let Some((id, state)) = self.tt_editing.take() else {
            return;
        };
        let title = state.read(cx).value().trim().to_string();
        let before = self
            .tt_blocks
            .iter()
            .find(|b| b.id() == id)
            .and_then(|b| b.field("title"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if title == before {
            return;
        }
        if let Err(e) = blocks::rename_block(&self.db, id, &title) {
            eprintln!("renameBlock: {e}");
        }
        self.reload_blocks();
    }
}
