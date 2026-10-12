// 스냅샷 비교의 순수 규칙 — 지문이 같으면 구조가 같고, 다르면 diff 가 무엇이 바뀌었는지 말한다.

import { describe, expect, it } from "vitest";
import {
  diffIsEmpty,
  diffSnapshots,
  finalizeSnapshot,
  type RawSnapshot,
} from "./schemaSnapshot";

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
