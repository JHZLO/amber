import { describe, expect, it } from "vitest";
import {
  neighborsOf,
  nullFlagOf,
  nullabilityFlag,
  parseEdgeEndpoints,
  splitOptionalType,
} from "./diagramGraph";

// 실제 렌더 결과에서 가져온 노드 id 들 (이름에 '_' 가 들어가는 게 핵심 함정)
const NODES = [
  "entity-booking_travelers-0",
  "entity-booking_air_tickets-1",
  "entity-booking_air_ticket_travelers-2",
  "entity-booking_air_ticket_prices-3",
];

describe("parseEdgeEndpoints", () => {
  it("splits a normal edge id even when names contain underscores", () => {
    expect(
      parseEdgeEndpoints(
        "id_entity-booking_air_tickets-1_entity-booking_air_ticket_travelers-2_1",
        NODES,
      ),
    ).toEqual({
      source: "entity-booking_air_tickets-1",
      target: "entity-booking_air_ticket_travelers-2",
    });
  });

  it("reads an ELK self-reference as source === target", () => {
    expect(
      parseEdgeEndpoints(
        "id_entity-booking_travelers-0_entity-booking_travelers-0_0",
        NODES,
      ),
    ).toEqual({
      source: "entity-booking_travelers-0",
      target: "entity-booking_travelers-0",
    });
  });

  it("reads all three dagre cyclic-special segments as one self-reference", () => {
    for (const suffix of ["1", "mid", "2"]) {
      expect(
        parseEdgeEndpoints(
          `entity-booking_travelers-0-cyclic-special-${suffix}`,
          NODES,
        ),
      ).toEqual({
        source: "entity-booking_travelers-0",
        target: "entity-booking_travelers-0",
      });
    }
  });

  it("prefers the longest matching node id over a shorter prefix", () => {
    // entity-a-0 은 entity-a_b-1 의 접두사가 아니지만, 짧은 쪽부터 보면
    // 엉뚱하게 걸릴 수 있는 형태를 일부러 만든다
    const nodes = ["entity-a-0", "entity-a-0_x-1", "entity-b-2"];
    expect(parseEdgeEndpoints("id_entity-a-0_x-1_entity-b-2_0", nodes)).toEqual({
      source: "entity-a-0_x-1",
      target: "entity-b-2",
    });
  });

  it("returns null for ids it cannot resolve", () => {
    expect(parseEdgeEndpoints("", NODES)).toBeNull();
    expect(parseEdgeEndpoints("something-else", NODES)).toBeNull();
    expect(parseEdgeEndpoints("id_entity-unknown-9_entity-other-8_0", NODES)).toBeNull();
    // cyclic 인데 노드 목록에 없는 경우
    expect(parseEdgeEndpoints("entity-gone-9-cyclic-special-1", NODES)).toBeNull();
  });
});

describe("neighborsOf", () => {
  const edges = [
    { source: "a", target: "b" },
    { source: "a", target: "c" },
    { source: "d", target: "a" },
    { source: "b", target: "c" },
    { source: "a", target: "a" }, // 자기참조
  ];

  it("collects both directions", () => {
    expect([...neighborsOf("a", edges)].sort()).toEqual(["b", "c", "d"]);
  });

  it("excludes the node itself even with a self-reference", () => {
    expect(neighborsOf("a", edges).has("a")).toBe(false);
  });

  it("returns empty for an unconnected node", () => {
    expect(neighborsOf("zzz", edges).size).toBe(0);
  });
});

describe("splitOptionalType", () => {
  it("strips a trailing ? and marks the column optional", () => {
    expect(splitOptionalType("string?")).toEqual({ type: "string", optional: true });
    // 괄호가 붙은 타입도 mermaid 가 그대로 통과시킨다(실측)
    expect(splitOptionalType("varchar(31)?")).toEqual({
      type: "varchar(31)",
      optional: true,
    });
  });

  it("leaves a plain type untouched", () => {
    expect(splitOptionalType("bigint")).toEqual({ type: "bigint", optional: false });
    expect(splitOptionalType("")).toEqual({ type: "", optional: false });
  });
});

describe("nullFlagOf", () => {
  it("picks the marker out of a comment, whatever follows it", () => {
    expect(nullFlagOf("[NOTNULL] 논리 FK -> x.id; uk(a,b)")).toBe("[NOTNULL]");
    expect(nullFlagOf("[NULL] 출국편 잔여 좌석 수")).toBe("[NULL]");
    expect(nullFlagOf("[NOT NULL]")).toBe("[NOT NULL]");
  });

  it("returns empty when the comment is prose", () => {
    expect(nullFlagOf("출발 공항 IATA")).toBe("");
    expect(nullFlagOf("")).toBe("");
  });
});

describe("nullabilityFlag", () => {
  it("reads the ? suffix first", () => {
    expect(nullabilityFlag("string?", "", true)).toBe("[NULL]");
    // ? 가 코멘트 표기를 이긴다 — 타입 쪽이 더 구체적인 선언이다
    expect(nullabilityFlag("string?", "[NOTNULL] 설명", true)).toBe("[NULL]");
  });

  it("infers NOT NULL only when the table actually uses ?", () => {
    expect(nullabilityFlag("bigint", "", true)).toBe("[NOTNULL]");
    // ? 를 아무도 안 쓰는 표에서 NOT NULL 을 지어내면 안 된다
    expect(nullabilityFlag("bigint", "", false)).toBe("");
  });

  it("falls back to the comment convention", () => {
    expect(nullabilityFlag("bigint", "[NOTNULL] 회원 ID", false)).toBe("[NOTNULL]");
    expect(nullabilityFlag("int", "[NULL] 잔여 좌석", false)).toBe("[NULL]");
    expect(nullabilityFlag("varchar", "출발 공항", false)).toBe("");
  });
});
