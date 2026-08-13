// 2-pane 뷰(.body)의 좌측 pane 폭을 마우스로 조절하는 공용 훅 — 개념·필기노트·다이어그램·할 일 공용.
// 가로 비중은 사용자가 직접 정한다: 파일 트리는 깊은 경로를, 캘린더는 넓은 달을 원할 때가 다르다.
//
// - 폭은 뷰마다 따로 기억한다(localStorage). 트리에서 넓힌 게 캘린더까지 따라가면 안 된다.
// - `width` 는 '희망 폭'이고 실제 렌더 폭은 창 폭에 맞춰 clamp 한다 — 창을 좁히면 좌 pane 이
//   양보하고(우측 detailMin 보장), 다시 넓히면 원래 희망 폭으로 돌아온다.
// - 드래그는 포인터 기반(§8) — HTML5 DnD 는 WKWebView 에서 불안정하다. mousedown 에서
//   preventDefault 로 텍스트 선택을 막고, 커밋은 매 move 마다(폭 하나뿐이라 리렌더가 싸다).

import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type RefObject,
} from "react";
import { t } from "./i18n";

/** 좌 pane 기본 치수 — 뷰가 필요하면 개별로 덮어쓴다 */
const PANE_MIN = 240;
const PANE_MAX = 640;
const PANE_DEFAULT = 340;
/** 우측(상세)이 항상 확보할 최소 폭 — 창이 좁아지면 좌 pane 이 이만큼 양보한다 */
const DETAIL_MIN = 340;

export interface PaneResizeOptions {
  /** 폭을 기억할 localStorage 키 (뷰마다 다르게) */
  storageKey: string;
  /** 이 섹션이 화면에 떠 있는지 — 숨은 섹션은 폭을 재도 0 이라 측정하지 않는다 */
  active: boolean;
  min?: number;
  max?: number;
  defaultWidth?: number;
  detailMin?: number;
}

export interface PaneResize {
  /** 실제 렌더되는 좌 pane 폭 (창 폭에 맞춰 clamp 된 값) */
  width: number;
  /** `.body` 에 그대로 스프레드 */
  bodyProps: {
    ref: RefObject<HTMLDivElement | null>;
    style: CSSProperties;
  };
  /** `.list` 와 `.detail` **사이**에 렌더할 손잡이 — 같은 그리드 칸의 오른쪽 끝에 겹친다 */
  resizerProps: {
    className: string;
    role: "separator";
    "aria-orientation": "vertical";
    "aria-label": string;
    onMouseDown: (e: ReactMouseEvent) => void;
  };
}

export function usePaneResize({
  storageKey,
  active,
  min = PANE_MIN,
  max = PANE_MAX,
  defaultWidth = PANE_DEFAULT,
  detailMin = DETAIL_MIN,
}: PaneResizeOptions): PaneResize {
  const bodyRef = useRef<HTMLDivElement>(null);

  const [width, setWidth] = useState(() => {
    const s = Number(localStorage.getItem(storageKey));
    return s >= min && s <= max ? s : defaultWidth;
  });
  useEffect(() => {
    localStorage.setItem(storageKey, String(Math.round(width)));
  }, [storageKey, width]);

  // 반응형: .body 실측 폭을 추적해 희망 폭을 창에 맞춘다
  const [bodyW, setBodyW] = useState(0);
  useEffect(() => {
    if (!active) return;
    const measure = () => setBodyW(bodyRef.current?.clientWidth ?? 0);
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [active]);

  const effective = bodyW
    ? Math.max(min, Math.min(width, bodyW - detailMin))
    : width;

  function startResize(e: ReactMouseEvent) {
    e.preventDefault(); // 드래그가 글자를 선택해버리는 것을 막는다
    const rect = bodyRef.current?.getBoundingClientRect();
    if (!rect) return;
    // 잡은 지점을 존중한다(§8) — 손잡이 폭(10px) 안 어디를 잡았든 그 지점 기준으로 움직인다.
    // 커서 절대위치를 폭으로 쓰면 잡는 순간 경계가 최대 5px 튄다.
    const startX = e.clientX;
    const startW = effective;
    document.body.classList.add("resizing-col");
    const onMove = (ev: MouseEvent) => {
      const hi = Math.min(max, rect.width - detailMin);
      setWidth(Math.max(min, Math.min(hi, startW + (ev.clientX - startX))));
    };
    const onUp = () => {
      document.body.classList.remove("resizing-col");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  return {
    width: effective,
    bodyProps: {
      ref: bodyRef,
      style: { gridTemplateColumns: `${effective}px 1fr` },
    },
    resizerProps: {
      className: "pane-resizer",
      role: "separator",
      "aria-orientation": "vertical",
      "aria-label": t("common.resizePane"),
      onMouseDown: startResize,
    },
  };
}
