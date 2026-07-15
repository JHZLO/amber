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
