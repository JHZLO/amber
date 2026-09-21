// 설정의 MCP 서버 선택 — 저장 형식과 예전 두 칸(slack, notion)에서의 이관.
// 조용히 비워지면 리포트 소스가 하루아침에 사라진 것처럼 보이므로, 이관 경로를 고정해 둔다.

import { describe, expect, it } from "vitest";
import { mcpSourcesFrom, parseMcpPicked, type ReportConfig } from "./report";

describe("parseMcpPicked", () => {
  it("저장된 배열을 그대로 읽는다", () => {
    expect(parseMcpPicked('["plugin:slack:slack","Gmail"]', null, null)).toEqual([
      "plugin:slack:slack",
      "Gmail",
    ]);
  });

  it("키가 없으면 예전 두 칸에서 옮겨 온다", () => {
    expect(parseMcpPicked(null, "plugin:slack:slack", "plugin:Notion:notion")).toEqual([
      "plugin:slack:slack",
      "plugin:Notion:notion",
    ]);
  });

  it("예전 칸이 비어 있으면 빈 목록", () => {
    expect(parseMcpPicked(null, "", null)).toEqual([]);
  });

  it("형식이 깨졌으면 이관 경로로 떨어진다 — 예외로 설정 로드를 깨지 않는다", () => {
    expect(parseMcpPicked("{oops", "plugin:slack:slack", null)).toEqual(["plugin:slack:slack"]);
  });

  it("중복과 공백은 걸러 낸다", () => {
    expect(parseMcpPicked('["a","a","  ",""]', null, null)).toEqual(["a"]);
  });

  it("한 번 저장한 빈 배열은 이관으로 되돌리지 않는다 — 사용자가 끈 것이다", () => {
    expect(parseMcpPicked("[]", "plugin:slack:slack", null)).toEqual([]);
  });
});

describe("mcpSourcesFrom", () => {
  const cfg = (mcpPicked: string[], enabled = 2): ReportConfig => ({
    onboarded: true,
    sources: [
      { id: "github", enabled: enabled > 0 },
      { id: "ai_sessions", enabled: enabled > 1 },
    ],
    githubPath: "",
    githubRepos: [],
    githubAccount: "",
    sessionsClaude: true,
    sessionsCodex: true,
    mcpPicked,
    displayName: "",
    context: "",
  });

  it("rank 는 내장 소스 뒤에서 이어진다", () => {
    expect(mcpSourcesFrom(cfg(["Gmail", "plugin:Notion:notion"]))).toEqual([
      { id: "Gmail", rank: 3, server: "Gmail" },
      { id: "plugin:Notion:notion", rank: 4, server: "plugin:Notion:notion" },
    ]);
  });

  it("내장 소스를 끄면 그만큼 앞으로 당겨진다", () => {
    expect(mcpSourcesFrom(cfg(["Gmail"], 0))[0].rank).toBe(1);
  });

  it("고른 서버가 없으면 빈 배열 — MCP 인자 자체가 안 붙는다", () => {
    expect(mcpSourcesFrom(cfg([]))).toEqual([]);
  });
});
