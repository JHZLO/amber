// 사전 정합성 — 타입(Record<keyof typeof ko, string>)이 잡지 못하는 것만 검사한다.
// 누락 키는 컴파일 에러라 여기서 다시 보지 않고, "값이 비었거나 번역을 잊은" 경우를 잡는다.

import { describe, expect, it } from "vitest";
import { commonMessages } from "./messages/common";
import { appMessages } from "./messages/app";
import { conceptsMessages } from "./messages/concepts";
import { notesMessages } from "./messages/notes";
import { diagramsMessages } from "./messages/diagrams";
import { todosMessages } from "./messages/todos";
import { reportMessages } from "./messages/report";
import { settingsMessages } from "./messages/settings";

const DOMAINS = {
  common: commonMessages,
  app: appMessages,
  concepts: conceptsMessages,
  notes: notesMessages,
  diagrams: diagramsMessages,
  todos: todosMessages,
  report: reportMessages,
  settings: settingsMessages,
} as const;

// 영어 사전에 한글이 남아 있으면 번역을 빼먹은 것. 자기 표기(언어 이름)는 컴포넌트 상수라 여기 없다.
const HANGUL = /[가-힣]/;
// 값 없이 비워두는 걸 허용하는 키 (상태 없음 표시 등)
const ALLOW_EMPTY = new Set(["report.mcpStatus.unknown"]);

describe("메시지 사전", () => {
  for (const [name, m] of Object.entries(DOMAINS)) {
    describe(name, () => {
      const ko = m.ko as Record<string, string>;
      const en = m.en as Record<string, string>;

      it("ko/en 키 집합이 같다", () => {
        expect(Object.keys(en).sort()).toEqual(Object.keys(ko).sort());
      });

      it("키가 도메인 접두어를 쓴다 — 합칠 때 다른 도메인을 덮어쓰지 않게", () => {
        const bad = Object.keys(ko).filter((k) => !k.startsWith(`${name}.`));
        expect(bad).toEqual([]);
      });

      it("빈 값이 없다", () => {
        const empty = Object.entries(ko)
          .concat(Object.entries(en))
          .filter(([k, v]) => !v.trim() && !ALLOW_EMPTY.has(k))
          .map(([k]) => k);
        expect(empty).toEqual([]);
      });

      it("en 에 한글이 남아 있지 않다", () => {
        const untranslated = Object.entries(en)
          .filter(([, v]) => HANGUL.test(v))
          .map(([k]) => k);
        expect(untranslated).toEqual([]);
      });

      it("{자리표시자} 가 ko/en 에서 일치한다 — 한쪽만 치환되면 화면에 중괄호가 노출된다", () => {
        const slots = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort();
        const mismatched = Object.keys(ko).filter(
          (k) => String(slots(ko[k])) !== String(slots(en[k])),
        );
        expect(mismatched).toEqual([]);
      });
    });
  }

  it("도메인 간 키 충돌이 없다", () => {
    const seen = new Set<string>();
    const dupes: string[] = [];
    for (const m of Object.values(DOMAINS)) {
      for (const k of Object.keys(m.ko)) {
        if (seen.has(k)) dupes.push(k);
        seen.add(k);
      }
    }
    expect(dupes).toEqual([]);
  });
});
