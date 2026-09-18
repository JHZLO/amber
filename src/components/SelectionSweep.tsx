// 인라인 질문을 보내고 답을 기다리는 동안, **드래그한 그 자리**에 형광펜이 지나가게 한다.
//
// 왜 여기에 따로 있나: 기다림을 말하는 표시가 말풍선 안에만 있으면, 내가 물어본 게 본문의
// 어느 문장이었는지는 말해 주지 않는다. 답이 붙을 자리에서 미리 움직이면 "이 문장을 읽고 있다"로
// 읽힌다 — 진행 바가 말하지 못하는 것이다.
//
// 왜 ::highlight() 가 아닌가: CSS Custom Highlight API 는 color·background-color 정도만 받아
// 그라데이션도 애니메이션도 얹을 수 없다. 그래서 Range 의 클라이언트 사각형을 떠서 그 위에
// 고정 레이어를 깐다. DOM 은 건드리지 않는다(마크다운 렌더 결과에 흔적이 남지 않게).

import { useEffect, useState } from "react";
import { mergeLineRects, nthIndex, rangeAt, type Rect } from "../lib/noteAnchor";

export function SelectionSweep({
  containerRef,
  anchor,
  occurrence,
  active,
}: {
  containerRef: React.RefObject<HTMLElement | null>;
  /** 질문을 건 본문 조각 */
  anchor: string;
  occurrence: number;
  /** 답을 기다리는 중인가 */
  active: boolean;
}) {
  const [boxes, setBoxes] = useState<Rect[]>([]);

  useEffect(() => {
    if (!active) {
      setBoxes([]);
      return;
    }
    let raf = 0;
    const measure = () => {
      raf = 0;
      const c = containerRef.current;
      if (!c) return setBoxes([]);
      const full = c.textContent ?? "";
      let idx = nthIndex(full, anchor, occurrence);
      if (idx === -1) idx = nthIndex(full, anchor, 0); // 본문이 바뀌었으면 첫 출현으로
      const r = idx === -1 ? null : rangeAt(c, idx, anchor.length);
      if (!r) return setBoxes([]);
      // 줄마다 하나로 합쳐서 넘긴다 — 굵은 글씨·코드가 낀 줄은 getClientRects 가 조각을 여럿 주는데,
      // 그대로 그리면 조각마다 빛이 따로 돌아 경계에서 띠가 끊긴다
      setBoxes(mergeLineRects([...r.getClientRects()]));
    };
    const soon = () => {
      if (!raf) raf = requestAnimationFrame(measure);
    };
    measure();
    // 스크롤·리사이즈로 글자가 움직이면 형광펜도 따라간다 (fixed 레이어라 좌표가 어긋난다)
    window.addEventListener("scroll", soon, true);
    window.addEventListener("resize", soon);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      window.removeEventListener("scroll", soon, true);
      window.removeEventListener("resize", soon);
    };
  }, [active, anchor, occurrence, containerRef]);

  if (!active || boxes.length === 0) return null;
  return (
    <div className="sel-sweep" aria-hidden="true">
      {boxes.map((b, i) => (
        <span
          key={i}
          className="sel-sweep-line"
          style={
            { top: b.top, left: b.left, width: b.width, height: b.height, "--i": i } as React.CSSProperties
          }
        >
          <i />
        </span>
      ))}
    </div>
  );
}
