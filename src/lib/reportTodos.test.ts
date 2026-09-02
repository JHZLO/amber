import { describe, expect, it } from "vitest";
import { todoDigestLines, type DigestTodo } from "./reportTodos";

const todo = (
  id: number,
  parent_id: number | null,
  content: string,
  done = 0,
): DigestTodo => ({ id, parent_id, content, done });

describe("todoDigestLines", () => {
  const tree = [
    todo(1, null, "Devops"),
    todo(2, 1, "kafka 리밸런싱 알고리즘 변경"),
    todo(3, 2, "GracefulKafkaModule 신설 및 배포"),
    todo(4, 3, "개발 적용", 1),
    todo(5, 3, "운영 적용"),
  ];

  it("깊이를 들여쓰기로, 체크를 [x] 로 찍는다", () => {
    const lines = todoDigestLines(tree);
    expect(lines[0]).toBe("- [ ] Devops");
    expect(lines[2].startsWith("    - [ ] GracefulKafkaModule")).toBe(true);
    expect(lines[3].startsWith("      - [x] 개발 적용")).toBe(true);
  });

  it("자식 줄에 상위 경로를 붙인다 — 한 줄만 떼어 봐도 무슨 일인지 알게", () => {
    const lines = todoDigestLines(tree);
    expect(lines[3]).toContain(
      "(상위: Devops › kafka 리밸런싱 알고리즘 변경 › GracefulKafkaModule 신설 및 배포)",
    );
  });

  it("최상위 줄에는 경로를 붙이지 않는다", () => {
    expect(todoDigestLines(tree)[0]).not.toContain("상위:");
  });

  it("긴 조상 제목은 잘라 붙인다", () => {
    const long = "가".repeat(80);
    const lines = todoDigestLines([todo(1, null, long), todo(2, 1, "자식", 1)]);
    expect(lines[1]).toContain(`(상위: ${"가".repeat(49)}…)`);
  });

  it("부모가 목록에 없어도 자식이 사라지지 않는다 (visibleRoots)", () => {
    // 이월된 부모가 걸러진 상황 — 그 아래 체크된 자식은 그날의 성과라 남아야 한다
    const lines = todoDigestLines([todo(9, 7, "체크된 자식", 1)]);
    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain("체크된 자식");
  });

  it("parent_id 가 순환해도 무한 루프에 빠지지 않는다", () => {
    const lines = todoDigestLines([todo(1, 2, "가"), todo(2, 1, "나")]);
    expect(lines.length).toBeLessThanOrEqual(2);
  });

  it("밀린 항목은 별도 묶음으로 뒤에 붙인다", () => {
    const lines = todoDigestLines([todo(1, null, "오늘 일")], [
      { ...todo(5, null, "지난 일"), due_date: "2026-08-30" },
    ]);
    expect(lines[1]).toBe("");
    expect(lines[2]).toBe("밀린(미완료) 항목:");
    expect(lines[3]).toContain("지난 일");
    expect(lines[3]).toContain("원래");
  });
});
