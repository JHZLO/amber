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
const FENCE = /^(\s*)(```+|~~~+)/;
const HEADING = /^(#{1,3}) +(\S.*)$/;

/** 제목(`#`~`###`) 기준으로 절을 나눈다. 첫 제목 앞의 서문은 절이 아니라 포함되지 않는다 */
export function splitSections(md: string): MdSection[] {
  const lines = md.split("\n");
  const heads: { level: number; title: string; start: number }[] = [];
  let offset = 0;
  let fence: string | null = null;
  for (const line of lines) {
    const f = FENCE.exec(line);
    if (f) {
      const marker = f[2][0].repeat(3);
      if (!fence) fence = marker;
      else if (marker === fence) fence = null;
    } else if (!fence) {
      const h = HEADING.exec(line);
      if (h) heads.push({ level: h[1].length, title: h[2].trim(), start: offset });
    }
    offset += line.length + 1; // +1 = split 으로 사라진 개행
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
