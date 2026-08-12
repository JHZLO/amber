// diagrams 도메인 문자열 — 키는 "diagrams." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).

const ko = {
  // 트리·헤더
  "diagrams.title": "다이어그램",
  "diagrams.newFile": "새 다이어그램",
  "diagrams.newFolder": "새 폴더",
  "diagrams.rename": "이름 변경",
  "diagrams.refresh": "새로고침",
  "diagrams.tooltip.newFileAt": "새 다이어그램 · {dir}",
  "diagrams.tooltip.newFolderAt": "새 폴더 · {dir}",
  "diagrams.tooltip.refresh": "새로고침 · Finder 변경 반영",
  "diagrams.tree.newFileHere": "이 폴더에 새 다이어그램",
  "diagrams.tree.newFolderHere": "이 폴더에 새 폴더",

  // 빈 상태 (트리 / 상세)
  "diagrams.empty.tree1": "아직 다이어그램이 없어요.",
  "diagrams.empty.tree2": "mermaid 문법으로 ERD·플로우차트를 그려 보세요.",
  "diagrams.empty.create": "첫 다이어그램 만들기",
  "diagrams.empty.sub1": "mermaid 문법으로 ERD·플로우차트·시퀀스를 그리고",
  "diagrams.empty.sub2": "폴더로 분류해 관리하세요.",
  "diagrams.empty.tipEdit": "편집하면 실시간 렌더",
  "diagrams.empty.tipZoom": "클릭하면 확대 (팬·줌)",

  // 상세/편집
  "diagrams.edit": "편집",
  "diagrams.saving": "저장 중…",
  "diagrams.working": "처리 중…",
  "diagrams.deleting": "삭제 중…",
  "diagrams.readError": "다이어그램을 읽을 수 없어요 — {msg}",
  "diagrams.meta.modified": "수정 {ago}",

  // 새 다이어그램/폴더·이름 변경 모달
  "diagrams.modal.location": "위치",
  "diagrams.modal.fileName": "다이어그램 이름",
  "diagrams.modal.folderName": "폴더 이름",
  "diagrams.modal.filePh": "예: 주문 ERD  ·  주문/주문 ERD",
  "diagrams.modal.folderPh": "예: 서비스별",
  "diagrams.modal.pathHint": "/ 로 구분하면 중간 폴더가 자동으로 만들어져요.",
  "diagrams.modal.renameConfirm": "변경",
  "diagrams.modal.create": "만들기",

  // 삭제 확인
  "diagrams.delete.fileTitle": "다이어그램 삭제",
  "diagrams.delete.folderTitle": "폴더 삭제",
  "diagrams.delete.bodyFile": "다이어그램을 삭제할까요?",
  "diagrams.delete.bodyDir": "폴더와 안의 모든 다이어그램을 삭제할까요?",
  "diagrams.delete.trashNote": "휴지통으로 옮겨져요 — Finder 에서 되돌릴 수 있어요.",

  // 저장 안 된 변경 모달
  "diagrams.unsaved.title": "저장하지 않은 변경",
  "diagrams.unsaved.body": "지금 다이어그램에 저장하지 않은 변경이 있어요. 버리고 이동할까요?",
  "diagrams.unsaved.keep": "계속 편집",
  "diagrams.unsaved.discard": "버리고 이동",

  // 외부 변경 충돌 모달
  "diagrams.conflict.title": "파일이 밖에서 바뀌었어요",
  "diagrams.conflict.body1":
    "이 다이어그램을 연 뒤에 다른 프로그램(Finder·vim·git 등)이 파일을 고쳤어요",
  "diagrams.conflict.diskMtime": "디스크 수정 {ago}",
  "diagrams.conflict.body2": "덮어쓰면 그 변경이 사라져요.",
  "diagrams.conflict.reread": "내 편집 버리고 다시 읽기",
  "diagrams.conflict.overwrite": "그래도 덮어쓰기",

  // DDL → ERD 변환 (AI 모달)
  "diagrams.ai.title": "DDL → ERD 변환",
  "diagrams.ai.tooltip": "스키마 DDL 을 붙여넣어 ERD 로 변환",
  "diagrams.ai.tooltipNoProvider": "AI 를 연결하면 쓸 수 있어요 (설정)",
  "diagrams.ai.configLoading": "설정을 불러오는 중이에요",
  "diagrams.ai.convert": "ERD로 변환",
  "diagrams.ai.back": "다시 변환",
  "diagrams.ai.apply": "에디터에 적용",
  "diagrams.ai.applyHint":
    "을 누르면 결과가 초안으로 들어가고, 저장(⌘S) 전까지 파일은 그대로예요.",
  "diagrams.ai.ddlLabel": "스키마 DDL",
  "diagrams.ai.ddlPh":
    "CREATE TABLE ts_order (\n  id BIGINT NOT NULL AUTO_INCREMENT COMMENT '주문 ID',\n  ...\n);",
  "diagrams.ai.ddlHint1":
    "CREATE TABLE · ALTER TABLE 를 그대로 붙여넣으세요. 컬럼 COMMENT · 인덱스 · UNIQUE · FK 제약을 읽어 ERD 표기 규칙(실선=물리 FK, 점선=논리 참조, ",
  "diagrams.ai.ddlHint2": " 태그, ",
  "diagrams.ai.ddlHint3": ", enum 나열)에 맞춰 변환해요.",
  "diagrams.ai.instrLabel": "추가 지시 (선택)",
  "diagrams.ai.instrPh": "예: 결제 관련 테이블만 · 컬럼 설명은 짧게 · 감사 테이블은 빼줘",
  "diagrams.ai.presetsLabel": "빠른 지시",
  "diagrams.ai.preset.noAudit": "감사(_aud) 테이블 제외",
  "diagrams.ai.preset.noIndex": "인덱스 표기 생략",
  "diagrams.ai.preset.inferFk": "논리 FK 도 추론해서 연결",
  "diagrams.ai.preset.coreOnly": "핵심 테이블만 추리기",
  "diagrams.ai.preset.append": "현재 다이어그램에 이어 붙이기",
  "diagrams.ai.generating": "스키마를 ERD로 옮기는 중…",
  "diagrams.ai.waiting": "응답을 기다리는 중…",
  "diagrams.ai.resultLabel": "변환 결과",
  "diagrams.ai.tabDiagram": "다이어그램",
  "diagrams.ai.tabSource": "소스",
  "diagrams.ai.tabDiff": "변경사항",

  // 렌더 캔버스 툴바/줌 (DiagramCanvas·MermaidZoom 공용)
  "diagrams.canvas.hint": "휠: 줌 · 드래그: 이동 · 더블클릭: 줌인",
  "diagrams.canvas.fullscreen": "전체화면",
  "diagrams.canvas.fullscreenClose": "전체화면 닫기 (Esc)",
  "diagrams.canvas.rendering": "렌더링 중…",
  "diagrams.zoom.in": "확대",
  "diagrams.zoom.out": "축소",
  "diagrams.zoom.fit": "맞춤",
  "diagrams.zoom.fitTitle": "화면에 맞춤",
  "diagrams.copy": "복사",

  // 노드 선택 정보 카드
  "diagrams.node.unnamed": "(이름 없음)",
  "diagrams.node.line": "라인 {n}",
  "diagrams.node.deselect": "선택 해제 (Esc)",
  "diagrams.node.copyName": "이름 복사",
  "diagrams.node.copied": "복사됨",

  // mermaid 인라인 렌더러
  "diagrams.mmd.failHead": "mermaid 문법 오류 — 렌더하지 못해 원본 코드를 표시해요",
  "diagrams.mmd.rendering": "다이어그램 렌더링 중…",
  "diagrams.mmd.clickToZoom": "클릭하면 확대해서 볼 수 있어요",
  "diagrams.mmd.staleBadge": "문법 오류 — 마지막 정상 상태 표시 중",
  "diagrams.mmd.expand": "확대",

  // diff 뷰 (AI 결과 비교)
  "diagrams.diff.none": "내용 변경이 없어요",
  "diagrams.diff.lines": "줄 변경",
  "diagrams.diff.fold": "⋯ 변경 없는 {n}줄",
} as const;

const en: Record<keyof typeof ko, string> = {
  "diagrams.title": "Diagrams",
  "diagrams.newFile": "New diagram",
  "diagrams.newFolder": "New folder",
  "diagrams.rename": "Rename",
  "diagrams.refresh": "Refresh",
  "diagrams.tooltip.newFileAt": "New diagram · {dir}",
  "diagrams.tooltip.newFolderAt": "New folder · {dir}",
  "diagrams.tooltip.refresh": "Refresh · pick up Finder changes",
  "diagrams.tree.newFileHere": "New diagram in this folder",
  "diagrams.tree.newFolderHere": "New folder in this folder",

  "diagrams.empty.tree1": "No diagrams yet.",
  "diagrams.empty.tree2": "Draw ERDs and flowcharts in mermaid syntax.",
  "diagrams.empty.create": "Create your first diagram",
  "diagrams.empty.sub1": "Draw ERDs, flowcharts, and sequences in mermaid,",
  "diagrams.empty.sub2": "and organize them into folders.",
  "diagrams.empty.tipEdit": "Live render while editing",
  "diagrams.empty.tipZoom": "Click to zoom (pan & zoom)",

  "diagrams.edit": "Edit",
  "diagrams.saving": "Saving…",
  "diagrams.working": "Working…",
  "diagrams.deleting": "Deleting…",
  "diagrams.readError": "Can't read this diagram — {msg}",
  "diagrams.meta.modified": "modified {ago}",

  "diagrams.modal.location": "Location",
  "diagrams.modal.fileName": "Diagram name",
  "diagrams.modal.folderName": "Folder name",
  "diagrams.modal.filePh": "e.g. Order ERD  ·  orders/Order ERD",
  "diagrams.modal.folderPh": "e.g. by-service",
  "diagrams.modal.pathHint": "Use / to create intermediate folders automatically.",
  "diagrams.modal.renameConfirm": "Rename",
  "diagrams.modal.create": "Create",

  "diagrams.delete.fileTitle": "Delete diagram",
  "diagrams.delete.folderTitle": "Delete folder",
  "diagrams.delete.bodyFile": "— delete this diagram?",
  "diagrams.delete.bodyDir": "— delete this folder and all diagrams inside?",
  "diagrams.delete.trashNote": "It moves to the Trash — you can restore it from Finder.",

  "diagrams.unsaved.title": "Unsaved changes",
  "diagrams.unsaved.body": "This diagram has unsaved changes. Discard them and switch?",
  "diagrams.unsaved.keep": "Keep editing",
  "diagrams.unsaved.discard": "Discard & switch",

  "diagrams.conflict.title": "File changed outside the app",
  "diagrams.conflict.body1":
    "Another program (Finder, vim, git, …) changed this file after you opened it",
  "diagrams.conflict.diskMtime": "disk modified {ago}",
  "diagrams.conflict.body2": "Overwriting will discard those changes.",
  "diagrams.conflict.reread": "Discard my edits & reload",
  "diagrams.conflict.overwrite": "Overwrite anyway",

  "diagrams.ai.title": "DDL → ERD",
  "diagrams.ai.tooltip": "Paste schema DDL to convert it into an ERD",
  "diagrams.ai.tooltipNoProvider": "Connect an AI provider to use this (Settings)",
  "diagrams.ai.configLoading": "Settings are still loading",
  "diagrams.ai.convert": "Convert to ERD",
  "diagrams.ai.back": "Convert again",
  "diagrams.ai.apply": "Apply to editor",
  "diagrams.ai.applyHint":
    " puts the result into the editor as a draft — the file stays as-is until you save (⌘S).",
  "diagrams.ai.ddlLabel": "Schema DDL",
  "diagrams.ai.ddlPh":
    "CREATE TABLE ts_order (\n  id BIGINT NOT NULL AUTO_INCREMENT COMMENT 'Order ID',\n  ...\n);",
  "diagrams.ai.ddlHint1":
    "Paste CREATE TABLE / ALTER TABLE statements as-is. Column COMMENTs, indexes, UNIQUE and FK constraints are read and converted with the ERD notation rules (solid = physical FK, dotted = logical reference, ",
  "diagrams.ai.ddlHint2": " tags, ",
  "diagrams.ai.ddlHint3": ", enum lists).",
  "diagrams.ai.instrLabel": "Extra instructions (optional)",
  "diagrams.ai.instrPh":
    "e.g. payment tables only · keep column notes short · drop audit tables",
  "diagrams.ai.presetsLabel": "Quick instructions",
  "diagrams.ai.preset.noAudit": "Exclude audit (_aud) tables",
  "diagrams.ai.preset.noIndex": "Skip index notation",
  "diagrams.ai.preset.inferFk": "Infer logical FKs and link them",
  "diagrams.ai.preset.coreOnly": "Keep only core tables",
  "diagrams.ai.preset.append": "Append to the current diagram",
  "diagrams.ai.generating": "Converting the schema to an ERD…",
  "diagrams.ai.waiting": "Waiting for a response…",
  "diagrams.ai.resultLabel": "Result",
  "diagrams.ai.tabDiagram": "Diagram",
  "diagrams.ai.tabSource": "Source",
  "diagrams.ai.tabDiff": "Changes",

  "diagrams.canvas.hint": "Wheel: zoom · Drag: pan · Double-click: zoom in",
  "diagrams.canvas.fullscreen": "Full screen",
  "diagrams.canvas.fullscreenClose": "Exit full screen (Esc)",
  "diagrams.canvas.rendering": "Rendering…",
  "diagrams.zoom.in": "Zoom in",
  "diagrams.zoom.out": "Zoom out",
  "diagrams.zoom.fit": "Fit",
  "diagrams.zoom.fitTitle": "Fit to view",
  "diagrams.copy": "Copy",

  "diagrams.node.unnamed": "(unnamed)",
  "diagrams.node.line": "Line {n}",
  "diagrams.node.deselect": "Clear selection (Esc)",
  "diagrams.node.copyName": "Copy name",
  "diagrams.node.copied": "Copied",

  "diagrams.mmd.failHead": "Mermaid syntax error — couldn't render, showing the source instead",
  "diagrams.mmd.rendering": "Rendering diagram…",
  "diagrams.mmd.clickToZoom": "Click to zoom in",
  "diagrams.mmd.staleBadge": "Syntax error — showing the last good render",
  "diagrams.mmd.expand": "Zoom",

  "diagrams.diff.none": "No changes",
  "diagrams.diff.lines": "lines changed",
  "diagrams.diff.fold": "⋯ {n} unchanged lines",
} as const;

export const diagramsMessages = { ko, en };
