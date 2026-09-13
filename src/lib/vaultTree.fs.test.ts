// createVaultTree 의 **파일시스템 조작** 검증. 순수 계산은 vaultTree.test.ts 가 맡는다.
//
// 이 파일이 생긴 이유: 파일을 실제로 지우고·옮기고·덮어쓰는 네 함수(deleteEntry·moveEntry·
// renameEntry·createFile)와 writeAtomic 이 "파일시스템 래퍼"라는 이유로 테스트에서 빠져 있었다.
// "파일이 정본"인 앱에서 가장 파괴적인 코드가 게이트 밖에 있던 셈이다. fs 플러그인과 invoke 를
// 갈아끼우면 전부 순수 로직으로 검증된다.

import { beforeEach, describe, expect, it, vi } from "vitest";

const fs = vi.hoisted(() => {
  const paths = new Set<string>();
  return {
    paths,
    exists: vi.fn(async (p: string) => paths.has(p)),
    mkdir: vi.fn(async (p: string) => {
      paths.add(p);
    }),
    rename: vi.fn(async (from: string, to: string) => {
      paths.delete(from);
      paths.add(to);
    }),
    remove: vi.fn(async (p: string) => {
      paths.delete(p);
    }),
    writeTextFile: vi.fn(async (p: string) => {
      paths.add(p);
    }),
    readDir: vi.fn(async () => []),
    readTextFile: vi.fn(async () => ""),
    stat: vi.fn(async () => ({ mtime: null })),
    invoke: vi.fn(async () => undefined),
  };
});

vi.mock("@tauri-apps/plugin-fs", () => ({
  BaseDirectory: { AppData: 1 },
  exists: fs.exists,
  mkdir: fs.mkdir,
  readDir: fs.readDir,
  readTextFile: fs.readTextFile,
  remove: fs.remove,
  rename: fs.rename,
  stat: fs.stat,
  writeTextFile: fs.writeTextFile,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: fs.invoke }));

const { createVaultTree, writeAtomic } = await import("./vaultTree");

const ROOT = "vault/notes";
const tree = createVaultTree({
  root: ROOT,
  exts: [".md"],
  template: (title) => `# ${title}\n`,
});

beforeEach(() => {
  fs.paths.clear();
  vi.clearAllMocks();
});

describe("deleteEntry — 빈 경로는 루트 자체다", () => {
  // full("") 은 루트를 그대로 돌려준다. 백엔드는 "홈 하위면 허용"이라 사용자가 '폴더 열기'로
  // 연 ~/rust-notes 같은 작업 폴더가 그대로 통과한다 — 즉 여기서 막지 않으면 막는 곳이 없다.
  it.each(["", "   ", "/", ".", ".."])(
    "%o 은 휴지통으로 보내지 않고 거부한다",
    async (bad) => {
      await expect(tree.deleteEntry(bad)).rejects.toThrow();
      expect(fs.invoke).not.toHaveBeenCalled();
    },
  );

  it("정상 경로는 루트를 붙여 넘긴다", async () => {
    await tree.deleteEntry("CS/tcp.md");
    expect(fs.invoke).toHaveBeenCalledWith("move_to_trash", {
      relPath: `${ROOT}/CS/tcp.md`,
    });
  });
});

describe("moveEntry", () => {
  it("자기 하위로 옮기려 하면 거부하고 rename 하지 않는다", async () => {
    await expect(tree.moveEntry("CS/네트워크", "CS/네트워크/하위")).rejects.toThrow();
    expect(fs.rename).not.toHaveBeenCalled();
  });

  it("자기 자신으로 옮기려 해도 거부한다", async () => {
    await expect(tree.moveEntry("CS/네트워크", "CS/네트워크")).rejects.toThrow();
    expect(fs.rename).not.toHaveBeenCalled();
  });

  it("대상에 같은 이름이 있으면 덮어쓰지 않는다", async () => {
    fs.paths.add(`${ROOT}/OS/tcp.md`);
    await expect(tree.moveEntry("CS/tcp.md", "OS")).rejects.toThrow();
    expect(fs.rename).not.toHaveBeenCalled();
  });

  it("같은 폴더로의 이동은 no-op (rename 없음)", async () => {
    expect(await tree.moveEntry("CS/tcp.md", "CS")).toBe("CS/tcp.md");
    expect(fs.rename).not.toHaveBeenCalled();
  });

  it("정상 이동은 경로만 바꾼다", async () => {
    expect(await tree.moveEntry("CS/tcp.md", "OS")).toBe("OS/tcp.md");
    expect(fs.rename).toHaveBeenCalledWith(
      `${ROOT}/CS/tcp.md`,
      `${ROOT}/OS/tcp.md`,
      expect.anything(),
    );
  });
});

describe("renameEntry", () => {
  it("이미 있는 이름으로는 바꾸지 않는다 (덮어쓰기 방지)", async () => {
    fs.paths.add(`${ROOT}/CS/udp.md`);
    await expect(tree.renameEntry("CS/tcp.md", "udp", false)).rejects.toThrow();
    expect(fs.rename).not.toHaveBeenCalled();
  });

  // macOS 기본 APFS 는 대소문자를 구분하지 않아, exists(newRel) 가 자기 자신에 걸린다.
  // 예전에는 그래서 대소문자만 바꾸는 이름 변경이 "이미 있는 이름"으로 막혔다.
  it("대소문자만 바꾸는 이름 변경이 막히지 않는다", async () => {
    fs.paths.add(`${ROOT}/CS/tcp.md`); // 자기 자신 — 대소문자 무시 FS 에선 TCP.md 도 '있다'
    fs.exists.mockImplementation(async (p: string) =>
      [...fs.paths].some((x) => x.toLowerCase() === p.toLowerCase()),
    );

    expect(await tree.renameEntry("CS/tcp.md", "TCP", false)).toBe("CS/TCP.md");
    expect(fs.rename).toHaveBeenCalledWith(
      `${ROOT}/CS/tcp.md`,
      `${ROOT}/CS/TCP.md`,
      expect.anything(),
    );
  });

  it("폴더도 대소문자만 바꿀 수 있다", async () => {
    fs.paths.add(`${ROOT}/cs`);
    fs.exists.mockImplementation(async (p: string) =>
      [...fs.paths].some((x) => x.toLowerCase() === p.toLowerCase()),
    );
    expect(await tree.renameEntry("cs", "CS", true)).toBe("CS");
    expect(fs.rename).toHaveBeenCalled();
  });

  it("이름이 그대로면 no-op", async () => {
    expect(await tree.renameEntry("CS/tcp.md", "tcp", false)).toBe("CS/tcp.md");
    expect(fs.rename).not.toHaveBeenCalled();
  });
});

describe("createFile / createFolder", () => {
  it("같은 이름의 파일이 있으면 쓰지 않는다", async () => {
    fs.paths.add(`${ROOT}/CS/tcp.md`);
    await expect(tree.createFile("CS", "tcp")).rejects.toThrow();
    expect(fs.writeTextFile).not.toHaveBeenCalled();
  });

  it("다단계 경로는 중간 폴더까지 만든다", async () => {
    expect(await tree.createFile("", "CS/네트워크/tcp")).toBe("CS/네트워크/tcp.md");
    expect(fs.mkdir).toHaveBeenCalledWith(
      `${ROOT}/CS/네트워크`,
      expect.objectContaining({ recursive: true }),
    );
  });

  it("같은 이름의 폴더가 있으면 만들지 않는다", async () => {
    fs.paths.add(`${ROOT}/CS`);
    await expect(tree.createFolder("", "CS")).rejects.toThrow();
    expect(fs.mkdir).not.toHaveBeenCalled();
  });
});

describe("writeAtomic — 저장이 파일을 잃는 유일한 지점", () => {
  it("tmp 에 쓰고 rename 으로 덮는다", async () => {
    await writeAtomic("vault/notes/a.md", "본문");
    expect(fs.writeTextFile).toHaveBeenCalledWith(
      "vault/notes/a.md.amber-tmp",
      "본문",
      expect.anything(),
    );
    expect(fs.rename).toHaveBeenCalledWith(
      "vault/notes/a.md.amber-tmp",
      "vault/notes/a.md",
      expect.anything(),
    );
  });

  it("rename 이 실패하면 tmp 를 치우고 **원래** 오류를 던진다", async () => {
    const boom = new Error("ENOSPC");
    fs.rename.mockRejectedValueOnce(boom);
    await expect(writeAtomic("vault/notes/a.md", "본문")).rejects.toBe(boom);
    expect(fs.remove).toHaveBeenCalledWith(
      "vault/notes/a.md.amber-tmp",
      expect.anything(),
    );
  });

  // 정리 실패가 원래 오류를 가리면 UI 가 "저장 실패"를 못 띄우고 사용자는 저장된 줄 안다
  it("정리까지 실패해도 원래 오류가 그대로 올라온다", async () => {
    const boom = new Error("ENOSPC");
    fs.rename.mockRejectedValueOnce(boom);
    fs.remove.mockRejectedValueOnce(new Error("정리 실패"));
    await expect(writeAtomic("vault/notes/a.md", "본문")).rejects.toBe(boom);
  });
});
