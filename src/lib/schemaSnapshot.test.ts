// 연동 헤더·스냅샷 비교의 순수 규칙. 헤더는 파일이 "DB 연동 파일"임을 알아보는 유일한 표식이라
// 형식이 흔들리면 배너·동기화 버튼이 통째로 사라진다 — 라운드트립과 구버전 호환을 여기서 고정한다.

import { describe, expect, it } from "vitest";
import {
  diffIsEmpty,
  diffSnapshots,
  finalizeSnapshot,
  formatDbHeader,
  parseDbHeader,
  type RawSnapshot,
} from "./schemaSnapshot";

const at = new Date(2026, 8, 2, 9, 41);

describe("연동 헤더", () => {
  it("지문·규칙 버전·noaud 를 붙이고 그대로 읽어 낸다", () => {
    const line = formatDbHeader("dev", "svc_booking", at, "3f9a1c0b2d4e5f", { audit: false, gen: 2 });
    expect(line).toBe("    %% amber:db dev/svc_booking · 2026-09-02 09:41 · 3f9a1c0b2d4e5f · g2 · noaud");
    expect(parseDbHeader(`erDiagram\n${line}\n    %% legend`)).toEqual({
      connection: "dev",
      schema: "svc_booking",
      generatedAt: "2026-09-02 09:41",
      fingerprint: "3f9a1c0b2d4e5f",
      audit: false,
      gen: 2,
    });
  });

  it("표식이 없던 첫 버전 헤더는 gen 1·audit true 로 읽힌다 (기존 파일 호환)", () => {
    const h = parseDbHeader("erDiagram\n    %% amber:db MySQL/svc_accounts · 2026-09-02 16:15 · 0ad65901762974\n");
    expect(h).toMatchObject({ connection: "MySQL", schema: "svc_accounts", fingerprint: "0ad65901762974", audit: true, gen: 1 });
  });

  it("연결 이름의 공백·슬래시 뒤 스키마를 바르게 가른다", () => {
    const h = parseDbHeader(formatDbHeader("prod ro", "svc_booking", at, "abcdef012345"));
    expect(h?.connection).toBe("prod ro");
    expect(h?.schema).toBe("svc_booking");
  });

  it("헤더가 없는 손으로 만든 다이어그램은 null", () => {
    expect(parseDbHeader("erDiagram\n    A ||--o{ B : x\n")).toBeNull();
  });
});

describe("스냅샷 비교", () => {
  const raw = (cols: string[], comment = ""): RawSnapshot => ({
    connection: "c",
    schema: "s",
    server: "MySQL 8",
    synced_at: 0,
    tables: [
      {
        name: "t",
        comment,
        rows_estimate: null,
        columns: cols.map((name) => ({
          name,
          data_type: "bigint",
          column_type: "bigint",
          nullable: false,
          key: "",
          default_value: null,
          extra: "",
          comment: "",
        })),
        indexes: [],
        foreign_keys: [],
        checks: [],
      },
    ],
  });

  it("컬럼 추가는 지문을 바꾸고 diff 에 잡힌다", () => {
    const a = finalizeSnapshot(raw(["id"]));
    const b = finalizeSnapshot(raw(["id", "note"]));
    expect(a.fingerprint).not.toBe(b.fingerprint);
    expect(diffSnapshots(a, b).columnsAdded).toEqual(["t.note"]);
  });

  it("같은 구조면 지문이 같고 diff 는 비어 있다", () => {
    const a = finalizeSnapshot(raw(["id", "note"]));
    const b = finalizeSnapshot({ ...raw(["id", "note"]), synced_at: 99 });
    expect(a.fingerprint).toBe(b.fingerprint);
    expect(diffIsEmpty(diffSnapshots(a, b))).toBe(true);
  });
});
