pub mod blocks;
pub mod db;
pub mod mermaid;
pub mod todos;
pub mod vacations;

use serde_json::{Value as Json, json};

use db::Db;

/// 동등성 하네스가 부르는 이름("파일.함수")과 인자 → 결과. 인자는 TS 함수의 기본값까지 따른다
pub const OPS: &[&str] = &[
    "todos.listTodos",
    "todos.listRangeRows",
    "todos.listOverdueOpen",
    "todos.createTodo",
    "todos.listWeekTodos",
    "todos.listWeekCounts",
    "todos.listParked",
    "todos.parkSubtree",
    "todos.listParkedAncestors",
    "todos.unparkSubtree",
    "todos.reparentTodo",
    "todos.reorderTodos",
    "todos.setSubtreeDone",
    "todos.recomputeParentDone",
    "todos.recomputeChainFrom",
    "todos.updateTodoContent",
    "todos.moveTodos",
    "todos.deleteTodo",
    "todos.removeCarry",
    "timeBlocks.listBlocks",
    "timeBlocks.createBlock",
    "timeBlocks.updateBlockTime",
    "timeBlocks.renameBlock",
    "timeBlocks.deleteBlock",
    "vacations.listVacations",
    "vacations.getVacation",
    "vacations.setVacation",
    "vacations.removeVacation",
];

pub fn call(db: &Db, op: &str, args: &[Json]) -> Result<Json, String> {
    let a = Args(args);
    let err = |e: rusqlite::Error| e.to_string();
    let rows = |r: db::Result<Vec<db::Row>>| {
        r.map(|v| Json::from(v.into_iter().map(Json::Object).collect::<Vec<_>>()))
            .map_err(err)
    };
    let unit = |r: db::Result<()>| r.map(|_| Json::Null).map_err(err);
    match op {
        "todos.listTodos" => rows(todos::list_todos(db, a.s(0)?)),
        "todos.listRangeRows" => rows(todos::list_range_rows(db, a.s(0)?, a.s(1)?)),
        "todos.listOverdueOpen" => rows(todos::list_overdue_open(db, a.s(0)?)),
        "todos.createTodo" => {
            todos::create_todo(db, a.s(0)?, a.s(1)?, a.oi(2), a.s(3).unwrap_or("day"))
                .map(Json::from)
                .map_err(err)
        }
        "todos.listWeekTodos" => rows(todos::list_week_todos(db, a.s(0)?)),
        "todos.listWeekCounts" => rows(todos::list_week_counts(db, a.s(0)?, a.s(1)?)),
        "todos.listParked" => rows(todos::list_parked(db)),
        "todos.parkSubtree" => unit(todos::park_subtree(db, a.i(0)?)),
        "todos.listParkedAncestors" => rows(todos::list_parked_ancestors(db)),
        "todos.unparkSubtree" => todos::unpark_subtree(db, a.i(0)?, a.s(1)?)
            .map(|p| json!(p))
            .map_err(err),
        "todos.reparentTodo" => unit(todos::reparent_todo(db, a.i(0)?, a.oi(1))),
        "todos.reorderTodos" => unit(todos::reorder_todos(db, &a.ids(0)?)),
        "todos.setSubtreeDone" => unit(todos::set_subtree_done(db, a.i(0)?, a.i(1)?)),
        "todos.recomputeParentDone" => unit(todos::recompute_parent_done(db, a.i(0)?)),
        "todos.recomputeChainFrom" => unit(todos::recompute_chain_from(db, a.oi(0))),
        "todos.updateTodoContent" => unit(todos::update_todo_content(db, a.i(0)?, a.s(1)?)),
        "todos.moveTodos" => unit(todos::move_todos(db, &a.ids(0)?, a.s(1)?)),
        "todos.deleteTodo" => unit(todos::delete_todo(db, a.i(0)?)),
        "todos.removeCarry" => unit(todos::remove_carry(db, a.i(0)?, a.s(1)?)),
        "timeBlocks.listBlocks" => rows(blocks::list_blocks(db, a.s(0)?, a.s(1)?)),
        "timeBlocks.createBlock" => {
            blocks::create_block(db, a.s(0)?, a.i(1)?, a.i(2)?, a.s(3).unwrap_or(""), a.oi(4))
                .map(Json::from)
                .map_err(err)
        }
        "timeBlocks.updateBlockTime" => unit(blocks::update_block_time(
            db,
            a.i(0)?,
            a.s(1)?,
            a.i(2)?,
            a.i(3)?,
        )),
        "timeBlocks.renameBlock" => unit(blocks::rename_block(db, a.i(0)?, a.s(1)?)),
        "timeBlocks.deleteBlock" => unit(blocks::delete_block(db, a.i(0)?)),
        "vacations.listVacations" => vacations::list_vacations(db, a.s(0)?, a.s(1)?)
            .map(Json::Object)
            .map_err(err),
        "vacations.getVacation" => vacations::get_vacation(db, a.s(0)?)
            .map(Json::from)
            .map_err(err),
        "vacations.setVacation" => unit(vacations::set_vacation(db, a.s(0)?, a.s(1)?)),
        "vacations.removeVacation" => unit(vacations::remove_vacation(db, a.s(0)?)),
        _ => Err(format!("모르는 op: {op}")),
    }
}

struct Args<'a>(&'a [Json]);

impl Args<'_> {
    fn s(&self, i: usize) -> Result<&str, String> {
        self.0
            .get(i)
            .and_then(Json::as_str)
            .ok_or(format!("인자 {i}: 문자열이 아니다"))
    }
    fn i(&self, i: usize) -> Result<i64, String> {
        self.0
            .get(i)
            .and_then(Json::as_i64)
            .ok_or(format!("인자 {i}: 정수가 아니다"))
    }
    /// 없거나 null 이면 None — TS 의 `parentId: number | null = null`
    fn oi(&self, i: usize) -> Option<i64> {
        self.0.get(i).and_then(Json::as_i64)
    }
    fn ids(&self, i: usize) -> Result<Vec<i64>, String> {
        self.0
            .get(i)
            .and_then(Json::as_array)
            .map(|v| v.iter().filter_map(Json::as_i64).collect())
            .ok_or(format!("인자 {i}: 배열이 아니다"))
    }
}
