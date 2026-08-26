// 알림 블록 변환 회귀 테스트. 마커가 본문에 새는 것·일반 인용구가 콜아웃으로 잡히는 것
// 둘 다 화면에서 바로 드러나는 사고라 데이터로 고정한다.

import { describe, expect, it } from "vitest";
import { ALERT_KINDS, matchAlertMarker, remarkAlerts } from "./mdAlerts";

type MdNode = {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, string> };
};

const text = (value: string): MdNode => ({ type: "text", value });
const para = (...children: MdNode[]): MdNode => ({ type: "paragraph", children });
const quote = (...children: MdNode[]): MdNode => ({ type: "blockquote", children });
const root = (...children: MdNode[]): MdNode => ({ type: "root", children });

const run = (tree: MdNode) => {
  remarkAlerts()(tree);
  return tree;
};

describe("matchAlertMarker", () => {
  it("지원하는 다섯 종류를 모두 알아본다", () => {
    for (const kind of ALERT_KINDS) {
      expect(matchAlertMarker(`[!${kind}]\n본문`)?.kind).toBe(kind);
    }
  });

  it("마커만 있고 본문이 없어도 인정한다", () => {
    expect(matchAlertMarker("[!NOTE]")?.kind).toBe("NOTE");
  });

  it("같은 줄에 본문이 이어지면 알림이 아니다 — Paper 렌더러와 같은 판정", () => {
    expect(matchAlertMarker("[!NOTE] 같은 줄 본문")).toBeNull();
  });

  it("모르는 종류·소문자·앞에 글자가 붙은 것은 알림이 아니다", () => {
    expect(matchAlertMarker("[!UNKNOWN]\n본문")).toBeNull();
    expect(matchAlertMarker("[!note]\n본문")).toBeNull();
    expect(matchAlertMarker("주의 [!NOTE]\n본문")).toBeNull();
  });
});

describe("remarkAlerts", () => {
  it("마커를 떼고 data-alert 를 남긴다", () => {
    const tree = run(root(quote(para(text("[!WARNING]\n조심하세요")))));
    const bq = tree.children![0];
    expect(bq.data?.hProperties?.["data-alert"]).toBe("WARNING");
    // 마커가 본문에 남으면 화면에 "[!WARNING]" 이 글자로 보인다
    expect(bq.children![0].children![0].value).toBe("조심하세요");
  });

  it("마커만 있던 문단은 걷어낸다 — 빈 <p> 가 남지 않게", () => {
    const tree = run(root(quote(para(text("[!NOTE]")), para(text("다음 문단")))));
    const bq = tree.children![0];
    expect(bq.children).toHaveLength(1);
    expect(bq.children![0].children![0].value).toBe("다음 문단");
  });

  it("일반 인용구는 손대지 않는다", () => {
    const tree = run(root(quote(para(text("그냥 인용문")))));
    const bq = tree.children![0];
    expect(bq.data?.hProperties).toBeUndefined();
    expect(bq.children![0].children![0].value).toBe("그냥 인용문");
  });

  it("중첩된 인용구도 찾는다 (목록 안 등)", () => {
    const tree = run(
      root({
        type: "listItem",
        children: [quote(para(text("[!TIP]\n안쪽")))],
      }),
    );
    const bq = tree.children![0].children![0];
    expect(bq.data?.hProperties?.["data-alert"]).toBe("TIP");
  });

  it("굵게 같은 인라인이 먼저 오면 알림이 아니다", () => {
    const tree = run(
      root(quote(para({ type: "strong", children: [text("[!NOTE]")] }))),
    );
    expect(tree.children![0].data?.hProperties).toBeUndefined();
  });
});
