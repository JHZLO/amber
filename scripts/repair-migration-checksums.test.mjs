// `--apply` 게이트의 순수부 회귀 테스트.
//
// 왜 이게 중요한가: 이 스크립트는 "파일 변경이 주석/공백뿐일 때만 안전하다"고 헤더에 적어두고도
// 그걸 **검증하지 않았다**. DDL 이 바뀐 상태에서 돌리면 sqlx 가 그 마이그레이션을 영구히
// "적용됨"으로 믿어 DDL 이 실행되지 않고, 앱은 조용히 "no such column" 을 낸다.
// normalizeSql 이 그 판정의 전부이므로 여기서 고정한다.

import { describe, expect, it } from "vitest";
import { normalizeSql } from "./repair-migration-checksums.mjs";

describe("normalizeSql — 주석 변경과 SQL 변경을 가른다", () => {
  const DDL = `-- 할 일 테이블
CREATE TABLE todos (
  id INTEGER PRIMARY KEY,
  content TEXT NOT NULL
);`;

  it("줄 주석만 바뀌면 같다", () => {
    const after = `-- 할 일 테이블 (2026-09 설명 보강)
CREATE TABLE todos (
  id INTEGER PRIMARY KEY,
  content TEXT NOT NULL
);`;
    expect(normalizeSql(after)).toBe(normalizeSql(DDL));
  });

  it("주석 줄이 통째로 추가돼도 같다", () => {
    const after = `-- 할 일 테이블
-- 왜 sort_order 가 없나: 0003 에서 따로 얹는다
CREATE TABLE todos (
  id INTEGER PRIMARY KEY,
  content TEXT NOT NULL
);`;
    expect(normalizeSql(after)).toBe(normalizeSql(DDL));
  });

  it("들여쓰기 폭만 달라도 같다", () => {
    const after = DDL.replace(/^ {2}/gm, "    ");
    expect(normalizeSql(after)).toBe(normalizeSql(DDL));
  });

  // 줄을 합치는 재포맷은 구두점 옆 공백이 달라져 "다름"으로 잡힌다. 정규화를 더 똑똑하게
  // 만들 수도 있지만 그러지 않는다 — 이 판정의 두 실패 방향은 대가가 다르다.
  // 잘못 "같다"고 하면 DDL 이 영구히 유실되고, 잘못 "다르다"고 하면 그냥 거부될 뿐이다.
  it("줄을 합치는 재포맷은 보수적으로 다르게 본다 (거부가 안전한 방향)", () => {
    const oneLine =
      "CREATE TABLE todos (id INTEGER PRIMARY KEY, content TEXT NOT NULL);";
    expect(normalizeSql(oneLine)).not.toBe(normalizeSql(DDL));
  });

  it("블록 주석도 걷어낸다", () => {
    const after = `/* 할 일 테이블 */
CREATE TABLE todos (
  id INTEGER PRIMARY KEY,
  content TEXT NOT NULL
);`;
    expect(normalizeSql(after)).toBe(normalizeSql(DDL));
  });

  it("컬럼이 하나라도 늘면 다르다 — 여기서 막아야 한다", () => {
    const after = `-- 할 일 테이블
CREATE TABLE todos (
  id INTEGER PRIMARY KEY,
  content TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0
);`;
    expect(normalizeSql(after)).not.toBe(normalizeSql(DDL));
  });

  it("제약 조건이 바뀌면 다르다", () => {
    const after = DDL.replace("content TEXT NOT NULL", "content TEXT");
    expect(normalizeSql(after)).not.toBe(normalizeSql(DDL));
  });

  it("테이블 이름이 바뀌면 다르다", () => {
    const after = DDL.replace("todos", "tasks");
    expect(normalizeSql(after)).not.toBe(normalizeSql(DDL));
  });

  it("빈 문자열과 주석만 있는 파일은 같은 것으로 본다", () => {
    expect(normalizeSql("-- 아무것도 없음\n\n")).toBe("");
    expect(normalizeSql("")).toBe("");
  });
});
