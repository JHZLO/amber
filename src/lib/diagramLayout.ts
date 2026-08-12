// 다이어그램 레이아웃 엔진 선택 (ELK ↔ Dagre). 앱 전역 설정 — 캔버스에서 고르면
// 노트 안 다이어그램까지 같은 엔진으로 그려진다(같은 그림이 화면마다 달라 보이면 헷갈린다).
//
// 왜 고를 수 있어야 하나: 두 엔진은 배치 철학이 다르다.
//   ELK   = 직교 배선 + 자기참조(self FK)를 엔티티에 붙는 짧은 루프로. 큰 ERD 에서 유리.
//   Dagre = mermaid 기본. 곡선 배선이라 작은 흐름도는 더 부드럽게 읽힌다. 다만 ER
//           자기참조는 3토막 난 선으로 흩어진다(mermaid 의 자기루프 병합에 er 이 빠져 있음).
//
// 컴포넌트 밖 모듈 스토어인 이유: mermaid 는 싱글턴이라 설정도 전역이다. 구독은
// useDiagramLayout(), 렌더러(Mermaid.tsx)는 getDiagramLayout() 으로 현재 값만 읽는다.

import { useSyncExternalStore } from "react";

export type DiagramLayout = "elk" | "dagre";

export const DIAGRAM_LAYOUTS: DiagramLayout[] = ["elk", "dagre"];

const KEY = "amber.diagram.layout";

function read(): DiagramLayout {
  try {
    return localStorage.getItem(KEY) === "dagre" ? "dagre" : "elk";
  } catch {
    return "elk"; // localStorage 없는 환경(테스트 등) — 기본값으로
  }
}

let current: DiagramLayout = read();
const listeners = new Set<() => void>();

export function getDiagramLayout(): DiagramLayout {
  return current;
}

export function setDiagramLayout(next: DiagramLayout): void {
  if (next === current) return;
  current = next;
  try {
    localStorage.setItem(KEY, next);
  } catch {
    /* 저장 실패해도 이번 세션 동안은 적용된다 */
  }
  listeners.forEach((l) => l());
}

export function useDiagramLayout(): DiagramLayout {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => current,
  );
}
