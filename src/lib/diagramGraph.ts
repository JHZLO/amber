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

// ── ER 컬럼의 널 허용 여부 ──────────────────────────────────────────────
// 두 가지 표기를 모두 읽는다. mermaid 는 널 여부를 **모델링하지 않으므로**
// (11.16 의 Attribute 는 {type,name,keys,comment} 뿐) 둘 다 우리가 해석한다.
//   ① 타입 뒤 `?`  — `string? middle_name`. 파서가 `?` 를 타입 문자열에 그대로 남긴다(실측).
//   ② 코멘트의 `[NULL]`/`[NOTNULL]` — 이 저장소의 기존 ERD 들이 쓰던 방식.

/** `string?` → { type: "string", optional: true }. `?` 없으면 그대로. */
export function splitOptionalType(raw: string): {
  type: string;
  optional: boolean;
} {
  const optional = raw.endsWith("?");
  return { type: optional ? raw.slice(0, -1) : raw, optional };
}

/** 코멘트 앞머리의 `[NULL]`/`[NOTNULL]` 만 뽑는다(없으면 빈 문자열). */
export function nullFlagOf(comment: string): string {
  return /\[\s*(?:NOT\s*NULL|NULL)\s*\]/i.exec(comment)?.[0] ?? "";
}

/** 한 컬럼에 표시할 널 표기.
 *
 *  `entityUsesOptional` = 이 엔티티의 **어느 컬럼이든** `?` 를 쓰고 있는가.
 *  이게 필요한 이유: "`?` 없음 = NOT NULL" 은 작성자가 `?` 규약을 쓸 때만 참이다.
 *  두 규약 다 안 쓰는 다이어그램에까지 `[NOTNULL]` 을 붙이면 **없는 정보를 지어내는** 셈이라,
 *  같은 표에서 `?` 를 한 번이라도 봤을 때만 나머지를 NOT NULL 로 읽는다. */
export function nullabilityFlag(
  rawType: string,
  comment: string,
  entityUsesOptional: boolean,
): string {
  if (splitOptionalType(rawType).optional) return "[NULL]";
  if (entityUsesOptional) return "[NOTNULL]";
  return nullFlagOf(comment);
}

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
