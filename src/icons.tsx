// 얇은 라인 아이콘 세트 (Lucide 계열, 1.75 stroke, currentColor).
// 유니코드 글리프 대신 이걸 써서 톤을 통일한다.

import type { ReactNode } from "react";

export type IconName =
  | "chevron-left"
  | "chevron-right"
  | "chevron-down"
  | "chevron-up"
  | "plus"
  | "minus"
  | "check"
  | "pencil"
  | "trash"
  | "settings"
  | "sparkles"
  | "search"
  | "x"
  | "undo"
  | "expand"
  | "fit-screen"
  | "zoom-in"
  | "zoom-out"
  | "panel"
  | "folder"
  | "folder-open"
  | "folder-plus"
  | "file"
  | "file-plus"
  | "refresh"
  | "layers"
  | "book"
  | "sun"
  | "moon"
  | "workflow"
  | "message"
  | "user"
  | "calendar-check"
  | "copy"
  | "grip"
  | "clock"
  | "arrow-right"
  | "info"
  | "lightbulb"
  | "alert-triangle"
  | "alert-octagon"
  | "scissors"
  | "database"
  | "table"
  | "key"
  | "eye"
  | "eye-off"
  | "star";

const PATHS: Record<IconName, ReactNode> = {
  // 즐겨찾기 — 켜짐은 같은 획에 채움만 더한다(.icon-fill, DESIGN §3 채움/아웃라인)
  star: (
    <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
  ),
  // DB 연동 — 연결 폴더(database)·스키마 폴더(table)·키체인 표식(key). 폴더 아이콘과 나란히 서므로 획을 단순하게
  database: (
    <>
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
      <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
    </>
  ),
  table: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M3 10h18M9 4v16" />
    </>
  ),
  key: (
    <>
      <circle cx="7.5" cy="15.5" r="4.5" />
      <path d="m10.5 12.5 9-9M15 6l3 3M18 3l3 3" />
    </>
  ),
  // 비밀번호 보기/숨기기 — 입력 칸 오른쪽 끝의 토글. 한 쌍이라 뜻이 겹치지 않게 사선(off)으로 가른다
  eye: (
    <>
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  "eye-off": (
    <>
      <path d="M17.9 17.9A10.9 10.9 0 0 1 12 19c-6.5 0-10-7-10-7a19 19 0 0 1 4.1-5.1" />
      <path d="M9.9 4.2A10 10 0 0 1 12 5c6.5 0 10 7 10 7a19 19 0 0 1-2.3 3.3" />
      <path d="M9.9 9.9a3 3 0 1 0 4.2 4.2" />
      <path d="m2 2 20 20" />
    </>
  ),
  // 부분만 고쳐 쓰기 — 전문 재작성(sparkles)과 나란히 서므로 아이콘으로 갈라 준다
  scissors: (
    <>
      <circle cx="6" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M8.12 8.12L20 20" />
      <path d="M20 4L8.12 15.88" />
    </>
  ),
  "chevron-left": <path d="M15 18l-6-6 6-6" />,
  "chevron-right": <path d="M9 18l6-6-6-6" />,
  "chevron-down": <path d="M6 9l6 6 6-6" />,
  "chevron-up": <path d="M18 15l-6-6-6 6" />,
  plus: (
    <>
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </>
  ),
  minus: <path d="M5 12h14" />,
  check: <path d="M20 6 9 17l-5-5" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </>
  ),
  /* 오른쪽 서랍으로 보낸다 — 서랍 카드에서 돌아올 때의 화살표(←)와 방향으로 짝을 이룬다.
     나갈 땐 →, 돌아올 땐 ←. 상자(archive)도 아래로 내리는 화살표도 아닌 이유는 같다:
     치우거나 쌓아 두는 게 아니라 **옆으로 옮기는 것**이고, 그 옆이 어디인지가 화면에 있다. */
  "arrow-right": (
    <>
      <path d="M5 12h14" />
      <path d="m12 5 7 7-7 7" />
    </>
  ),
  pencil: <path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />,
  trash: (
    <>
      <path d="M3 6h18" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </>
  ),
  settings: (
    <>
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  sparkles: (
    <>
      <path d="M9.94 15.5A2 2 0 0 0 8.5 14.06l-6.14-1.58a.5.5 0 0 1 0-.96L8.5 9.94A2 2 0 0 0 9.94 8.5l1.58-6.14a.5.5 0 0 1 .96 0L14.06 8.5A2 2 0 0 0 15.5 9.94l6.14 1.58a.5.5 0 0 1 0 .96L15.5 14.06a2 2 0 0 0-1.44 1.44l-1.58 6.14a.5.5 0 0 1-.96 0z" />
      <path d="M20 3v4" />
      <path d="M22 5h-4" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </>
  ),
  x: (
    <>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </>
  ),
  undo: (
    <>
      <path d="M3 7v6h6" />
      <path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />
    </>
  ),
  expand: (
    <>
      <path d="M15 3h6v6" />
      <path d="M9 21H3v-6" />
      <path d="M21 3l-7 7" />
      <path d="M3 21l7-7" />
    </>
  ),
  // 네 귀퉁이 브래킷 = 화면에 맞춤(프레임 안에 담는다). 전체화면은 대각 화살표(expand).
  "fit-screen": (
    <>
      <path d="M3 8V5a2 2 0 0 1 2-2h3" />
      <path d="M16 3h3a2 2 0 0 1 2 2v3" />
      <path d="M21 16v3a2 2 0 0 1-2 2h-3" />
      <path d="M8 21H5a2 2 0 0 1-2-2v-3" />
    </>
  ),
  "zoom-in": (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.9-3.9" />
      <path d="M11 8v6" />
      <path d="M8 11h6" />
    </>
  ),
  "zoom-out": (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.9-3.9" />
      <path d="M8 11h6" />
    </>
  ),
  panel: (
    <>
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M15 3v18" />
    </>
  ),
  folder: (
    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
  ),
  "folder-open": (
    <path d="M6 14l1.45-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.55 6a2 2 0 0 1-1.94 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />
  ),
  "folder-plus": (
    <>
      <path d="M12 10v6" />
      <path d="M9 13h6" />
      <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
    </>
  ),
  file: (
    <>
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
      <path d="M14 2v4a2 2 0 0 0 2 2h4" />
      <path d="M16 13H8" />
      <path d="M16 17H8" />
    </>
  ),
  "file-plus": (
    <>
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
      <path d="M14 2v4a2 2 0 0 0 2 2h4" />
      <path d="M9 15h6" />
      <path d="M12 12v6" />
    </>
  ),
  refresh: (
    <>
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
    </>
  ),
  layers: (
    <>
      <path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" />
      <path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65" />
      <path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65" />
    </>
  ),
  book: (
    <>
      <path d="M12 7v14" />
      <path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2" />
      <path d="M12 20v2" />
      <path d="m4.93 4.93 1.41 1.41" />
      <path d="m17.66 17.66 1.41 1.41" />
      <path d="M2 12h2" />
      <path d="M20 12h2" />
      <path d="m6.34 17.66-1.41 1.41" />
      <path d="m19.07 4.93-1.41 1.41" />
    </>
  ),
  moon: <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />,
  workflow: (
    <>
      <rect x="3" y="3" width="8" height="8" rx="2" />
      <path d="M7 11v4a2 2 0 0 0 2 2h4" />
      <rect x="13" y="13" width="8" height="8" rx="2" />
    </>
  ),
  message: <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />,
  // 문답 스레드의 '나' 표식 — AI 쪽(sparkles)과 짝이다. 어깨선을 둥글게 닫아 24px 원 안에서
  // 아래가 잘린 것처럼 보이지 않게 했다
  user: (
    <>
      <circle cx="12" cy="8" r="3.6" />
      <path d="M5.4 20a6.6 6.6 0 0 1 13.2 0" />
    </>
  ),
  // 마크다운 알림(NOTE/TIP/WARNING…) 표식 — 색을 못 쓰는 판이라 아이콘이 종류를 나른다
  info: (
    <>
      <circle cx="12" cy="12" r="10" />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </>
  ),
  lightbulb: (
    <>
      <path d="M9 18h6" />
      <path d="M10 22h4" />
      <path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14" />
    </>
  ),
  "alert-octagon": (
    <>
      <path d="M7.86 2h8.28L22 7.86v8.28L16.14 22H7.86L2 16.14V7.86Z" />
      <path d="M12 8v4" />
      <path d="M12 16h.01" />
    </>
  ),
  "alert-triangle": (
    <>
      <path d="M21.73 18 13.73 4a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
    </>
  ),
  "calendar-check": (
    <>
      <path d="M8 2v4" />
      <path d="M16 2v4" />
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M3 10h18" />
      <path d="m9 16 2 2 4-4" />
    </>
  ),
  copy: (
    <>
      <rect x="9" y="9" width="13" height="13" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </>
  ),
  grip: (
    <>
      <circle cx="9" cy="6" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="9" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="9" cy="18" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="15" cy="6" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="15" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="15" cy="18" r="1.4" fill="currentColor" stroke="none" />
    </>
  ),
};

export function Icon({
  name,
  size = 16,
  className,
}: {
  name: IconName;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}

/** Amber 브랜드 마크 — **책상 스탠드와 호박빛**(앱 아이콘과 같은 그림에서 밤 타일만 뺀 것).
 *  타일도 테두리도 없다: 판을 깔고 테를 두르는 건 컨트롤의 문법이라 로고가 활성 탭처럼 읽힌다.
 *
 *  두 재질이 한 몸이다.
 *   스탠드 — 받침, 팔, 갓. 전부 currentColor 라 테마 토큰 하나(.rail-brand svg 의 color)만
 *          따라간다(갓의 반사만 흰색 — 빛은 테마를 안 탄다)
 *   빛 — 갓에서 떨어지는 빛줄기와 책상 위 웅덩이. 레일에서 **색이 있는 유일한 것**이라 로고가 로고로 읽힌다.
 *  좌표는 앱 아이콘과 같은 1024 판이다(빛줄기 끝은 갓 입구에서 계산한 값). */
export function AmberMark({
  size = 28,
  className,
}: {
  size?: number;
  className?: string;
}) {
  const shade =
    "M10 -45C61.2 -57.5 106.08 -92.88 136 -108L136 108C106.08 92.88 61.2 57.5 10 45Z";
  const arm = { strokeWidth: 38, strokeLinecap: "round", strokeLinejoin: "round" } as const;
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      role="img"
      aria-label="Amber"
    >
      <defs>
        <linearGradient id="amber-beam" gradientUnits="userSpaceOnUse" x1="470" y1="496" x2="412" y2="710">
          <stop offset="0" stopColor="#FFD47E" stopOpacity="0.9" />
          <stop offset="1" stopColor="#FFAE3D" stopOpacity="0.1" />
        </linearGradient>
        <filter id="amber-soft" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="8" />
        </filter>
      </defs>

      <g transform="translate(16 16) scale(0.064) translate(-496 -515)">
        <path d="M575 522L365 469L268 710L555 710Z" fill="url(#amber-beam)" filter="url(#amber-soft)" />
        <ellipse cx="412" cy="710" rx="132" ry="18" fill="#FFAE3D" filter="url(#amber-soft)" />
        <ellipse cx="412" cy="708" rx="80" ry="10" fill="#FFF6E0" />

        <g stroke="currentColor" fill="currentColor">
          <path d="M650 684L694 480" {...arm} />
          <path d="M694 480L506 352" {...arm} />
          <circle cx="694" cy="480" r="30" stroke="none" />
          <rect x="574" y="676" width="150" height="36" rx="18" stroke="none" />
          <g transform="translate(506 352) rotate(104)">
            <circle r="50" stroke="none" />
            <path d={shade} strokeWidth="24" strokeLinejoin="round" />
          </g>
        </g>
        <ellipse cx="452" cy="420" rx="30" ry="18" transform="rotate(-40 452 420)" fill="#fff" fillOpacity="0.45" />
        <ellipse cx="470" cy="496" rx="95" ry="14" transform="rotate(194 470 496)" fill="#FFF4D8" />
      </g>
    </svg>
  );
}
