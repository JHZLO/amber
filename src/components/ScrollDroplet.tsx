// 스크롤 포인터 — 오른쪽 가장자리를 따라 내려가는 **유리 알약**.
//
// 왜 만들었나: 네이티브 스크롤바는 전역으로 꺼져 있다(styles.css, DESIGN §2). 이 WebView 에는
// 겹침 스크롤바가 없어서 얇게 칠해도 11px 짜리 **골**이 남고, 그 골이 옆 패널 경계선과 나란히
// 서면 막대가 상자에 갇힌 것처럼 보였다. 그래서 막대를 끄고 위치 표시만 우리가 그린다.
//
// **한 벌만 둔다.** App 에 하나 얹어 두고 document 의 scroll 을 캡처 단계에서 받는다 —
// 어느 패널이든, 앞으로 생길 패널이든 배선 없이 같은 표시를 받는다.
//
// 길이는 네이티브 엄지와 같은 규칙이다: **보이는 만큼의 비율**. 글이 길수록 짧아져서
// 얼마나 남았는지가 길이로 읽힌다. 위치만 알리는 점이었다면 이 정보가 사라진다.
//
// **색을 쓰지 않는다.** 뒤를 흐려서 비치는 유리다(iOS 의 liquid glass) — 어떤 판 위에 놓이든
// 그 판의 색을 빌리므로 라이트/다크 분기도, 브랜드색도 필요 없다. 표식 하나에 색을 주면
// 거의 무채색인 화면에서 그것만 계속 튄다.
//
// 양 끝이 다 둥근 알약이다. 물방울처럼 위를 뾰족하게 깎아 봤더니 9px 폭에서는 바늘처럼
// 날카로워서, 위치를 알리는 표식이 아니라 찌르는 물건으로 읽혔다.
//
// 잡아서 끌 수는 없다. 구르는 동안에만 떠 있다가 사라지는 **표식**이지 손잡이가 아니다 —
// 늘 떠 있어야 잡을 수 있는데, 늘 떠 있는 것은 글 옆에서 계속 시선을 가져간다.

import { useEffect, useRef, useState } from "react";

/** 마지막 스크롤 뒤 이만큼 지나면 사라진다. 한 번 굴리고 읽는 동안은 남아 있을 만큼 */
const FADE_AFTER_MS = 900;
/** 알약 폭(px) */
const W = 9;
/** 아무리 긴 글이어도 이보다 짧아지지 않는다 — 더 줄면 알약이 아니라 점이 된다 */
const MIN_H = 28;
/** 가장자리에서 띄우는 거리 */
const INSET = 6;

export interface Drop {
  /** 화면 좌표 (fixed 레이어라 그대로 쓴다) */
  left: number;
  top: number;
  height: number;
}

/** 스크롤 진행도(0~1) — 끝까지 갔으면 1. 스크롤할 게 없으면 null */
export function progressOf(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
): number | null {
  const span = scrollHeight - clientHeight;
  if (span < 4) return null; // 한두 픽셀 넘치는 건 스크롤이 아니다
  return Math.min(1, Math.max(0, scrollTop / span));
}

/** 엄지 길이 — 트랙에서 **보이는 만큼의 비율**. 네이티브 스크롤바와 같은 규칙이다 */
export function thumbLength(
  track: number,
  clientHeight: number,
  scrollHeight: number,
): number {
  const ratio = scrollHeight > 0 ? clientHeight / scrollHeight : 1;
  return Math.max(MIN_H, Math.min(track, Math.round(track * ratio)));
}

export function ScrollDroplet() {
  const [drop, setDrop] = useState<Drop | null>(null);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    const hide = () => setDrop(null);
    const onScroll = (e: Event) => {
      const el = e.target;
      // document 스크롤(창 전체)은 amber 에 없다 — 패널 안에서만 구른다
      if (!(el instanceof HTMLElement)) return;
      const p = progressOf(el.scrollTop, el.scrollHeight, el.clientHeight);
      if (p === null) return;
      const rect = el.getBoundingClientRect();
      const track = Math.max(MIN_H, rect.height - INSET * 2);
      const height = thumbLength(track, el.clientHeight, el.scrollHeight);
      setDrop({
        left: rect.right - W - INSET,
        top: rect.top + INSET + (track - height) * p,
        height,
      });
      if (timer.current) clearTimeout(timer.current);
      timer.current = window.setTimeout(hide, FADE_AFTER_MS);
    };
    // 캡처 단계 — scroll 은 버블링하지 않으므로 이렇게 받아야 모든 컨테이너가 잡힌다
    document.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("scroll", onScroll, true);
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  if (!drop) return null;
  return (
    <div
      className="scroll-droplet"
      style={{ left: drop.left, top: drop.top, width: W, height: drop.height }}
      aria-hidden="true"
    />
  );
}
