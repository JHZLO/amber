// 앱 내 섹션 간 이동(같은 창) — 노트↔개념 상호 링크용 경량 이벤트 버스.
// 위젯 등 다른 창에서 오는 것은 App 이 Tauri listen("open-concept") 으로 따로 처리.

export const OPEN_CONCEPT = "amber:open-concept";
export const OPEN_NOTE = "amber:open-note";
export const OPEN_DIAGRAM = "amber:open-diagram";

/** 개념 열기 (id) — App 이 개념 섹션으로 전환 후 선택 */
export function openConceptInApp(id: number) {
  window.dispatchEvent(new CustomEvent(OPEN_CONCEPT, { detail: { id } }));
}

/** 노트 열기 (notes 루트 기준 상대경로) — App 이 필기노트 섹션으로 전환 후 연다 */
export function openNoteInApp(path: string) {
  window.dispatchEvent(new CustomEvent(OPEN_NOTE, { detail: { path } }));
}

/** 다이어그램 열기 (diagrams 루트 기준 상대경로) — OPEN_NOTE 짝. DiagramsView 가 파일을 연다 */
export function openDiagramInApp(path: string) {
  window.dispatchEvent(new CustomEvent(OPEN_DIAGRAM, { detail: { path } }));
}
