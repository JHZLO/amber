// UI 규약 지키기 — 소스를 훑어 손으로 되돌아간 자리를 잡는다(.claude/DESIGN.md §5).
// - 버튼은 <Button> 이다. `<button className="btn …">` 을 손으로 쓰면 variant/size 문법을 벗어난다
// - 오버레이(창, 메뉴)를 createPortal 로 손수 짜지 않는다 — Radix 레이어에 안 껴서 Esc 순서가 깨진다

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(__dirname);

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return tsxFiles(p);
    return p.endsWith(".tsx") ? [p] : [];
  });
}

const files = tsxFiles(SRC).filter((p) => !relative(SRC, p).startsWith(join("components", "ui")));

describe("UI 규약", () => {
  it("버튼 모양 클래스(btn, icon-btn)는 <Button> 으로만 낸다", () => {
    const raw = /<button\b[^>]*?className=(?:"|\{`)([^"`]*)/g;
    const hits = files.flatMap((f) =>
      [...readFileSync(f, "utf8").matchAll(raw)]
        // 클래스 토큰 단위로 본다 — `widget-btn`, `dgm-float-btn` 은 다른 물건이다
        .filter((m) => m[1].split(/\s+/).some((c) => c === "btn" || c === "icon-btn"))
        .map((m) => `${relative(SRC, f)}: ${m[0].replace(/\s+/g, " ").slice(0, 80)}`),
    );
    expect(hits).toEqual([]);
  });

  it("createPortal 은 정해진 자리에서만 쓴다", () => {
    // 오버레이가 아닌 portal 들: 드래그 따라다니는 복제본, 노트 질문 패널(형광펜 포함), 문서 찾기 막대
    const allowed = new Set([
      "ui.tsx",
      join("components", "NoteComments.tsx"),
      join("components", "PageFind.tsx"),
    ]);
    const hits = files
      .filter((f) => /createPortal\(/.test(readFileSync(f, "utf8")))
      .map((f) => relative(SRC, f))
      .filter((f) => !allowed.has(f));
    expect(hits).toEqual([]);
  });
});
