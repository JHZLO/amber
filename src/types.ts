// 도메인 타입 (PRD §7 데이터 모델과 1:1)

export type ConceptStatus = "learning" | "learned";
export type Confidence = 1 | 2 | 3;
export type SourceKind = "paste" | "url" | "file" | null;

/** concepts 테이블 한 행 (DB가 정본인 필드들) */
export interface Concept {
  id: number;
  ulid: string;
  title: string;
  summary: string;
  detail_path: string; // vault 기준 상대경로 'concepts/<ulid>/index.md'
  status: ConceptStatus;
  confidence: Confidence;
  source: string | null;
  source_kind: SourceKind;
  seen_count: number;
  last_seen_at: number | null; // UTC ms
  created_at: number; // UTC ms
  updated_at: number; // UTC ms
  learned_at: number | null; // UTC ms
}

/** 리스트/상세에서 태그까지 붙인 뷰 */
export interface ConceptWithTags extends Concept {
  tags: string[];
}

/** Claude headless 정리 결과 계약 (PRD §6.1) */
export interface ClaudeNote {
  title: string;
  summary: string;
  detail_markdown: string;
  tags: string[];
  confidence_suggestion: Confidence;
  source_excerpt?: string | null;
}

/** 관리 창 리스트 필터 */
export interface ConceptFilter {
  status: ConceptStatus | "all";
  search?: string; // 제목/요약/태그 substring
  tags?: string[]; // AND 교집합
  sort?: ConceptSort;
  /** 최대 행 수. ⌘K 처럼 앞의 몇 개만 그리는 화면이 전체 아카이브를 끌어오지 않게 */
  limit?: number;
}

export type ConceptSort =
  | "canonical" // confidence ASC, last_seen_at ASC(NULLS first), id ASC — 위젯/학습중 기본
  | "recent_updated"
  | "recent_created"
  | "title";

// ---- 할 일 (todos 테이블 한 행, DB 정본) ----

/** 할 일이 걸린 좌표의 단위 (migrations/0012).
 *  'day'  = due_date 가 그 날짜
 *  'week' = due_date 가 **그 주 시작일** — 요일을 정하지 않은 '이번 주에 할 것' */
export type TodoScope = "day" | "week";

/** 할 일 섹션의 선택 단위 — 캘린더가 하루를 고르는 판인지 주를 고르는 판인지.
 *  값은 TodoScope 와 같지만 뜻이 다르다(이건 화면 상태, 저건 행의 성격). */
export type TodoUnit = TodoScope;

export interface Todo {
  id: number;
  content: string;
  due_date: string; // 'YYYY-MM-DD' — 사용자 로컬 달력 날짜 (UTC ms 아님, lib/date.ts 참조)
  /** day = 그 날짜의 항목, week = 그 주(due_date=주 시작일)의 항목.
   *  주 항목의 due_date 도 실재하는 날짜라, 일별 조회는 반드시 scope='day' 를 건다. */
  scope: TodoScope;
  done: 0 | 1;
  completed_at: number | null; // UTC ms, 트리거가 관리
  parent_id: number | null; // 상위 항목 id (null=최상위). 다단계 중첩(무제한 깊이)
  sort_order: number; // 표시 순서(드래그로 조정), 형제 그룹 내 오름차순
  created_at: number; // UTC ms
  updated_at: number; // UTC ms
  /** 이 날짜 목록에서 '이월 고스트'인가 — 조회한 날짜에 있었지만 지금은 due_date 가 다른 줄.
   *  DB 컬럼이 아니라 listTodos 가 붙이는 표식이라, 다른 조회는 undefined 로 온다.
   *  고스트의 내용·부모는 이월 시점 스냅샷이고(migrations/0014), 라이브 행이 살아있는 한
   *  **같은 항목**이다 — 여기서 체크하면 그 할 일이 완료된다(migrations/0008 참조). */
  carried?: 0 | 1;
  /** 라이브 행이 사라진 이월 고스트 — '그 날 이런 게 있었다'는 기록만 남은 줄.
   *  carried=1 일 때만 의미가 있다. 체크·편집·이동이 없고, 그 날짜에서 따로 치울 수 있다. */
  gone?: 0 | 1;
}

/** 시간 블록(타임테이블) — 선택 날짜의 시간 계획. 시간은 자정 기준 분(로컬 벽시계) */
export interface TimeBlock {
  id: number;
  date: string; // 'YYYY-MM-DD' — 로컬 달력 날짜 (lib/date.ts)
  start_min: number; // 자정 기준 분, 0~1435
  end_min: number; // start_min < end_min <= 1440
  title: string; // 연동 블록은 '' — 표시할 땐 연결된 할 일 내용을 미러
  todo_id: number | null; // 연결된 할 일. 할 일이 지워지면 NULL 로 떨어진다(0014) —
  // 기록이 남는 날짜의 블록은 그 날의 기록이라 살리고, 그 전에 title 에 이름을 찍어둔다
  created_at: number; // UTC ms
  updated_at: number; // UTC ms
}

/** 캘린더 날짜별 개수 (점·월 요약용) */
export interface DayTodoCount {
  due_date: string; // 'YYYY-MM-DD'
  total: number;
  done: number;
}

// ---- 데일리 리포트 (daily_reports 테이블 한 행 + 수집 계층) ----

/** 연동 플랫폼 소스 식별자. P1 은 github·ai_sessions 만 동작(slack·notion 은 P2). */
export type ReportSourceId = "github" | "ai_sessions" | "slack" | "notion";

/** 소스 활성화 상태. 배열에서의 순서(index)가 곧 우선순위(rank) */
export interface ReportSourcePref {
  id: ReportSourceId;
  enabled: boolean;
}

/** Rust report_collect 가 소스별로 돌려주는 수집 결과 (생성 재료 + UI 근거) */
export interface SourceDigest {
  id: string;
  rank: number;
  ok: boolean;
  items: number;
  digest_md: string;
  error: string | null;
}

/** 수집 진행 알림(Channel) — 소스별로 끝나는 대로 도착 */
export interface CollectProgress {
  id: string;
  ok: boolean;
  items: number;
  error: string | null;
}

/** 등록된 MCP 서버 (claude mcp list 파싱, P2 Slack·Notion 소스 선택용) */
export interface McpServer {
  name: string;
  connected: boolean;
  status: string; // connected | needs_auth | failed | pending | unknown
  transport: string; // http | sse | stdio
}

/** 생성 시 claude 가 직접 조회할 MCP 소스 (report_generate 로 전달) */
export interface McpSource {
  id: ReportSourceId; // slack | notion
  rank: number;
  server: string; // 등록 서버 이름
}

/** daily_reports 테이블 한 행 (본문 정본은 vault/reports/<date>.md 파일) */
export interface DailyReport {
  id: number;
  report_date: string; // 'YYYY-MM-DD'
  file_path: string; // vault 기준 상대경로 'reports/<date>.md'
  sources_json: string; // 생성 근거 스냅샷 [{id,rank,ok,items,error}]
  provider: string | null;
  model: string | null;
  duration_ms: number | null;
  created_at: number; // UTC ms
  updated_at: number; // UTC ms
}

/** 주간 리포트 메타 (migrations/0011). 본문은 vault/reports/<월요일>-week.md */
export interface WeeklyReport {
  id: number;
  week_start: string; // 'YYYY-MM-DD' — 그 주의 시작일 (lib/date.ts WEEK_STARTS_ON)
  file_path: string;
  /** 묶은 일간 리포트 날짜 목록 ["2026-08-17", …] — 빈 날은 담기지 않는다 */
  sources_json: string;
  provider: string | null;
  model: string | null;
  duration_ms: number | null;
  created_at: number;
  updated_at: number;
}
