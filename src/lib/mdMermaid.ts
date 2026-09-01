// 여는 펜스에 언어를 안 적은 코드블록이 실은 mermaid 인지 첫 줄로 알아본다.
//
// 왜 필요한가: 렌더는 `language-mermaid` 클래스로 판별하는데, 노트를 쓴 모델이 ```mermaid 의
// `mermaid` 를 빠뜨리면 다이어그램이 통째로 코드블록으로 굳는다. 실측으로 노트 하나에서
// 다이어그램 16개가 전부 그렇게 굳었고, 파일을 고치지 않는 한 되살릴 방법이 없었다.
// 프롬프트에서 태그를 강제하는 것과 별개로, **이미 쓰인 노트도 살아나야** 한다.
//
// 언어를 **적은** 블록에는 절대 끼어들지 않는다 — ```ts 안의 첫 줄이 우연히 걸리는 일이 없게.

/** mermaid 다이어그램 선언 키워드 (mermaid 11 기준). 첫 줄이 이걸로 시작하면 다이어그램이다 */
const DECLARATIONS = [
  "sequenceDiagram",
  "stateDiagram-v2",
  "stateDiagram",
  "flowchart",
  "graph", // 구버전 flowchart 문법
  "erDiagram",
  "classDiagram-v2",
  "classDiagram",
  "journey",
  "gantt",
  "pie",
  "mindmap",
  "timeline",
  "gitGraph",
  "quadrantChart",
  "requirementDiagram",
  "sankey-beta",
  "xychart-beta",
  "block-beta",
  "architecture-beta",
  "packet-beta",
  "radar-beta",
  "treemap-beta",
  "kanban",
  "C4Context",
  "C4Container",
  "C4Component",
  "C4Dynamic",
  "C4Deployment",
];

/** 코드 본문의 첫 줄이 mermaid 선언인가. 앞쪽 빈 줄과 mermaid 지시문(%%{...}%%)은 건너뛴다 */
export function looksLikeMermaid(code: string): boolean {
  const first = code
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith("%%"));
  if (!first) return false;
  return DECLARATIONS.some(
    (d) =>
      first === d ||
      // 선언 뒤에는 방향·제목·콜론이 붙는다(`flowchart LR`, `pie title X`, `gantt`).
      // 경계를 요구해 `graphql`, `piece` 같은 낱말이 걸리지 않게 한다.
      (first.startsWith(d) && /^[\s:]/.test(first.slice(d.length))),
  );
}
