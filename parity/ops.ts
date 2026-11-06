// 재는 대상: SQLite 를 만지는 도메인 함수와 vault 파일을 만지는 함수 전부. 이름은 "파일.함수" — Rust 엔진도
// 같은 이름으로 받는다. 파일은 앱 데이터 폴더(= DB 파일이 있는 폴더) 아래에서 다룬다(fs.ts).
//
// MODULES 파일의 export 는 셋 중 하나에 꼭 들어간다(parity.test.ts 가 막는다). 새 함수를 만들고
// 여기 안 넣으면 테스트가 멈춘다 — 재지 않는 함수가 조용히 생기지 않게.
//  - OPS: 시나리오로 잰다
//  - NOT_COVERED: 이 단에서 재지 않는다(키체인, 외부 DB, AI 실행 등). 이유를 남긴다
//  - PURE: DB 와 파일을 안 쓰는 계산 — 동기 함수여야 한다

import * as aiActivity from "../src/lib/aiActivity";
import * as aiInstruction from "../src/lib/aiInstruction";
import * as aiWait from "../src/lib/aiWait";
import * as comments from "../src/lib/comments";
import * as config from "../src/lib/config";
import * as date from "../src/lib/date";
import * as db from "../src/lib/db";
import * as dbconn from "../src/lib/dbconn";
import * as diagramGraph from "../src/lib/diagramGraph";
import * as diagrams from "../src/lib/diagrams";
import * as erdGen from "../src/lib/erdGen";
import * as erdStore from "../src/lib/erdStore";
import * as holidays from "../src/lib/holidays";
import * as mdSections from "../src/lib/mdSections";
import * as noteAiRun from "../src/lib/noteAiRun";
import * as noteAnchor from "../src/lib/noteAnchor";
import * as notes from "../src/lib/notes";
import * as report from "../src/lib/report";
import * as timeBlocks from "../src/lib/timeBlocks";
import * as todos from "../src/lib/todos";
import * as todoTree from "../src/lib/todoTree";
import * as treeFind from "../src/lib/treeFind";
import * as ttLayout from "../src/lib/ttLayout";
import * as vacations from "../src/lib/vacations";
import * as vault from "../src/lib/vault";
import * as vaultTree from "../src/lib/vaultTree";

type Op = (...args: never[]) => Promise<unknown>;

/** 인자의 모양은 픽스처 JSON 이 정한다 — 타입은 녹화한 TS 함수가 보증했다 */
export const callOp = (name: string, args: unknown[]) =>
  (OPS[name] as unknown as (...a: unknown[]) => Promise<unknown>)(...args);

export const MODULES = [
  "todos",
  "db",
  "report",
  "dbconn",
  "timeBlocks",
  "vacations",
  "notes",
  "diagrams",
  "vault",
  "erdStore",
  "vaultTree",
  "date",
  "holidays",
  "todoTree",
  "ttLayout",
  "treeFind",
  "comments",
  "config",
  "noteAnchor",
  "mdSections",
  "aiInstruction",
  "aiActivity",
  "aiWait",
  "noteAiRun",
  "diagramGraph",
  "erdGen",
] as const;

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

  "notes.listNoteTree": notes.listNoteTree,
  "notes.readNoteFile": notes.readNoteFile,
  "notes.writeNoteFile": notes.writeNoteFile,
  "notes.createFolder": notes.createFolder,
  "notes.createNote": notes.createNote,
  "notes.renameEntry": notes.renameEntry,
  "notes.moveEntry": notes.moveEntry,
  "notes.deleteEntry": notes.deleteEntry,

  "diagrams.listDiagramTree": diagrams.listDiagramTree,
  "diagrams.readDiagramFile": diagrams.readDiagramFile,
  "diagrams.writeDiagramFile": diagrams.writeDiagramFile,
  "diagrams.createFolder": diagrams.createFolder,
  "diagrams.createDiagram": diagrams.createDiagram,
  "diagrams.renameEntry": diagrams.renameEntry,
  "diagrams.moveEntry": diagrams.moveEntry,
  "diagrams.deleteEntry": diagrams.deleteEntry,

  "vault.writeNote": vault.writeNote,
  "vault.readNote": vault.readNote,
  "vault.noteExists": vault.noteExists,
  "vault.deleteConceptDir": vault.deleteConceptDir,

  "erdStore.readErdFileIfExists": erdStore.readErdFileIfExists,
  "erdStore.writeErdFile": erdStore.writeErdFile,
  "erdStore.ensureErdDir": erdStore.ensureErdDir,
  "erdStore.erdFileExists": erdStore.erdFileExists,
  "erdStore.deleteErdEntry": erdStore.deleteErdEntry,

  "vaultTree.writeAtomic": vaultTree.writeAtomic,
  // 트리 화면이 쓰는 순수 계산 — 이름 검증, 만들 자리 목록, 이름이 바뀐 폴더 아래 경로 다시 잇기
  "vaultTree.parentOf": vaultTree.parentOf,
  "vaultTree.invalidNameReason": vaultTree.invalidNameReason,
  "vaultTree.invalidPathReason": vaultTree.invalidPathReason,
  "vaultTree.normalizePath": vaultTree.normalizePath,
  "vaultTree.flattenDirs": vaultTree.flattenDirs,
  "vaultTree.ancestorPaths": vaultTree.ancestorPaths,
  "vaultTree.remapPath": vaultTree.remapPath,
  // Set 은 배열로 오간다
  "vaultTree.remapPaths": (paths: string[], oldPrefix: string, newPrefix: string) => [
    ...vaultTree.remapPaths(paths, oldPrefix, newPrefix),
  ],

  // AI 연결 설정 — 옛 키(claude_path) 이전, 걸러지는 제공자(gemini), 제공자별 경로와 모델
  "config.loadConfig": config.loadConfig,
  "config.connectProvider": config.connectProvider,
  "config.skipAiOnboarding": config.skipAiOnboarding,
  "config.saveConfig": config.saveConfig,

  // 노트 인라인 질문의 사이드카 — 못 읽은 파일을 빈 목록으로 뭉개지 않는다(덮으면 문답이 사라진다)
  "comments.commentsPathFor": comments.commentsPathFor,
  "comments.readComments": comments.readComments,
  "comments.loadComments": comments.loadComments,
  // 고치는 함수는 JSON 으로 못 넘긴다 — 새 목록으로 바꾸는 편집 하나로 잰다(읽고 검사한 뒤 쓰는 순서가 요점)
  "comments.mutateComments": (noteRel: string, next: comments.NoteComment[]) =>
    comments.mutateComments(noteRel, () => next),
  "comments.saveComments": comments.saveComments,
  "noteAnchor.nthIndex": noteAnchor.nthIndex,

  // 노트 AI 의 순수 계산 — 소스 좌표(절 나누기, 되끼우기), 지시 합치기, 진행 한 줄
  "mdSections.splitSections": mdSections.splitSections,
  "mdSections.mergeRuns": mdSections.mergeRuns,
  "mdSections.spliceSpan": mdSections.spliceSpan,
  "noteAiRun.spliceSpans": noteAiRun.spliceSpans,
  "aiInstruction.composeInstruction": aiInstruction.composeInstruction,
  "aiInstruction.tailSpan": aiInstruction.tailSpan,
  "aiActivity.shortTarget": aiActivity.shortTarget,
  "aiActivity.describeActivity": aiActivity.describeActivity,
  "aiWait.waitLine": aiWait.waitLine,

  // 다이어그램 포커스 모드와 ER 컬럼 판 — 렌더된 SVG 의 data-id, 컬럼 라벨을 읽는 규칙
  // ERD 탭 — 스냅샷 → resin 원문(사내 추론 + resin fromSchema)
  "erdGen.generateErd": erdGen.generateErd,
  "erdGen.inferReferences": erdGen.inferReferences,
  "erdGen.commonTablePrefix": erdGen.commonTablePrefix,

  "diagramGraph.splitOptionalType": diagramGraph.splitOptionalType,
  "diagramGraph.nullFlagOf": diagramGraph.nullFlagOf,
  "diagramGraph.nullabilityFlag": diagramGraph.nullabilityFlag,
  "diagramGraph.parseEdgeEndpoints": diagramGraph.parseEdgeEndpoints,
  "diagramGraph.neighborsOf": (id: string, edges: { source: string; target: string }[]) => [
    ...diagramGraph.neighborsOf(id, edges),
  ],

  // 트리에서 찾기(⌘F) — 접힌 폴더 안까지 거르고 조상을 펼친다
  "treeFind.hits": treeFind.hits,
  "treeFind.splitHit": treeFind.splitHit,
  "treeFind.filterTree": (nodes: treeFind.FindableNode[], query: string) => {
    const r = treeFind.filterTree(nodes, query);
    return { nodes: r.nodes, matches: r.matches, expand: [...r.expand] };
  },
  // 검색 화면(SearchModal)이 부르는 그대로 — 트리와 읽기를 노트 또는 다이어그램 쪽으로 묶는다
  "vaultTree.searchFiles": (section: "notes" | "diagrams", query: string, limit?: number) =>
    vaultTree.searchFiles(
      section === "diagrams"
        ? { listTree: diagrams.listDiagramTree, readFile: diagrams.readDiagramFile }
        : { listTree: notes.listNoteTree, readFile: notes.readNoteFile },
      query,
      limit,
    ),

  // 날짜와 공휴일 — 순수 계산이지만 달력이 이걸로 그려진다. 하네스에서는 언어가 ko 다(i18n.getLang)
  "date.todayStr": date.todayStr,
  "date.shiftDay": date.shiftDay,
  "date.addMonths": date.addMonths,
  "date.weekStartOf": date.weekStartOf,
  "date.weekDays": date.weekDays,
  "date.monthOf": date.monthOf,
  "date.dayRangeMs": date.dayRangeMs,
  "date.monthGridDates": date.monthGridDates,
  "date.formatDayLong": date.formatDayLong,
  "date.weekdayShort": date.weekdayShort,
  "date.formatDayShort": date.formatDayShort,
  "date.formatMonthTitle": date.formatMonthTitle,
  "date.formatYearTitle": date.formatYearTitle,
  "date.monthsShort": date.monthsShort,
  "date.weekdaysShort": date.weekdaysShort,
  // Map 은 JSON 으로 안 넘어간다 — 날짜 순 객체로
  "holidays.holidaysOfYear": (year: number) => Object.fromEntries(holidays.holidaysOfYear(year)),
  "holidays.holidayOf": holidays.holidayOf,
  "holidays.hasLunarData": holidays.hasLunarData,

  // 할 일 트리 계산 — 목록, 진행률, 끌어 놓기가 이걸로 정해진다. Set 은 JSON 으로 안 넘어가 배열로 주고받는다
  "todoTree.childrenOf": todoTree.childrenOf,
  "todoTree.ancestorPath": todoTree.ancestorPath,
  "todoTree.flattenTree": todoTree.flattenTree,
  "todoTree.visibleRoots": todoTree.visibleRoots,
  "todoTree.flattenSubset": todoTree.flattenSubset,
  "todoTree.subtreeIds": (nodes: todoTree.TodoNode[], rootId: number) => [...todoTree.subtreeIds(nodes, rootId)],
  "todoTree.descendantCount": todoTree.descendantCount,
  "todoTree.clampDropDepth": todoTree.clampDropDepth,
  "todoTree.resolveDrop": (
    nodes: todoTree.TodoNode[],
    candidates: todoTree.TreeRow[],
    draggedId: number,
    slot: todoTree.DropSlot,
    invalidParents?: number[],
  ) => todoTree.resolveDrop(nodes, candidates, draggedId, slot, invalidParents ? new Set(invalidParents) : undefined),
  "todoTree.dayProgress": todoTree.dayProgress,

  // 타임테이블 — 겹친 블록의 칸 나누기와 들여쓰기, CSS 위치 문자열, 빈 자리 찾기
  "ttLayout.withGhost": ttLayout.withGhost,
  "ttLayout.boxLeft": ttLayout.boxLeft,
  "ttLayout.boxWidth": ttLayout.boxWidth,
  "ttLayout.layoutColumn": ttLayout.layoutColumn,
  "timeBlocks.nowMinute": timeBlocks.nowMinute,
  "timeBlocks.findFreeSlot": timeBlocks.findFreeSlot,
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
  "notes.noteMtime": "파일 수정 시각은 녹화할 때마다 다르다",
  "comments.newCommentId": "crypto.randomUUID — 매번 다르다",
  "aiWait.useAiWaitLine": "React 훅(1초마다 다시 그린다) — 계산은 aiWait.waitLine 으로 잰다",
  "noteAiRun.getNoteAiRun": "실행 스토어(메모리) — GPUI 는 note_ai.rs 가 같은 상태 기계를 갖고 ui_tests 가 가짜 CLI 로 잰다",
  "noteAiRun.isNoteAiRunning": "실행 스토어(메모리) — note_ai.rs, ui_tests",
  "noteAiRun.getNoteAiPhases": "실행 스토어(메모리) — note_ai.rs, ui_tests",
  "noteAiRun.useNoteAiRun": "React 훅 — 실행 스토어 구독",
  "noteAiRun.useNoteAiPhases": "React 훅 — 실행 스토어 구독",
  "noteAiRun.useAnyNoteAiRunning": "React 훅 — 실행 스토어 구독",
  "noteAiRun.startNoteAi": "AI CLI 실행 — note_ai.rs, ui_tests(가짜 CLI)",
  "noteAiRun.startNoteSpanAi": "AI CLI 실행 — note_ai.rs, ui_tests(가짜 CLI)",
  "noteAiRun.continueNoteAi": "AI CLI 실행 — note_ai.rs, ui_tests(가짜 CLI)",
  "noteAiRun.stopNoteAi": "AI CLI 취소 — note_ai.rs, ui_tests",
  "noteAiRun.dismissNoteAi": "실행 스토어(메모리) — note_ai.rs, ui_tests",
  "noteAiRun.dismissNoteAiError": "실행 스토어(메모리) — note_ai.rs, ui_tests",
  "noteAiRun.setNoteSpanResult": "실행 스토어(메모리) — 되끼우기는 noteAiRun.spliceSpans 로 잰다",
  "noteAiRun.setNoteAiResult": "실행 스토어(메모리) — note_ai.rs, ui_tests",
  "noteAiRun.remapNoteAiPaths": "실행 스토어(메모리) — note_ai.rs 단위 시험",
  "noteAiRun.dropNoteAiUnder": "실행 스토어(메모리) — note_ai.rs 단위 시험",
  "noteAiRun.resetNoteAiRunsForTest": "시험 전용",
  "noteAnchor.rangeAt": "DOM Range 를 만든다 — GPUI 는 그려진 글자 범위(RangeHighlight)로 칠한다",
  "noteAnchor.mergeLineRects": "DOM 사각형 합치기 — GPUI 는 RangeHighlight 가 줄마다 칠한다",
  "diagrams.diagramMtime": "파일 수정 시각은 녹화할 때마다 다르다",
  "vaultTree.createVaultTree": "트리를 만드는 함수 — notes, diagrams, erdStore 의 op 로 잰다",
  "date.localDateStr": "Date 객체를 받는다 — 다른 date op 가 모두 거친다",
  "date.parseLocalDate": "Date 객체를 돌려준다 — 다른 date op 가 모두 거친다",
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
  "vacations.isVacationKind",
  "vacations.vacationLabel",
  "notes.noteDirPath",
  "vault.detailPathFor",
]);
