import { beforeEach, describe, expect, it, vi } from "vitest";

// 사이드카는 파일시스템 계층이라 fs 플러그인과 writeAtomic 을 갈아끼운다
// (noteAiRun.test.ts 가 ./ai 에 쓰는 것과 같은 기법).
const fs = vi.hoisted(() => ({
  files: new Map<string, string>(),
  exists: vi.fn(async (p: string) => fs.files.has(p)),
  readTextFile: vi.fn(async (p: string) => {
    const v = fs.files.get(p);
    if (v === undefined) throw new Error("ENOENT");
    return v;
  }),
  remove: vi.fn(async (p: string) => {
    fs.files.delete(p);
  }),
  writeAtomic: vi.fn(async (p: string, body: string) => {
    fs.files.set(p, body);
  }),
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
  BaseDirectory: { AppData: 1 },
  exists: fs.exists,
  readTextFile: fs.readTextFile,
  remove: fs.remove,
}));
vi.mock("./vaultTree", () => ({ writeAtomic: fs.writeAtomic }));
vi.mock("./workspace", () => ({ getRoot: () => "/root" }));

const {
  commentsPathFor,
  loadComments,
  mutateComments,
  readComments,
  saveComments,
} = await import("./comments");

const NOTE = "CS/tcp.md";
const PATH = `/root/${commentsPathFor(NOTE)}`;

function comment(id: string) {
  return {
    id,
    anchor: "혼잡 제어",
    occurrence: 0,
    question: `${id} 질문`,
    answer: `${id} 답변`,
    createdAt: 1,
  };
}

beforeEach(() => {
  fs.files.clear();
  vi.clearAllMocks();
});

describe("readComments — 없음과 못 읽음을 구분한다", () => {
  it("파일이 없으면 성공 + 빈 목록 (질문이 없는 노트다)", async () => {
    expect(await readComments(NOTE)).toEqual({ ok: true, comments: [] });
  });

  it("JSON 이 깨졌으면 실패로 알린다", async () => {
    fs.files.set(PATH, '{"version":1,"comments":[{"anchor":"a","que');
    expect(await readComments(NOTE)).toEqual({ ok: false });
  });

  it("comments 가 배열이 아니면 실패로 알린다", async () => {
    fs.files.set(PATH, '{"version":1,"comments":{}}');
    expect(await readComments(NOTE)).toEqual({ ok: false });
  });

  it("읽기 자체가 던지면 실패로 알린다", async () => {
    fs.exists.mockResolvedValueOnce(true); // 있다고 했는데
    fs.readTextFile.mockRejectedValueOnce(new Error("EIO")); // 읽다가 깨진다
    expect(await readComments(NOTE)).toEqual({ ok: false });
  });
});

describe("mutateComments — 못 읽으면 쓰지 않는다", () => {
  it("정상 파일이면 병합해 저장한다", async () => {
    fs.files.set(
      PATH,
      JSON.stringify({ version: 1, comments: [comment("a")] }),
    );
    const next = await mutateComments(NOTE, (list) => [...list, comment("b")]);
    expect(next.map((c) => c.id)).toEqual(["a", "b"]);
    expect(fs.writeAtomic).toHaveBeenCalledTimes(1);
    const saved = JSON.parse(fs.files.get(PATH)!);
    expect(saved.comments.map((c: { id: string }) => c.id)).toEqual(["a", "b"]);
  });

  /** 이 테스트가 막는 사고: 질문 20개가 든 사이드카가 꼬리만 잘려도 예전 코드는 빈 목록으로
   *  읽은 뒤 새 질문 하나로 파일을 통째로 교체했다. writeAtomic 이라 복구 수단이 없었다. */
  it("깨진 파일에 질문을 추가하려 하면 던지고 파일을 건드리지 않는다", async () => {
    const broken = '{"version":1,"comments":[{"anchor":"혼잡", "ques';
    fs.files.set(PATH, broken);

    await expect(
      mutateComments(NOTE, (list) => [...list, comment("new")]),
    ).rejects.toThrow();

    expect(fs.writeAtomic).not.toHaveBeenCalled();
    expect(fs.remove).not.toHaveBeenCalled();
    expect(fs.files.get(PATH)).toBe(broken); // 원본이 그대로 남아 손으로 고칠 수 있다
  });

  it("깨진 파일에서 삭제를 시도해도 파일을 비우지 않는다", async () => {
    const broken = "not json at all";
    fs.files.set(PATH, broken);

    await expect(
      mutateComments(NOTE, (list) => list.filter((c) => c.id !== "a")),
    ).rejects.toThrow();

    expect(fs.writeAtomic).not.toHaveBeenCalled();
    expect(fs.remove).not.toHaveBeenCalled();
    expect(fs.files.get(PATH)).toBe(broken);
  });

  it("파일이 없던 노트의 첫 질문은 정상 저장된다 (없음은 실패가 아니다)", async () => {
    const next = await mutateComments(NOTE, (list) => [...list, comment("a")]);
    expect(next).toHaveLength(1);
    expect(fs.writeAtomic).toHaveBeenCalledTimes(1);
  });
});

describe("loadComments — 표시용은 빈 목록으로 접는다", () => {
  it("깨졌어도 던지지 않는다 (읽기만 하는 화면용)", async () => {
    fs.files.set(PATH, "{{{");
    expect(await loadComments(NOTE)).toEqual([]);
  });
});

describe("saveComments", () => {
  it("목록이 비면 사이드카를 지운다", async () => {
    fs.files.set(PATH, JSON.stringify({ version: 1, comments: [comment("a")] }));
    await saveComments(NOTE, []);
    expect(fs.remove).toHaveBeenCalledTimes(1);
    expect(fs.files.has(PATH)).toBe(false);
  });
});
