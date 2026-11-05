// 재는 대상: SQLite 를 만지는 도메인 함수 전부. 이름은 "파일.함수" — Rust 엔진도 같은 이름으로 받는다.
//
// 이 여섯 파일의 export 는 셋 중 하나에 꼭 들어간다(parity.test.ts 가 막는다). 새 DB 함수를 만들고
// 여기 안 넣으면 테스트가 멈춘다 — 재지 않는 함수가 조용히 생기지 않게.
//  - OPS: 시나리오로 잰다
//  - NOT_COVERED: DB 밖(파일, 키체인, Rust 명령)을 함께 건드려 이 단에서 재지 않는다. 이유를 남긴다
//  - PURE: DB 를 안 쓰는 계산 — 동기 함수여야 한다

import * as db from "../src/lib/db";
import * as dbconn from "../src/lib/dbconn";
import * as report from "../src/lib/report";
import * as timeBlocks from "../src/lib/timeBlocks";
import * as todos from "../src/lib/todos";
import * as vacations from "../src/lib/vacations";

type Op = (...args: never[]) => Promise<unknown>;

/** 인자의 모양은 픽스처 JSON 이 정한다 — 타입은 녹화한 TS 함수가 보증했다 */
export const callOp = (name: string, args: unknown[]) =>
  (OPS[name] as unknown as (...a: unknown[]) => Promise<unknown>)(...args);

export const MODULES = ["todos", "db", "report", "dbconn", "timeBlocks", "vacations"] as const;

export const OPS: Record<string, Op> = {
  "todos.listTodos": todos.listTodos,
  "todos.listRangeRows": todos.listRangeRows,
  "todos.listOverdueOpen": todos.listOverdueOpen,
  "todos.createTodo": todos.createTodo,
  "todos.listWeekTodos": todos.listWeekTodos,
  "todos.listWeekCounts": todos.listWeekCounts,
  "todos.listParked": todos.listParked,
  "todos.parkSubtree": todos.parkSubtree,
  "todos.listParkedAncestors": todos.listParkedAncestors,
  "todos.unparkSubtree": todos.unparkSubtree,
  "todos.reparentTodo": todos.reparentTodo,
  "todos.reorderTodos": todos.reorderTodos,
  "todos.setSubtreeDone": todos.setSubtreeDone,
  "todos.recomputeParentDone": todos.recomputeParentDone,
  "todos.recomputeChainFrom": todos.recomputeChainFrom,
  "todos.updateTodoContent": todos.updateTodoContent,
  "todos.moveTodos": todos.moveTodos,
  "todos.deleteTodo": todos.deleteTodo,
  "todos.removeCarry": todos.removeCarry,

  "db.listConcepts": db.listConcepts,
  "db.learningQueue": db.learningQueue,
  "db.getConcept": db.getConcept,
  "db.conceptsLearnedOn": db.conceptsLearnedOn,
  "db.repointConceptSource": db.repointConceptSource,
  "db.createConcept": db.createConcept,
  "db.updateConceptContent": db.updateConceptContent,
  "db.setStatus": db.setStatus,
  "db.adjustConfidence": db.adjustConfidence,
  "db.markSeen": db.markSeen,
  "db.deleteConcept": db.deleteConcept,
  "db.setConceptTags": db.setConceptTags,
  "db.allTags": db.allTags,
  "db.statusCounts": db.statusCounts,
  "db.getSetting": db.getSetting,
  "db.setSetting": db.setSetting,
  "db.logInvocation": db.logInvocation,

  "report.loadMcpCache": report.loadMcpCache,
  "report.saveMcpCache": report.saveMcpCache,
  "report.loadReportConfig": report.loadReportConfig,
  "report.saveReportConfig": report.saveReportConfig,
  "report.getReport": report.getReport,
  "report.upsertReport": report.upsertReport,
  "report.recentReportDates": report.recentReportDates,
  "report.buildTodosDigest": report.buildTodosDigest,
  "report.getWeeklyReport": report.getWeeklyReport,
  "report.upsertWeeklyReport": report.upsertWeeklyReport,

  "dbconn.listConnections": dbconn.listConnections,
  "dbconn.getConnection": dbconn.getConnection,
  "dbconn.createConnection": dbconn.createConnection,
  "dbconn.updateConnection": dbconn.updateConnection,
  "dbconn.markSync": dbconn.markSync,
  "dbconn.setSchemaStarred": dbconn.setSchemaStarred,

  "timeBlocks.listBlocks": timeBlocks.listBlocks,
  "timeBlocks.createBlock": timeBlocks.createBlock,
  "timeBlocks.updateBlockTime": timeBlocks.updateBlockTime,
  "timeBlocks.renameBlock": timeBlocks.renameBlock,
  "timeBlocks.deleteBlock": timeBlocks.deleteBlock,

  "vacations.listVacations": vacations.listVacations,
  "vacations.getVacation": vacations.getVacation,
  "vacations.setVacation": vacations.setVacation,
  "vacations.removeVacation": vacations.removeVacation,
};

const FILES_AND_RUST = "파일이나 Rust 명령을 함께 건드린다";

export const NOT_COVERED: Record<string, string> = {
  "db.getDb": "연결 자체 — 엔진이 맡는다",
  "report.detectReportTools": FILES_AND_RUST,
  "report.reportMcpServers": FILES_AND_RUST,
  "report.reportCheckGithub": FILES_AND_RUST,
  "report.reportGhAccounts": FILES_AND_RUST,
  "report.reportCollect": FILES_AND_RUST,
  "report.reportGenerate": "AI CLI 실행",
  "report.reportGenerateWeekly": "AI CLI 실행",
  "report.deleteReport": "DB 행 + 리포트 파일을 휴지통으로(Rust 명령)",
  "report.deleteWeeklyReport": "DB 행 + 리포트 파일을 휴지통으로(Rust 명령)",
  "report.writeReportFile": "vault 파일 쓰기",
  "report.readReportFile": "vault 파일 읽기",
  "report.writeWeeklyReportFile": "vault 파일 쓰기",
  "report.readWeeklyReportFile": "vault 파일 읽기",
  "report.loadWeekSources": "vault 파일 읽기",
  "dbconn.deleteConnection": "DB 행 + 키체인 + ERD 폴더",
  "dbconn.dbTest": "외부 DB 접속(Rust)",
  "dbconn.dbIntrospect": "외부 DB 접속(Rust)",
  "dbconn.dbSecretSet": "키체인(Rust)",
  "dbconn.dbSecretDelete": "키체인(Rust)",
  "dbconn.readSnapshot": "ERD 파일 읽기",
  "dbconn.writeSnapshot": "ERD 파일 쓰기",
  "dbconn.syncSchema": "외부 DB 접속 + ERD 파일 + markSync",
  "dbconn.notifyConnectionsChanged": "창 이벤트",
};

export const PURE = new Set([
  "report.parseMcpPicked",
  "report.rankedSources",
  "report.mcpSourcesFrom",
  "report.reportPathFor",
  "report.weeklyReportPathFor",
  "dbconn.prefAudit",
  "dbconn.profileOf",
  "dbconn.schemaFolder",
  "dbconn.snapshotPath",
  "dbconn.isStale",
  "dbconn.enabledSchemas",
  "dbconn.envLabel",
  "dbconn.connStatus",
  "dbconn.connStatusLabel",
  "dbconn.connStatusDot",
  "dbconn.folderNameFor",
  "timeBlocks.nowMinute",
  "timeBlocks.findFreeSlot",
  "vacations.isVacationKind",
  "vacations.vacationLabel",
]);
