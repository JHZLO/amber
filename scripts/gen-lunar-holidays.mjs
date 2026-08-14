#!/usr/bin/env node
// 음력 기반 한국 공휴일(설날·부처님오신날·추석)의 양력 날짜 표를 만든다 —
// src/lib/holidays.ts 의 LUNAR 상수를 갱신할 때 쓰는 일회성 생성기.
//
// 왜 표인가: Amber 는 완전 오프라인이라 공공데이터포털(KASI 특일 정보) API 를 쓸 수 없다.
// 인증키를 public 레포에 넣을 수도, 사용자마다 발급받게 할 수도 없고, 무엇보다 네트워크가
// 없으면 달력이 비어버린다. 음력 날짜는 천문학적으로 확정된 값이라 미리 박아도 안전하다.
//
// 데이터 출처: korean-lunar-calendar (MIT, usingsky) — 한국천문연구원 음양력 자료 기반.
// ICU 의 중국력(`ca-chinese`)은 쓰지 않는다: 기준 자오선이 달라(KST 135°E vs CST 120°E)
// 삭(朔) 시각이 날짜 경계를 넘는 해에 하루씩 어긋난다(예: 2023 부처님오신날 5/26 vs 실제 5/27).
//
// 사용법:
//   npx --yes korean-lunar-calendar@0.4.0 >/dev/null 2>&1   # (설치만 필요)
//   node scripts/gen-lunar-holidays.mjs [시작연도] [끝연도]
// 출력된 블록을 src/lib/holidays.ts 의 LUNAR 에 붙여넣는다.

import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

let KoreanLunarCalendar;
try {
  KoreanLunarCalendar = require("korean-lunar-calendar");
} catch {
  console.error(
    "korean-lunar-calendar 가 없습니다 — `pnpm add -D korean-lunar-calendar` 후 다시 실행하세요.",
  );
  process.exit(1);
}

const from = Number(process.argv[2]) || 2020;
const to = Number(process.argv[3]) || 2050;

const cal = new KoreanLunarCalendar();
const pad2 = (n) => String(n).padStart(2, "0");

/** 음력 (year, month, day) → 양력 'MM-DD' */
function solarOf(year, month, day) {
  if (!cal.setLunarDate(year, month, day, false))
    throw new Error(`변환 실패: 음력 ${year}-${month}-${day}`);
  const s = cal.getSolarCalendar();
  if (s.year !== year)
    throw new Error(`연도 이월: 음력 ${year}-${month}-${day} → ${s.year}`);
  return `${pad2(s.month)}-${pad2(s.day)}`;
}

const lines = [];
for (let y = from; y <= to; y++) {
  // [설날(1/1), 부처님오신날(4/8), 추석(8/15)]
  const row = [solarOf(y, 1, 1), solarOf(y, 4, 8), solarOf(y, 8, 15)];
  lines.push(`  ${y}: ["${row.join('", "')}"],`);
}

console.log(lines.join("\n"));
