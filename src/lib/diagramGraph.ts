// 렌더된 mermaid SVG 에서 "무엇이 무엇과 이어져 있는지"를 읽어낸다 — 노드를 고르면
// 연결된 것만 남기고 나머지를 흐리게 하는 포커스 모드용.
//
// 왜 mermaid 원문을 다시 파싱하지 않나: ER 관계 표기가 (||--o{, }o..o|, …) 여러 가지고
// 별칭·따옴표 이름까지 있어서 재파싱은 틀리기 쉽다. mermaid 가 이미 파싱해 만든 SVG 의
// data-id 를 읽는 편이 정확하다.
//
// data-id 형식 (11.16 · ELK/Dagre 공통으로 확인):
//   일반    id_{source}_{target}_{n}      예) id_entity-a-0_entity-b-1_2
//   자기루프 {node}-cyclic-special-{1|mid|2}  ← dagre 가 자기참조를 3토막 낼 때만
// ELK 는 자기참조도 일반 형식(source===target)으로 낸다.

/** 엣지 data-id 에서 양 끝 노드 id 를 뽑는다. 해석 불가면 null. */
export function parseEdgeEndpoints(
  dataId: string,
  nodeIds: readonly string[],
): { source: string; target: string } | null {
  if (!dataId) return null;

  // dagre 자기루프 3토막 — 세 조각 모두 같은 노드의 자기참조로 친다
  const cyclic = /^(.+)-cyclic-special-(?:1|2|mid)$/.exec(dataId);
  if (cyclic) {
    const id = cyclic[1];
    return nodeIds.includes(id) ? { source: id, target: id } : null;
  }

  if (!dataId.startsWith("id_")) return null;
  const rest = dataId.slice(3);

  // 노드 이름에 '_' 가 들어가서(booking_air_tickets) 단순 split 이 불가능하다 —
  // 알고 있는 노드 id 로 맞춰본다. 긴 것부터 봐야 접두사가 겹치는 이름
  // (entity-a-0 vs entity-a_b-1)에서 짧은 쪽이 먼저 걸리는 오탐을 막는다.
  const byLength = [...nodeIds].sort((a, b) => b.length - a.length);
  for (const source of byLength) {
    if (!rest.startsWith(`${source}_`)) continue;
    const after = rest.slice(source.length + 1);
    for (const target of byLength) {
      // 뒤에 남는 '_{n}' 은 엣지 일련번호라 무시한다
      if (after === target || after.startsWith(`${target}_`))
        return { source, target };
    }
  }
  return null;
}

/** node → 직접 연결된 node 들. 자기참조는 자기 자신을 포함하지 않는다(이미 선택된 노드다). */
export function neighborsOf(
  nodeId: string,
  edges: readonly { source: string; target: string }[],
): Set<string> {
  const out = new Set<string>();
  for (const e of edges) {
    if (e.source === nodeId) out.add(e.target);
    if (e.target === nodeId) out.add(e.source);
  }
  out.delete(nodeId);
  return out;
}
