// report 도메인 문자열 — 키는 "report." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).
// 주의: report.ts 의 digest 빌더 텍스트(프롬프트 재료)는 여기 없다 — UI 노출 문자열만 담는다.

const ko = {
  // 패널·설정 공통 제목
  "report.title": "데일리 리포트",

  // 설정 — 헤더·소스 목록
  "report.set.desc":
    "켠 플랫폼만 수집해요. 위에 둘수록 리포트의 중심이 됩니다(순위 = 행 순서, 드래그로 조정).",
  "report.set.redetect": "다시 감지",
  "report.set.detecting": "감지 중…",
  "report.set.dragRank": "드래그해서 순위 변경",
  "report.set.collapse": "접기",
  "report.set.expand": "펼치기",

  // 소스 이름(브랜드 제외)·설명
  "report.source.todos": "투두",
  "report.source.aiSessions": "AI 세션",
  "report.sub.github": "내 계정 활동 이력",
  "report.sub.aiSessions": "로컬 세션 요약",
  "report.sub.slack": "MCP · 메시지·스레드",
  "report.sub.notion": "MCP · 페이지·코멘트",

  // 소스 상태 배지
  "report.status.ghMissing": "gh 설치 필요",
  "report.status.detected": "감지됨",
  "report.status.noSessions": "세션 없음",
  "report.status.claudeRequired": "claude 필요",
  "report.status.pickServer": "서버 선택",
  "report.status.checking": "확인 중…",
  "report.status.unknown": "미확인",
  "report.status.notConnected": "미연결",
  "report.mcpStatus.connected": "연결됨",
  "report.mcpStatus.needsAuth": "인증 필요",
  "report.mcpStatus.failed": "연결 실패",
  "report.mcpStatus.pending": "승인 대기",

  // GitHub 소스 설정
  "report.gh.accountLabel": "조회 계정",
  "report.gh.accountDefault": "활성 계정 (기본)",
  "report.gh.accountActive": "활성",
  "report.gh.accountHint":
    "gh 에 계정이 여러 개면 리포트에 쓸 계정을 고르세요. 그 계정으로 조회해요(전역 활성 계정은 안 바뀜).",
  "report.gh.pathLabel": "gh CLI 경로",
  "report.gh.pathHint":
    "비우면 로그인 셸 PATH 에서 자동으로 찾아요. gh 자체 로그인을 그대로 사용합니다.",
  "report.gh.repoLabel": "레포 필터 (선택)",
  "report.gh.repoPlaceholder": "owner/repo, owner/other — 비우면 전체",
  "report.gh.repoHint": "지정하면 그 레포 활동만 모아 잡음을 줄여요.",

  // AI 세션 소스 설정
  "report.sess.claude": "Claude Code 세션",
  "report.sess.codex": "Codex 세션",
  "report.sess.noFolder": "세션 폴더 없음",

  // MCP(Slack·Notion) 소스 설정 — claudeOnly 는 <b>AI 연결</b> 강조를 위해 3분할
  "report.mcp.claudeOnlyPre": "{name} 수집은 claude 프로바이더 전용이에요. 설정 상단 ",
  "report.mcp.claudeOnlyLink": "AI 연결",
  "report.mcp.claudeOnlyPost": "에서 claude 에 연결하면 등록된 MCP 서버를 그대로 사용합니다.",
  "report.mcp.serverLabel": "MCP 서버",
  "report.mcp.serverNone": "선택 안 함",
  "report.mcp.searching": "등록된 서버를 찾는 중…",
  "report.mcp.noneGuide":
    "claude 에 등록·인증된 MCP 서버가 없어요. 터미널에서 한 번만 등록·인증하면 이후 자동 재사용돼요:",
  "report.mcp.authStep": "claude → /mcp → 브라우저 인증",
  "report.mcp.tokenHint": "인증은 claude CLI 가 관리해요. Amber 는 토큰을 저장하지 않습니다.",
  "report.mcp.redetect": "서버 다시 감지",

  // 패널 — 생성 버튼·단계·빈 상태
  "report.generate": "리포트 생성",
  "report.connectGenerate": "AI 연결하고 생성",
  "report.futureNo": "미래 날짜는 생성할 수 없어요",
  "report.hintFuture": "미래 날짜예요. 지난 날짜나 오늘을 선택해 생성하세요.",
  "report.hintIdle": "투두와 연동 플랫폼 활동으로 하루를 정리해 드려요.",
  "report.collecting": "활동을 모으는 중…",
  "report.summarizing": "요약하는 중…",
  "report.srcFailed": "{name} 실패",
  "report.srcFailedTitle": "수집 실패",
  "report.empty": "이 날짜엔 기록된 활동이 없어요. 투두를 체크하거나 다른 날짜를 골라보세요.",

  // 패널 — 완료 액션·확인 모달
  "report.copy": "복사",
  "report.regen": "다시 생성",
  "report.regenTitle": "리포트 다시 생성",
  "report.regenBody": "기존 리포트를 새로 생성한 내용으로 덮어써요. 계속할까요?",
  "report.deleteTitle": "리포트 삭제",
  "report.deleteBody": "이 날짜의 리포트를 삭제해요. 되돌릴 수 없어요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "report.title": "Daily Report",

  "report.set.desc":
    "Only enabled platforms are collected. Sources higher up carry more weight in the report (rank = row order; drag to reorder).",
  "report.set.redetect": "Re-detect",
  "report.set.detecting": "Detecting…",
  "report.set.dragRank": "Drag to change rank",
  "report.set.collapse": "Collapse",
  "report.set.expand": "Expand",

  "report.source.todos": "To-dos",
  "report.source.aiSessions": "AI Sessions",
  "report.sub.github": "Your account activity",
  "report.sub.aiSessions": "Local session summaries",
  "report.sub.slack": "MCP · messages & threads",
  "report.sub.notion": "MCP · pages & comments",

  "report.status.ghMissing": "gh not installed",
  "report.status.detected": "Detected",
  "report.status.noSessions": "No sessions",
  "report.status.claudeRequired": "claude required",
  "report.status.pickServer": "Select a server",
  "report.status.checking": "Checking…",
  "report.status.unknown": "Not found",
  "report.status.notConnected": "Not connected",
  "report.mcpStatus.connected": "Connected",
  "report.mcpStatus.needsAuth": "Needs auth",
  "report.mcpStatus.failed": "Connection failed",
  "report.mcpStatus.pending": "Awaiting approval",

  "report.gh.accountLabel": "Query account",
  "report.gh.accountDefault": "Active account (default)",
  "report.gh.accountActive": "active",
  "report.gh.accountHint":
    "If gh has multiple accounts, choose the one to use for reports. Queries run as that account (the global active account is not changed).",
  "report.gh.pathLabel": "gh CLI path",
  "report.gh.pathHint":
    "Leave blank to find it automatically on your login shell PATH. Uses gh's own login as-is.",
  "report.gh.repoLabel": "Repo filter (optional)",
  "report.gh.repoPlaceholder": "owner/repo, owner/other — blank = all",
  "report.gh.repoHint": "When set, only activity from those repos is collected, cutting noise.",

  "report.sess.claude": "Claude Code sessions",
  "report.sess.codex": "Codex sessions",
  "report.sess.noFolder": "No session folder",

  "report.mcp.claudeOnlyPre": "{name} collection needs the claude provider. Connect to claude under ",
  "report.mcp.claudeOnlyLink": "AI connection",
  "report.mcp.claudeOnlyPost": " at the top of Settings to reuse its registered MCP servers.",
  "report.mcp.serverLabel": "MCP server",
  "report.mcp.serverNone": "None",
  "report.mcp.searching": "Looking for registered servers…",
  "report.mcp.noneGuide":
    "No MCP server is registered and authenticated with claude. Register and authenticate once in a terminal, and it's reused automatically from then on:",
  "report.mcp.authStep": "claude → /mcp → authenticate in browser",
  "report.mcp.tokenHint": "The claude CLI manages authentication. Amber never stores tokens.",
  "report.mcp.redetect": "Re-detect servers",

  "report.generate": "Generate report",
  "report.connectGenerate": "Connect AI & generate",
  "report.futureNo": "Future dates can't be generated",
  "report.hintFuture": "That's a future date. Pick today or a past day to generate.",
  "report.hintIdle": "Turns your to-dos and connected platform activity into a daily wrap-up.",
  "report.collecting": "Gathering activity…",
  "report.summarizing": "Summarizing…",
  "report.srcFailed": "{name} failed",
  "report.srcFailedTitle": "Collection failed",
  "report.empty": "No activity recorded for this date. Check off a to-do or pick another day.",

  "report.copy": "Copy",
  "report.regen": "Regenerate",
  "report.regenTitle": "Regenerate report",
  "report.regenBody": "The existing report will be overwritten with a newly generated one. Continue?",
  "report.deleteTitle": "Delete report",
  "report.deleteBody": "This deletes the report for this date. It can't be undone.",
} as const;

export const reportMessages = { ko, en };
