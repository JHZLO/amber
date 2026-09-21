// 스크롤 포인터 — amber 의 시그니처 물방울이 오른쪽 가장자리를 따라 내려간다.
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
// 잡아서 끌 수는 없다. 구르는 동안에만 떠 있다가 사라지는 **표식**이지 손잡이가 아니다 —
// 늘 떠 있어야 잡을 수 있는데, 늘 떠 있는 물방울은 글 옆에서 계속 시선을 가져간다.

import { useEffect, useRef, useState } from "react";

/** 마지막 스크롤 뒤 이만큼 지나면 사라진다. 한 번 굴리고 읽는 동안은 남아 있을 만큼 */
const FADE_AFTER_MS = 900;
/** 물방울 폭(px) */
const W = 9;
/** 아무리 긴 글이어도 이보다 짧아지지 않는다 — 더 줄면 방울이 아니라 점이 된다 */
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

/** 위 아래로 늘어나는 물방울 — 위는 뾰족하고 아래는 둥글다(브랜드 마크와 같은 방향).
 *  길이가 바뀌어도 머리와 꼬리의 생김새는 그대로 두고 **가운데만 늘린다**. */
export function dropletPath(w: number, h: number): string {
  const r = w / 2;
  const belly = Math.min(h * 0.42, h - r); // 옆구리가 가장 부푸는 높이
  return [
    `M ${r} 0`,
    `C ${w * 0.04} ${belly * 0.8}, 0 ${h - r * 1.7}, 0 ${h - r}`,
    `A ${r} ${r} 0 0 0 ${w} ${h - r}`,
    `C ${w} ${h - r * 1.7}, ${w * 0.96} ${belly * 0.8}, ${r} 0`,
    "Z",
  ].join(" ");
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
      style={{ left: drop.left, top: drop.top }}
      aria-hidden="true"
    >
      <svg width={W} height={drop.height} viewBox={`0 0 ${W} ${drop.height}`} fill="none">
        <path d={dropletPath(W, drop.height)} fill="url(#amber-drop)" />
        <defs>
          {/* 길이를 따라 흐르게 — 머리가 밝고 꼬리가 깊다(브랜드 마크의 빛 방향) */}
          <linearGradient id="amber-drop" x1="0" y1="0" x2="0.35" y2="1">
            <stop offset="0" stopColor="var(--brand-lit)" />
            <stop offset=".45" stopColor="var(--brand)" />
            <stop offset="1" stopColor="var(--brand-deep)" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}
