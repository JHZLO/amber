// 마크다운 소스를 **절 단위**로 쪼개고, 한 조각을 원래 자리에 되끼우는 계산.
// AI 로 노트를 고칠 때 전문을 다시 받지 않기 위한 좌표계다 — 여기서 다루는 오프셋은 전부
// **소스 문자열 좌표**다(렌더된 텍스트 좌표가 아니다. 그쪽은 NoteComments 의 앵커가 쓴다).

import { headingSection } from "./mdSecRefs";

export interface MdSection {
  /** 번호 글머리 (`## 2-3. 제목` → "2-3"). 번호가 없는 제목이면 null */
  sec: string | null;
  /** 1 = `#`, 2 = `##`, 3 = `###` */
  level: number;
  /** 제목 줄에서 `#` 과 공백을 뗀 텍스트 (번호 포함) */
  title: string;
  /** 소스 문자열에서 이 절이 차지하는 구간 [start, end) — 제목 줄부터 다음 제목 직전까지 */
  start: number;
  end: number;
}

// 코드블록 안의 `#` 주석을 제목으로 착각하지 않으려면 펜스를 세어야 한다.
// (```bash 안의 `# 설치` 가 제목으로 잡히면 절 경계가 코드 한복판에서 갈린다)
// 마커의 **길이**와 정보 문자열까지 잡는다 — 아래 닫기 판정에 둘 다 필요하다.
const FENCE = /^(\s*)(`{3,}|~{3,})(.*)$/;
const HEADING = /^(#{1,3}) +(\S.*)$/;

/** 제목(`#`~`###`) 기준으로 절을 나눈다. 첫 제목 앞의 서문은 절이 아니라 포함되지 않는다 */
export function splitSections(md: string): MdSection[] {
  // 개행을 캡처해 함께 쪼갠다(홀수 인덱스 = 구분자). `split("\n")` 만 쓰면 CRLF 노트에서
  // 줄 끝에 `\r` 가 남아 HEADING 의 `$` 에 걸려 **절이 하나도 안 잡힌다**(외부 편집기에서 온
  // 노트가 그렇다). 오프셋은 소스 좌표여야 하므로 구분자의 실제 길이를 더한다.
  const parts = md.split(/(\r\n|\r|\n)/);
  const heads: { level: number; title: string; start: number }[] = [];
  let offset = 0;
  // 여는 펜스의 문자와 길이를 같이 들고 있어야 한다. CommonMark 에서 닫는 펜스는 여는 것보다
  // 짧을 수 없는데, 길이를 버리고 3개로 접으면 ````md 안의 ``` 가 바깥 펜스를 닫아버려
  // 절 경계가 코드블록 한복판으로 들어온다 — 그 절을 AI 로 고치면 닫는 펜스와 뒤 단락이 날아간다.
  let fence: { char: string; len: number } | null = null;
  for (let i = 0; i < parts.length; i += 2) {
    const line = parts[i] ?? "";
    const f = FENCE.exec(line);
    if (f) {
      const marker = f[2];
      if (!fence) {
        fence = { char: marker[0], len: marker.length };
      } else if (
        marker[0] === fence.char &&
        marker.length >= fence.len &&
        f[3].trim() === "" // 닫는 펜스에는 정보 문자열이 없다 (```bash 는 닫기가 아니다)
      ) {
        fence = null;
      }
    } else if (!fence) {
      const h = HEADING.exec(line);
      if (h) heads.push({ level: h[1].length, title: h[2].trim(), start: offset });
    }
    offset += line.length + (parts[i + 1]?.length ?? 0);
  }
  return heads.map((h, i) => ({
    sec: headingSection(h.title),
    level: h.level,
    title: h.title,
    start: h.start,
    // 마지막 절은 문서 끝까지. 다음 제목 시작이 곧 이 절의 끝이다(경계가 겹치지 않는다)
    end: i + 1 < heads.length ? heads[i + 1].start : md.length,
  }));
}

/** 고른 구간들을 **붙어 있는 것끼리 한 덩어리로** 묶는다.
 *  절을 여러 개 골랐을 때 이어진 절은 한 번에 고쳐야 이음새(앞 절을 되짚는 문장)가 어긋나지
 *  않고, 떨어진 묶음은 각각 따로 고쳐야 사이에 있는 절을 삼키지 않는다. */
export function mergeRuns<T extends { start: number; end: number }>(
  spans: readonly T[],
): { start: number; end: number; items: T[] }[] {
  const sorted = [...spans].sort((a, b) => a.start - b.start);
  const runs: { start: number; end: number; items: T[] }[] = [];
  for (const s of sorted) {
    const last = runs[runs.length - 1];
    // 절 경계는 겹치지 않고 맞닿아 있다(앞 절의 end === 뒤 절의 start) — 그래서 <= 로 붙인다
    if (last && s.start <= last.end) {
      last.end = Math.max(last.end, s.end);
      last.items.push(s);
    } else {
      runs.push({ start: s.start, end: s.end, items: [s] });
    }
  }
  return runs;
}

/** [start, end) 를 replacement 로 갈아끼운 새 소스.
 *  원래 조각의 앞뒤 공백(빈 줄)은 그대로 두고 안쪽만 바꾼다 — 모델이 끝 개행을 떨어뜨려도
 *  절 사이 빈 줄이 사라지지 않는다. */
export function spliceSpan(
  md: string,
  start: number,
  end: number,
  replacement: string,
): string {
  const span = md.slice(start, end);
  const lead = span.match(/^\s*/)?.[0] ?? "";
  const tail = span.match(/\s*$/)?.[0] ?? "";
  // 조각이 공백뿐이면 lead 와 tail 이 같은 문자를 두 번 세게 된다 — 그때는 껍데기를 버린다
  const envelope = lead.length + tail.length <= span.length;
  const body = replacement.trim();
  return (
    md.slice(0, start) +
    (envelope ? lead + body + tail : body) +
    md.slice(end)
  );
}
