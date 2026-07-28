// Rust 에러 코드 ↔ 프론트 문구 매핑 교차 검증.
// Rust 에 코드를 추가하고 lib/errors.ts 에 안 넣으면, 영어 UI 에서 Rust 의 한국어 폴백 문구가
// 그대로 노출된다 — 타입으로는 못 잡히는(문자열 코드) 구멍이라 소스를 직접 읽어 막는다.

import { describe, expect, it } from "vitest";
import { CODE_KEY, errText } from "./errors";

// @types/node 를 끌어오지 않으려고 fs 대신 vite 의 raw glob 으로 Rust 소스를 읽는다
const RUST_SOURCES = import.meta.glob("/src-tauri/src/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Rust 소스에서 AiError::new / ::detailed 의 코드 리터럴을 모은다 (개행 포함 호출도 잡는다) */
function rustErrorCodes(): string[] {
  const codes = new Set<string>();
  for (const src of Object.values(RUST_SOURCES)) {
    for (const m of src.matchAll(/AiError::(?:new|detailed)\(\s*"([A-Z_]+)"/g)) {
      codes.add(m[1]);
    }
    // (code, msg) 튜플로 코드를 고르는 분기도 있다 — 문자열 리터럴만 훑어 보수적으로 수집
    for (const m of src.matchAll(/\(\s*"([A-Z]{2,}[A-Z_]*)",\s*\n?\s*"/g)) {
      codes.add(m[1]);
    }
  }
  return [...codes].sort();
}

describe("Rust 에러 코드 매핑", () => {
  it("Rust 가 쓰는 모든 코드가 프론트 문구를 가진다", () => {
    const missing = rustErrorCodes().filter((c) => !(c in CODE_KEY));
    expect(missing).toEqual([]);
  });

  it("실제로 코드를 수집했다 — 정규식이 조용히 0건이 되면 이 테스트가 무의미해진다", () => {
    expect(rustErrorCodes().length).toBeGreaterThan(10);
  });
});

describe("errText", () => {
  it("코드가 있으면 번역 문구를 쓴다 (Rust 의 message 를 노출하지 않는다)", () => {
    const out = errText({ code: "BACKUP_NO_DEST", message: "RUST_FALLBACK" });
    expect(out).not.toContain("RUST_FALLBACK");
    expect(out.length).toBeGreaterThan(0);
  });

  it("{detail} 을 끼워 넣는다", () => {
    const out = errText({ code: "AI_TIMEOUT", message: "x", detail: "42" });
    expect(out).toContain("42");
  });

  it("미등록 코드는 Rust 폴백 문구로 — 그래도 무언가는 보여 준다", () => {
    expect(errText({ code: "NOPE_UNKNOWN", message: "fallback text" })).toBe(
      "fallback text",
    );
    expect(errText({ code: "NOPE_UNKNOWN", message: "" })).toBe("NOPE_UNKNOWN");
  });

  it("평범한 Error·문자열도 처리한다", () => {
    expect(errText(new Error("boom"))).toBe("boom");
    expect(errText("plain")).toBe("plain");
  });
});
