// 좌 원문 / 우 라이브 프리뷰 2분할(마크다운 편집)에서 두 칸의 스크롤을 함께 움직인다.
// 한쪽을 내리면 다른 쪽도 같은 비율로 따라간다 — 손으로 굴리는 칸이 항상 주인이 되고,
// 그 반작용으로 되돌아오는 스크롤은 무시한다(잠금). 양방향이라 어느 쪽을 굴려도 된다.
//
// **비율 대응**이지 줄 대응이 아니다: 표·코드블록처럼 원문과 렌더 높이가 크게 다른 블록에서는
// 몇 줄씩 어긋난다. 줄 단위로 맞추려면 원문 줄 ↔ 렌더된 블록의 지도를 들고 있어야 하는데,
// 편집 중에는 그 지도가 매 입력마다 무효가 된다 — 위치를 대충 맞추는 값이 더 안 흔들린다.

import { useEffect, type RefObject } from "react";

/** src 의 스크롤 위치를 dst 의 스크롤 범위로 옮긴 값. 어느 쪽이든 못 굴리면 null */
export function mappedScrollTop(
  srcTop: number,
  srcMax: number,
  dstMax: number,
): number | null {
  if (srcMax <= 0 || dstMax <= 0) return null;
  const ratio = Math.min(1, Math.max(0, srcTop / srcMax));
  return ratio * dstMax;
}

const RELEASE_MS = 120; // 반작용 스크롤이 잦아들 시간 — 이 뒤엔 다시 어느 쪽이든 주인이 될 수 있다

export function useScrollSync(
  aRef: RefObject<HTMLElement | null>,
  bRef: RefObject<HTMLElement | null>,
  enabled: boolean,
) {
  useEffect(() => {
    const a = aRef.current;
    const b = bRef.current;
    if (!enabled || !a || !b) return;

    let owner: HTMLElement | null = null;
    let release: ReturnType<typeof setTimeout> | null = null;
    let raf = 0;

    const follow = (src: HTMLElement, dst: HTMLElement) => {
      raf = 0;
      const top = mappedScrollTop(
        src.scrollTop,
        src.scrollHeight - src.clientHeight,
        dst.scrollHeight - dst.clientHeight,
      );
      if (top !== null) dst.scrollTop = top;
    };

    const onScroll = (src: HTMLElement, dst: HTMLElement) => () => {
      // 주인이 아닌 칸의 이벤트 = 방금 우리가 옮긴 반작용이다. 여기서 되받으면 서로 밀며 떤다.
      if (owner && owner !== src) return;
      owner = src;
      if (release) clearTimeout(release);
      release = setTimeout(() => {
        owner = null;
      }, RELEASE_MS);
      if (!raf) raf = requestAnimationFrame(() => follow(src, dst));
    };

    const onA = onScroll(a, b);
    const onB = onScroll(b, a);
    a.addEventListener("scroll", onA, { passive: true });
    b.addEventListener("scroll", onB, { passive: true });
    return () => {
      a.removeEventListener("scroll", onA);
      b.removeEventListener("scroll", onB);
      if (release) clearTimeout(release);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [aRef, bRef, enabled]);
}
