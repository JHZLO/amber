pub mod blocks;
pub mod concepts;
pub mod db;
pub mod mermaid;
pub mod report;
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
    "db.listConcepts",
    "db.learningQueue",
    "db.getConcept",
    "db.conceptsLearnedOn",
    "db.repointConceptSource",
    "db.createConcept",
    "db.updateConceptContent",
    "db.setStatus",
    "db.adjustConfidence",
    "db.markSeen",
    "db.deleteConcept",
    "db.setConceptTags",
    "db.allTags",
    "db.statusCounts",
    "db.getSetting",
    "db.setSetting",
    "db.logInvocation",
    "report.loadMcpCache",
    "report.saveMcpCache",
    "report.loadReportConfig",
    "report.saveReportConfig",
    "report.getReport",
    "report.upsertReport",
    "report.recentReportDates",
    "report.buildTodosDigest",
    "report.getWeeklyReport",
    "report.upsertWeeklyReport",
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
        "db.listConcepts" => rows(concepts::list_concepts(db, &a.de(0)?)),
        "db.learningQueue" => rows(concepts::learning_queue(db)),
        "db.getConcept" => concepts::get_concept(db, a.i(0)?)
            .map(|r| r.map_or(Json::Null, Json::Object))
            .map_err(err),
        "db.conceptsLearnedOn" => rows(concepts::concepts_learned_on(db, a.f(0)?, a.f(1)?)),
        "db.repointConceptSource" => unit(concepts::repoint_concept_source(
            db,
            a.s(0)?,
            a.s(1)?,
            a.0.get(2).and_then(Json::as_bool).unwrap_or(false),
        )),
        "db.createConcept" => concepts::create_concept(db, &a.de(0)?)
            .map(Json::from)
            .map_err(err),
        "db.updateConceptContent" => {
            let field = |k| a.0.get(1).and_then(|f| f.get(k)).and_then(Json::as_str);
            unit(concepts::update_concept_content(
                db,
                a.i(0)?,
                field("title"),
                field("summary"),
            ))
        }
        "db.setStatus" => unit(concepts::set_status(db, a.i(0)?, a.s(1)?)),
        "db.adjustConfidence" => concepts::adjust_confidence(db, a.i(0)?, a.f(1)?).map_err(err),
        "db.markSeen" => unit(concepts::mark_seen(db, a.i(0)?)),
        "db.deleteConcept" => unit(concepts::delete_concept(db, a.i(0)?)),
        "db.setConceptTags" => unit(concepts::set_concept_tags(
            db,
            a.i(0)?,
            &a.de::<Vec<String>>(1)?,
        )),
        "db.allTags" => rows(concepts::all_tags(db)),
        "db.statusCounts" => concepts::status_counts(db).map_err(err),
        "db.getSetting" => concepts::get_setting(db, a.s(0)?)
            .map(Json::from)
            .map_err(err),
        "db.setSetting" => unit(concepts::set_setting(db, a.s(0)?, a.s(1)?)),
        "db.logInvocation" => unit(concepts::log_invocation(db, &a.de(0)?)),
        "report.loadMcpCache" => report::load_mcp_cache(db).map_err(err),
        "report.saveMcpCache" => unit(report::save_mcp_cache(db, &a.de::<Vec<Json>>(0)?)),
        "report.loadReportConfig" => report::load_report_config(db).map_err(err),
        "report.saveReportConfig" => unit(report::save_report_config(db, &a.de(0)?)),
        "report.getReport" => report::get_report(db, a.s(0)?)
            .map(|r| r.map_or(Json::Null, Json::Object))
            .map_err(err),
        "report.upsertReport" => unit(report::upsert_report(db, &a.de(0)?)),
        "report.recentReportDates" => report::recent_report_dates(db, a.s(0)?, a.f(1)?)
            .map(Json::from)
            .map_err(err),
        "report.buildTodosDigest" => report::build_todos_digest(db, a.s(0)?).map_err(err),
        "report.getWeeklyReport" => report::get_weekly_report(db, a.s(0)?)
            .map(|r| r.map_or(Json::Null, Json::Object))
            .map_err(err),
        "report.upsertWeeklyReport" => unit(report::upsert_weekly_report(db, &a.de(0)?)),
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
    fn f(&self, i: usize) -> Result<f64, String> {
        self.0
            .get(i)
            .and_then(Json::as_f64)
            .ok_or(format!("인자 {i}: 숫자가 아니다"))
    }
    /// 객체 인자 — TS 의 입력 타입(필터, 생성 입력 등)을 그대로 받는다
    fn de<T: serde::de::DeserializeOwned>(&self, i: usize) -> Result<T, String> {
        serde_json::from_value(self.0.get(i).cloned().unwrap_or(Json::Null))
            .map_err(|e| format!("인자 {i}: {e}"))
    }
    fn ids(&self, i: usize) -> Result<Vec<i64>, String> {
        self.0
            .get(i)
            .and_then(Json::as_array)
            .map(|v| v.iter().filter_map(Json::as_i64).collect())
            .ok_or(format!("인자 {i}: 배열이 아니다"))
    }
}
