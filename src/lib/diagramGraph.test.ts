import { describe, expect, it } from "vitest";
import { neighborsOf, parseEdgeEndpoints } from "./diagramGraph";

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
