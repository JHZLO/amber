import { describe, expect, it } from "vitest";
import { isGeneratedErd } from "./erdMigrate";

// 옛 다이어그램 탭이 만든 파일만 정리 대상이다 — 손으로 만든 ERD 를 휴지통에 넣으면 안 된다
describe("isGeneratedErd", () => {
  it("앞머리의 amber:db 표식으로 자동 생성 파일을 알아본다", () => {
    expect(isGeneratedErd("erDiagram\n    %% amber:db MySQL/svc_booking · 2026-10-01 13:26 · 0c4303b0e0503b · g2\n")).toBe(true);
    // 연결 이름에 공백이 있어도
    expect(isGeneratedErd("erDiagram\n    %% amber:db prod ro/svc_x · 2026-09-02 09:41 · abcdef\n")).toBe(true);
  });

  it("표식이 없거나 앞머리에 없으면 손으로 만든 파일이다", () => {
    expect(isGeneratedErd("erDiagram\n    A ||--o{ B : x\n")).toBe(false);
    expect(isGeneratedErd("erDiagram\n    %% 메모: amber:db 처럼 생긴 글\n")).toBe(false);
    expect(isGeneratedErd(["erDiagram", "a", "b", "c", "d", "    %% amber:db MySQL/svc_x · t"].join("\n"))).toBe(false);
  });
});
