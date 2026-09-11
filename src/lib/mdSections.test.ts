import { describe, expect, it } from "vitest";
import { mergeRuns, splitSections, spliceSpan } from "./mdSections";

const DOC = [
  "들어가는 문단.",
  "",
  "# 1. 첫 절",
  "본문 하나.",
  "",
  "## 1-1. 하위 절",
  "본문 둘.",
  "",
  "# 2. 둘째 절",
  "끝.",
].join("\n");

describe("splitSections", () => {
  it("제목마다 절을 만들고 번호를 뽑아낸다", () => {
    const s = splitSections(DOC);
    expect(s.map((x) => [x.sec, x.level])).toEqual([
      ["1", 1],
      ["1-1", 2],
      ["2", 1],
    ]);
  });

  it("구간이 제목 줄부터 다음 제목 직전까지다 — 겹치지도, 비지도 않는다", () => {
    const s = splitSections(DOC);
    expect(DOC.slice(s[0].start, s[0].end)).toBe("# 1. 첫 절\n본문 하나.\n\n");
    expect(DOC.slice(s[1].start, s[1].end)).toBe("## 1-1. 하위 절\n본문 둘.\n\n");
    expect(DOC.slice(s[2].start, s[2].end)).toBe("# 2. 둘째 절\n끝.");
    expect(s[0].end).toBe(s[1].start);
  });

  it("첫 제목 앞 서문은 절이 아니다", () => {
    expect(splitSections(DOC)[0].start).toBe(DOC.indexOf("# 1."));
  });

  it("코드블록 안의 # 는 제목이 아니다", () => {
    const md = "# 1. 절\n\n```bash\n# 설치\nbrew install x\n```\n\n## 1-1. 뒤 절\n";
    expect(splitSections(md).map((s) => s.title)).toEqual(["1. 절", "1-1. 뒤 절"]);
  });

  it("#### 이하와 제목 아닌 # 는 무시한다", () => {
    const md = "# 1. 절\n#### 너무 깊음\n#태그아님\n";
    expect(splitSections(md)).toHaveLength(1);
  });
});

describe("spliceSpan", () => {
  it("구간만 갈아끼운다", () => {
    const s = splitSections(DOC);
    const out = spliceSpan(DOC, s[1].start, s[1].end, "## 1-1. 하위 절\n고친 본문.");
    expect(out).toContain("## 1-1. 하위 절\n고친 본문.");
    expect(out).toContain("# 2. 둘째 절"); // 뒤 절은 그대로
    expect(out).toContain("본문 하나."); // 앞 절도 그대로
  });

  it("원래 조각의 앞뒤 빈 줄을 지킨다 — 모델이 끝 개행을 떨어뜨려도", () => {
    const md = "A\n\n중간\n\nB";
    expect(spliceSpan(md, 3, 7, "고침")).toBe("A\n\n고침\n\nB");
  });

  it("공백뿐인 구간이면 껍데기를 버리고 그냥 치환한다 — 개행을 두 번 세지 않게", () => {
    // 실제 진입 경로는 빈 조각을 막지만, 앞뒤 공백을 지키는 계산이 자기 자신을 두 번
    // 세면 개행이 늘어난다. 그 경우엔 평범한 치환으로 떨어진다.
    expect(spliceSpan("A\n\n\nB", 1, 3, "X")).toBe("AX\nB");
  });
});

describe("mergeRuns", () => {
  it("맞닿은 구간은 한 덩어리로 묶는다", () => {
    const runs = mergeRuns([
      { start: 0, end: 10 },
      { start: 10, end: 25 },
      { start: 25, end: 30 },
    ]);
    expect(runs).toHaveLength(1);
    expect([runs[0].start, runs[0].end]).toEqual([0, 30]);
    expect(runs[0].items).toHaveLength(3);
  });

  it("떨어진 구간은 따로 남긴다 — 사이에 있는 절을 삼키지 않게", () => {
    const runs = mergeRuns([
      { start: 0, end: 10 },
      { start: 40, end: 50 },
    ]);
    expect(runs.map((r) => [r.start, r.end])).toEqual([
      [0, 10],
      [40, 50],
    ]);
  });

  it("순서가 뒤섞여 들어와도 정렬해서 묶는다", () => {
    const runs = mergeRuns([
      { start: 25, end: 30 },
      { start: 0, end: 10 },
      { start: 10, end: 25 },
    ]);
    expect(runs).toHaveLength(1);
  });

  it("빈 입력은 빈 결과", () => {
    expect(mergeRuns([])).toEqual([]);
  });
});

describe("splitSections — 펜스 경계", () => {
  // 마크다운 예시를 싣는 노트의 흔한 형태: ````md 안에 ```bash 가 들어간다.
  // 여는 펜스의 길이를 버리고 3개로 접으면 안쪽 ``` 가 바깥을 닫아버려, 코드블록 **안**의
  // `# ...` 줄이 절로 잡힌다. 그 절을 AI 로 고치면 닫는 펜스와 뒤 단락이 함께 사라진다.
  const NESTED = [
    "# 1. 설치 가이드",
    "",
    "````md",
    "```bash",
    "# 설치 명령",
    "brew install amber",
    "```",
    "````",
    "",
    "본문 끝.",
    "",
    "# 2. 다음 절",
    "내용",
    "",
  ].join("\n");

  it("중첩 펜스 안의 # 는 절이 아니다", () => {
    const secs = splitSections(NESTED);
    expect(secs.map((s) => s.title)).toEqual(["1. 설치 가이드", "2. 다음 절"]);
  });

  it("첫 절의 끝이 코드블록을 통째로 품는다", () => {
    const [first] = splitSections(NESTED);
    const span = NESTED.slice(first.start, first.end);
    expect(span).toContain("brew install amber");
    expect(span).toContain("````"); // 닫는 펜스까지
    expect(span).toContain("본문 끝.");
  });

  it("정보 문자열이 붙은 같은 길이 펜스는 닫기가 아니다", () => {
    const md = ["# A", "```", "# 코드 주석", "```js", "# 또 다른 주석", "```", "# B"].join("\n");
    // 여는 ``` → `# 코드 주석` 은 코드. ```js 는 정보 문자열이 있어 닫기가 아니다.
    // 마지막 ``` 가 닫고, 그 뒤 `# B` 만 절이 된다.
    expect(splitSections(md).map((s) => s.title)).toEqual(["A", "B"]);
  });

  it("~~~ 와 ``` 는 서로를 닫지 않는다", () => {
    const md = ["# A", "~~~", "```", "# 안쪽", "```", "~~~", "# B"].join("\n");
    expect(splitSections(md).map((s) => s.title)).toEqual(["A", "B"]);
  });
});

describe("splitSections — 줄바꿈 형식", () => {
  it("CRLF 노트도 절이 잡힌다", () => {
    // 외부 편집기(Windows·일부 동기화 도구)에서 온 노트. \r 가 남으면 HEADING 의 $ 에 막혀
    // 절이 0개가 되고 '절 골라 고치기' 목록이 오류 없이 텅 빈다.
    const md = "# 1. 제목\r\n\r\n본문\r\n\r\n# 2. 둘째\r\n내용\r\n";
    const secs = splitSections(md);
    expect(secs.map((s) => s.title)).toEqual(["1. 제목", "2. 둘째"]);
  });

  it("CRLF 에서도 오프셋이 소스 좌표다", () => {
    const md = "# 1. 제목\r\n\r\n본문\r\n\r\n# 2. 둘째\r\n내용\r\n";
    const secs = splitSections(md);
    // start/end 로 잘라낸 조각이 실제 그 절이어야 되끼우기(spliceSpan)가 맞는다
    expect(md.slice(secs[0].start, secs[0].end)).toBe("# 1. 제목\r\n\r\n본문\r\n\r\n");
    expect(md.slice(secs[1].start, secs[1].end)).toBe("# 2. 둘째\r\n내용\r\n");
  });

  it("CR 단독 줄바꿈(구 Mac)도 센다", () => {
    const secs = splitSections("# A\r본문\r# B\r");
    expect(secs.map((s) => s.title)).toEqual(["A", "B"]);
  });
});
