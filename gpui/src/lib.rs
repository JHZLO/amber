pub mod ai_config;
pub mod blocks;
pub mod comments;
pub mod concepts;
pub mod dates;
pub mod db;
pub mod dbconn;
pub mod diagram_graph;
pub mod elk;
pub mod erd_gen;
pub mod i18n;
pub mod line_diff;
pub mod mermaid;
pub mod migrate;
pub mod note_ai_text;
pub mod prefs;
pub mod report;
pub mod resin;
pub mod timetable;
pub mod todos;
pub mod tree;
pub mod tree_find;
pub mod vacations;
pub mod vault;

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
    "dbconn.listConnections",
    "dbconn.getConnection",
    "dbconn.createConnection",
    "dbconn.updateConnection",
    "dbconn.markSync",
    "dbconn.setSchemaStarred",
    "notes.listNoteTree",
    "notes.readNoteFile",
    "notes.writeNoteFile",
    "notes.createFolder",
    "notes.createNote",
    "notes.renameEntry",
    "notes.moveEntry",
    "notes.deleteEntry",
    "diagrams.listDiagramTree",
    "diagrams.readDiagramFile",
    "diagrams.writeDiagramFile",
    "diagrams.createFolder",
    "diagrams.createDiagram",
    "diagrams.renameEntry",
    "diagrams.moveEntry",
    "diagrams.deleteEntry",
    "vault.writeNote",
    "vault.readNote",
    "vault.noteExists",
    "vault.deleteConceptDir",
    "erdStore.readErdFileIfExists",
    "erdStore.writeErdFile",
    "erdStore.ensureErdDir",
    "erdStore.erdFileExists",
    "erdStore.deleteErdEntry",
    "vaultTree.writeAtomic",
    "vaultTree.searchFiles",
    "vaultTree.parentOf",
    "vaultTree.invalidNameReason",
    "vaultTree.invalidPathReason",
    "vaultTree.normalizePath",
    "vaultTree.flattenDirs",
    "vaultTree.ancestorPaths",
    "vaultTree.remapPath",
    "vaultTree.remapPaths",
    "treeFind.hits",
    "treeFind.filterTree",
    "treeFind.splitHit",
    "comments.commentsPathFor",
    "comments.readComments",
    "comments.loadComments",
    "comments.mutateComments",
    "comments.saveComments",
    "noteAnchor.nthIndex",
    "config.loadConfig",
    "config.connectProvider",
    "config.skipAiOnboarding",
    "config.saveConfig",
    "date.todayStr",
    "date.shiftDay",
    "date.addMonths",
    "date.weekStartOf",
    "date.weekDays",
    "date.monthOf",
    "date.dayRangeMs",
    "date.monthGridDates",
    "date.formatDayLong",
    "date.weekdayShort",
    "date.formatDayShort",
    "date.formatMonthTitle",
    "date.formatYearTitle",
    "date.monthsShort",
    "date.weekdaysShort",
    "holidays.holidaysOfYear",
    "holidays.holidayOf",
    "holidays.hasLunarData",
    "todoTree.childrenOf",
    "todoTree.ancestorPath",
    "todoTree.flattenTree",
    "todoTree.visibleRoots",
    "todoTree.flattenSubset",
    "todoTree.subtreeIds",
    "todoTree.descendantCount",
    "todoTree.clampDropDepth",
    "todoTree.resolveDrop",
    "todoTree.dayProgress",
    "ttLayout.withGhost",
    "ttLayout.boxLeft",
    "ttLayout.boxWidth",
    "ttLayout.layoutColumn",
    "timeBlocks.nowMinute",
    "timeBlocks.findFreeSlot",
    "mdSections.splitSections",
    "mdSections.mergeRuns",
    "mdSections.spliceSpan",
    "noteAiRun.spliceSpans",
    "aiInstruction.composeInstruction",
    "aiInstruction.tailSpan",
    "aiActivity.shortTarget",
    "aiActivity.describeActivity",
    "aiWait.waitLine",
    "dbconn.prefAudit",
    "dbconn.schemaFolder",
    "dbconn.enabledSchemas",
    "dbconn.isStale",
    "dbconn.envLabel",
    "dbconn.connStatus",
    "dbconn.connStatusLabel",
    "dbconn.connStatusDot",
    "dbconn.folderNameFor",
    "dbconn.profileOf",
    "erdGen.generateErd",
    "erdGen.inferReferences",
    "erdGen.commonTablePrefix",
    "diagramGraph.splitOptionalType",
    "diagramGraph.nullFlagOf",
    "diagramGraph.nullabilityFlag",
    "diagramGraph.parseEdgeEndpoints",
    "diagramGraph.neighborsOf",
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
        "dbconn.listConnections" => dbconn::list_connections(db).map(Json::from).map_err(err),
        "dbconn.getConnection" => dbconn::get_connection(db, a.i(0)?)
            .map(|c| c.unwrap_or(Json::Null))
            .map_err(err),
        "dbconn.createConnection" => dbconn::create_connection(db, &a.de(0)?).map_err(err),
        "dbconn.updateConnection" => unit(dbconn::update_connection(db, a.i(0)?, &a.de(1)?)),
        "dbconn.markSync" => unit(dbconn::mark_sync(
            db,
            a.i(0)?,
            a.0.get(1).and_then(Json::as_str),
        )),
        "dbconn.setSchemaStarred" => unit(dbconn::set_schema_starred(
            db,
            &a.de(0)?,
            a.s(1)?,
            a.0.get(2).and_then(Json::as_bool).unwrap_or(false),
        )),
        _ => call_files(db, op, &a),
    }
}

/// vault 파일 op — DB 쪽과 결과 모양이 같다(경로는 문자열, 없음은 null)
fn call_files(db: &Db, op: &str, a: &Args) -> Result<Json, String> {
    if let Some(r) = comments::call(db, op, a.0) {
        return r;
    }
    if let Some(r) = ai_config::call(db, op, a.0) {
        return r;
    }
    use vault::{DIAGRAMS, ERD, Tree};
    fn json<T: serde::Serialize>(v: Result<T, String>) -> Result<Json, String> {
        v.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
    }
    let flag = |i: usize| a.0.get(i).and_then(Json::as_bool).unwrap_or(false);
    let notes = vault::notes(db);
    let diagrams = Tree::new(db, &DIAGRAMS);
    let erd = Tree::new(db, &ERD);
    match op {
        "notes.listNoteTree" => json(notes.list_tree()),
        "notes.readNoteFile" => json(notes.read_file(a.s(0)?)),
        "notes.writeNoteFile" => json(notes.write_file(a.s(0)?, a.s(1)?)),
        "notes.createFolder" => json(notes.create_folder(a.s(0)?, a.s(1)?)),
        "notes.createNote" => json(notes.create_file(a.s(0)?, a.s(1)?)),
        "notes.renameEntry" => json(vault::rename_note_entry(db, a.s(0)?, a.s(1)?, flag(2))),
        "notes.moveEntry" => json(vault::move_note_entry(db, a.s(0)?, a.s(1)?)),
        "notes.deleteEntry" => json(vault::delete_note_entry(db, a.s(0)?)),
        "diagrams.listDiagramTree" => json(diagrams.list_tree()),
        "diagrams.readDiagramFile" => json(diagrams.read_file(a.s(0)?)),
        "diagrams.writeDiagramFile" => json(diagrams.write_file(a.s(0)?, a.s(1)?)),
        "diagrams.createFolder" => json(diagrams.create_folder(a.s(0)?, a.s(1)?)),
        "diagrams.createDiagram" => json(diagrams.create_file(a.s(0)?, a.s(1)?)),
        "diagrams.renameEntry" => json(diagrams.rename_entry(a.s(0)?, a.s(1)?, flag(2))),
        "diagrams.moveEntry" => json(diagrams.move_entry(a.s(0)?, a.s(1)?)),
        "diagrams.deleteEntry" => json(diagrams.delete_entry(a.s(0)?)),
        "vault.writeNote" => json(vault::write_concept_note(db, a.s(0)?, a.s(1)?)),
        "vault.readNote" => json(vault::read_concept_note(db, a.s(0)?)),
        "vault.noteExists" => Ok(Json::from(vault::concept_note_exists(db, a.s(0)?))),
        "vault.deleteConceptDir" => json(vault::delete_concept_dir(db, a.s(0)?)),
        "erdStore.readErdFileIfExists" => json(erd.read_file_if_exists(a.s(0)?)),
        "erdStore.writeErdFile" => json(erd.write_file(a.s(0)?, a.s(1)?)),
        "erdStore.ensureErdDir" => json(erd.ensure_dir(a.s(0)?)),
        "erdStore.erdFileExists" => Ok(Json::from(erd.file_exists(a.s(0)?))),
        "erdStore.deleteErdEntry" => json(erd.delete_entry(a.s(0)?)),
        "vaultTree.writeAtomic" => json(vault::write_atomic(db, a.s(0)?, a.s(1)?)),
        "vaultTree.searchFiles" => {
            let tree = if a.s(0)? == "diagrams" {
                &diagrams
            } else {
                &notes
            };
            let limit = a.0.get(2).and_then(Json::as_u64).unwrap_or(20) as usize;
            json(tree.search(a.s(1)?, limit))
        }
        _ => vault::call_pure(op, a.0)
            .or_else(|| tree_find::call(op, a.0))
            .or_else(|| note_ai_text::call(op, a.0))
            .or_else(|| diagram_graph::call(op, a.0))
            .or_else(|| erd_gen::call(op, a.0))
            .or_else(|| dbconn::call_pure(op, a.0))
            .or_else(|| dates::call(op, a.0, db.now()))
            .or_else(|| tree::call(op, a.0))
            .or_else(|| timetable::call(op, a.0, db.now()))
            .unwrap_or_else(|| Err(format!("모르는 op: {op}"))),
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
