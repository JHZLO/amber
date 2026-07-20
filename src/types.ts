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
}

export type ConceptSort =
  | "canonical" // confidence ASC, last_seen_at ASC(NULLS first), id ASC — 위젯/학습중 기본
  | "recent_updated"
  | "recent_created"
  | "title";

// ---- 할 일 (todos 테이블 한 행, DB 정본) ----

export interface Todo {
  id: number;
  content: string;
  due_date: string; // 'YYYY-MM-DD' — 사용자 로컬 달력 날짜 (UTC ms 아님, lib/date.ts 참조)
  done: 0 | 1;
  completed_at: number | null; // UTC ms, 트리거가 관리
  parent_id: number | null; // 상위 항목 id (null=최상위). 1단계 중첩
  sort_order: number; // 표시 순서(드래그로 조정), 형제 그룹 내 오름차순
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
