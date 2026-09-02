// 리포트에 넣을 투두 digest 의 **줄 만들기**만 담당한다 (DB 접근은 report.ts).
// 여기 따로 둔 이유는 테스트다 — report.ts 는 Tauri 모듈을 import 해서 node 환경에서 못 돈다.

import { visibleRoots } from "./todoTree";
import { formatDayShort } from "./date";

/** digest 에 필요한 최소 형태 — Todo 전체를 요구하지 않아 테스트에서 쓰기 쉽다 */
export interface DigestTodo {
  id: number;
  parent_id: number | null;
  content: string;
  done: number;
  due_date?: string;
}

/** 상위 경로에 적을 조상 제목의 최대 길이. 투두 문구는 한 줄이 넘기도 해서 자른다 */
const TRAIL_CLIP = 50;

const clip = (s: string) =>
  s.length > TRAIL_CLIP ? `${s.slice(0, TRAIL_CLIP - 1)}…` : s;

/** 투두 트리 → digest 줄들.
 *
 *  들여쓰기만으로는 부족해서 **자식 줄마다 상위 경로를 붙인다.** 실측 실패: 3단 아래의
 *  `[x] 개발 적용` 이 프롬프트에서 들여쓰기로만 표현돼 있었더니, AI 가 그 줄을 목록 저 위쪽의
 *  다른 작업에 붙여 "개발 환경 적용" 항목을 엉뚱한 일 밑에 썼다. 한 줄만 떼어 봐도 무슨 일의
 *  어느 단계인지 알 수 있어야 한다. */
export function todoDigestLines(
  todos: readonly DigestTodo[],
  overdue: readonly DigestTodo[] = [],
): string[] {
  const tops = visibleRoots(todos);
  const kids = (pid: number) => todos.filter((t) => t.parent_id === pid);
  const mark = (t: DigestTodo) => (t.done === 1 ? "[x]" : "[ ]");

  const lines: string[] = [];
  // 중첩 깊이는 무제한이라 재귀로 끝까지 내려간다(2단만 찍으면 하위 계획이 프롬프트에서 통째로 빠진다).
  // seen 은 parent_id 가 순환하도록 망가진 데이터에서 무한 루프를 막는 안전장치.
  const seen = new Set<number>();
  const walk = (t: DigestTodo, depth: number, trail: readonly string[]) => {
    if (seen.has(t.id)) return;
    seen.add(t.id);
    const path = trail.length ? `  (상위: ${trail.map(clip).join(" › ")})` : "";
    lines.push(`${"  ".repeat(depth)}- ${mark(t)} ${t.content}${path}`);
    for (const c of kids(t.id)) walk(c, depth + 1, [...trail, t.content]);
  };
  for (const p of tops) walk(p, 0, []);

  if (overdue.length) {
    lines.push("", "밀린(미완료) 항목:");
    for (const t of overdue.slice(0, 20)) {
      const when = t.due_date ? ` (원래 ${formatDayShort(t.due_date)})` : "";
      lines.push(`- [ ] ${t.content}${when}`);
    }
  }
  return lines;
}
