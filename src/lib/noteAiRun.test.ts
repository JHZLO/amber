import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppConfig } from "./config";
import type { InvocationMeta } from "./ai";

const mocks = vi.hoisted(() => ({
  compose: vi.fn(),
  editSpan: vi.fn(),
  cancel: vi.fn(async (_k: string) => {}),
  keyN: 0,
}));
vi.mock("./ai", () => ({
  aiNoteComposeStream: mocks.compose,
  aiNoteEditSpanStream: mocks.editSpan,
  aiCancel: mocks.cancel,
  newCancelKey: () => `k${++mocks.keyN}`,
  friendlyError: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

import {
  continueNoteAi,
  dismissNoteAi,
  dismissNoteAiError,
  dropNoteAiUnder,
  getNoteAiPhases,
  getNoteAiRun,
  remapNoteAiPaths,
  resetNoteAiRunsForTest,
  setNoteSpanResult,
  startNoteAi,
  startNoteSpanAi,
  stopNoteAi,
} from "./noteAiRun";

const config = { provider: "claude", model: "", cliPath: null } as unknown as AppConfig;
const META: InvocationMeta = {
  model: "m",
  session_id: null,
  cost_usd: null,
  input_tokens: null,
  output_tokens: null,
  duration_ms: 1,
  truncated: false,
  continued: false,
};

type Compose = Parameters<typeof import("./ai").aiNoteComposeStream>;
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
const flush = () => new Promise((r) => setTimeout(r, 0));
const start = (path: string, markdown = "") =>
  startNoteAi({
    path,
    title: "n",
    markdown,
    typed: "write",
    chosen: [],
    refDirs: [],
    instruction: "write",
    config,
  });

beforeEach(() => {
  resetNoteAiRunsForTest();
  mocks.keyN = 0;
  mocks.compose.mockReset();
  mocks.editSpan.mockReset();
  mocks.cancel.mockClear();
});

describe("noteAiRun store", () => {
  it("streams while running and keeps the result on the note path when done", async () => {
    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation((...args: Compose) => {
      const [, onDelta, onActivity, onDraft] = args;
      onDelta("ab");
      onActivity?.({ tool: "Read", target: "/x" });
      onDraft?.("# full");
      onDelta("DONE 1"); // 파일 모드의 마무리 한 줄 — 스냅샷 뒤 델타는 버린다
      return d.promise;
    });
    void start("A/n.md", "old body");
    let run = getNoteAiRun("A/n.md")!;
    expect(run.phase).toBe("running");
    expect(run.stream).toBe("# full");
    expect(run.activity?.tool).toBe("Read");
    expect(run.baseMarkdown).toBe("old body");
    expect(run.cancelKey).toBe("k1");

    d.resolve({ markdown: "# full", meta: { ...META, truncated: true } });
    await flush();
    run = getNoteAiRun("A/n.md")!;
    expect(run.phase).toBe("done");
    expect(run.result).toBe("# full");
    expect(run.truncated).toBe(true);
    expect(run.cancelKey).toBeNull();
    expect(run.finishedAt).not.toBeNull();
  });

  it("ignores a second start on the same note while it is running", async () => {
    mocks.compose.mockImplementation(() => deferred<never>().promise);
    void start("A/n.md");
    void start("A/n.md");
    expect(mocks.compose).toHaveBeenCalledTimes(1);
  });

  it("stop kills the CLI, forgets a first run and drops its late result", async () => {
    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation(() => d.promise);
    void start("A/n.md");
    stopNoteAi("A/n.md");
    expect(mocks.cancel).toHaveBeenCalledWith("k1");
    expect(getNoteAiRun("A/n.md")).toBeUndefined();
    d.resolve({ markdown: "late", meta: META });
    await flush();
    expect(getNoteAiRun("A/n.md")).toBeUndefined();
  });

  it("keeps the previous result through a re-run; a failure keeps it too and can be dismissed", async () => {
    mocks.compose.mockResolvedValueOnce({ markdown: "old result", meta: META });
    await start("A/n.md");
    expect(getNoteAiRun("A/n.md")!.result).toBe("old result");

    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation(() => d.promise);
    void start("A/n.md", "changed body");
    let run = getNoteAiRun("A/n.md")!;
    expect(run.phase).toBe("running");
    expect(run.result).toBe("old result");

    d.reject(new Error("boom"));
    await flush();
    run = getNoteAiRun("A/n.md")!;
    expect(run.phase).toBe("error");
    expect(run.error).toBe("boom");
    expect(run.result).toBe("old result");

    dismissNoteAiError("A/n.md");
    expect(getNoteAiRun("A/n.md")!.phase).toBe("done");

    // 재실행 중 중단 → 받아 둔 결과로 돌아간다
    const d2 = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation(() => d2.promise);
    void start("A/n.md");
    stopNoteAi("A/n.md");
    expect(getNoteAiRun("A/n.md")!.phase).toBe("done");
    expect(getNoteAiRun("A/n.md")!.result).toBe("old result");

    dismissNoteAi("A/n.md");
    expect(getNoteAiRun("A/n.md")).toBeUndefined();
  });

  it("continues a truncated result by splicing the tail and never touches the head", async () => {
    mocks.compose.mockResolvedValueOnce({ markdown: "head\n\ntail part", meta: { ...META, truncated: true } });
    await start("A/n.md");
    mocks.editSpan.mockImplementation(async (params: { span: string }, onDelta: (s: string) => void) => {
      expect(params.span).toBe("head\n\ntail part"); // 짧은 결과는 전체가 꼬리다
      onDelta("...");
      return { text: "head\n\ntail part + more", meta: META };
    });
    await continueNoteAi("A/n.md", config);
    const run = getNoteAiRun("A/n.md")!;
    expect(run.phase).toBe("done");
    expect(run.continuing).toBe(false);
    expect(run.result).toBe("head\n\ntail part + more");
    expect(run.truncated).toBe(false);
  });

  it("follows renames and folder moves, and drops runs under a deleted folder", async () => {
    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation(() => d.promise);
    void start("A/n.md");
    remapNoteAiPaths("A", "B", true);
    expect(getNoteAiRun("A/n.md")).toBeUndefined();
    expect(getNoteAiRun("B/n.md")?.path).toBe("B/n.md");
    remapNoteAiPaths("B/n.md", "B/renamed.md", false);
    expect(getNoteAiRun("B/renamed.md")?.title).toBe("renamed");

    dropNoteAiUnder("B");
    expect(mocks.cancel).toHaveBeenCalledWith("k1");
    expect(getNoteAiRun("B/renamed.md")).toBeUndefined();
    d.resolve({ markdown: "late", meta: META });
    await flush();
    expect(getNoteAiRun("B/renamed.md")).toBeUndefined();
  });

  it("changes the phases snapshot only when a phase changes, not on every delta", async () => {
    let emitDelta: ((s: string) => void) | null = null;
    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockImplementation((...args: Compose) => {
      emitDelta = args[1];
      return d.promise;
    });
    void start("A/n.md");
    const snap1 = getNoteAiPhases();
    expect(snap1.get("A/n.md")).toBe("running");
    emitDelta!("more text");
    expect(getNoteAiPhases()).toBe(snap1); // 같은 참조 — 트리는 다시 그리지 않는다
    d.resolve({ markdown: "done", meta: META });
    await flush();
    const snap2 = getNoteAiPhases();
    expect(snap2).not.toBe(snap1);
    expect(snap2.get("A/n.md")).toBe("done");
  });
});

// ---- 부분 수정 (span) — 전문 작성과 같은 자리를 쓰되, 결과는 조각을 되끼운 노트 전문이다 ----
describe("부분 수정", () => {
  const BODY = "머리말\n\n가운데 문단\n\n꼬리말";
  const mid = { kind: "section" as const, start: 5, end: 11, label: "가운데" };
  const startSpan = (path: string, spans = [mid]) =>
    startNoteSpanAi({
      path,
      title: "n",
      markdown: BODY,
      spans,
      instruction: "짧게",
      refDirs: [],
      config,
    });

  it("조각을 받아 노트 전문으로 되끼운다", async () => {
    mocks.editSpan.mockResolvedValue({ text: "고친 문단", meta: META });
    await startSpan("a.md");
    const run = getNoteAiRun("a.md")!;
    expect(run.kind).toBe("span");
    expect(run.phase).toBe("done");
    expect(run.spanResults).toEqual(["고친 문단"]);
    expect(run.result).toBe("머리말\n\n고친 문단\n\n꼬리말");
  });

  it("떨어진 묶음은 뒤에서부터 되끼워 앞 묶음의 오프셋이 밀리지 않는다", async () => {
    // 앞을 먼저 갈아끼우면 길이가 달라져 뒤 구간이 엉뚱한 자리를 가리킨다
    mocks.editSpan
      .mockResolvedValueOnce({ text: "아주 긴 새 머리말", meta: META })
      .mockResolvedValueOnce({ text: "새 꼬리", meta: META });
    await startSpan("b.md", [
      { kind: "section", start: 0, end: 3, label: "머리말" },
      { kind: "section", start: 13, end: 16, label: "꼬리말" },
    ]);
    expect(getNoteAiRun("b.md")!.result).toBe("아주 긴 새 머리말\n\n가운데 문단\n\n새 꼬리");
  });

  it("검토 화면에서 조각을 손보면 전문도 다시 끼운다", async () => {
    mocks.editSpan.mockResolvedValue({ text: "첫 결과", meta: META });
    await startSpan("c.md");
    setNoteSpanResult("c.md", 0, "손본 결과");
    const run = getNoteAiRun("c.md")!;
    expect(run.spanResults).toEqual(["손본 결과"]);
    expect(run.result).toBe("머리말\n\n손본 결과\n\n꼬리말");
  });

  it("실패해도 실행은 남아 배너가 알린다", async () => {
    mocks.editSpan.mockRejectedValue(new Error("CLI 없음"));
    await startSpan("d.md");
    const run = getNoteAiRun("d.md")!;
    expect(run.phase).toBe("error");
    expect(run.error).toBe("CLI 없음");
  });

  it("같은 노트에 실행 둘을 겹치지 않는다 — 전문 작성 중이면 시작하지 않는다", async () => {
    const d = deferred<{ markdown: string; meta: InvocationMeta }>();
    mocks.compose.mockReturnValue(d.promise);
    void start("e.md");
    await flush();
    await startSpan("e.md");
    expect(getNoteAiRun("e.md")!.kind).toBe("compose"); // 부분 수정이 덮어쓰지 않았다
    expect(mocks.editSpan).not.toHaveBeenCalled();
    d.resolve({ markdown: "done", meta: META });
    await flush();
  });

  it("전문 작성 결과를 부분 수정 실행이 물려받지 않는다", async () => {
    mocks.compose.mockResolvedValue({ markdown: "전문 결과", meta: META });
    await start("f.md");
    expect(getNoteAiRun("f.md")!.result).toBe("전문 결과");
    mocks.editSpan.mockRejectedValue(new Error("끊김"));
    await startSpan("f.md");
    // 종류가 다른 결과를 이어받으면 부분 수정 검토 화면에 전문이 뜬다
    expect(getNoteAiRun("f.md")!.result).toBeNull();
  });

  it("중단하면 받아 둔 결과가 없을 때 실행 자체가 사라진다", async () => {
    const d = deferred<{ text: string; meta: InvocationMeta }>();
    mocks.editSpan.mockReturnValue(d.promise);
    void startSpan("g.md");
    await flush();
    expect(getNoteAiRun("g.md")!.phase).toBe("running");
    stopNoteAi("g.md");
    expect(getNoteAiRun("g.md")).toBeUndefined();
    expect(mocks.cancel).toHaveBeenCalled();
  });
});
