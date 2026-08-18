// 이름/경로 입력 검증 회귀 테스트. 여기서 새는 값이 곧 파일시스템 경로가 되므로
// (특히 '..' 같은 상위 이동) 사유 문자열까지 함께 고정한다.
// createVaultTree 쪽은 파일시스템 래퍼라 대상이 아니다.

import { describe, expect, it } from "vitest";
import {
  invalidNameReason,
  invalidPathReason,
  normalizePath,
  parentOf,
  searchFiles,
  type VaultNode,
} from "./vaultTree";

describe("invalidNameReason", () => {
  it("정상 이름은 null", () => {
    expect(invalidNameReason("네트워크 기초")).toBeNull();
    expect(invalidNameReason("  여백은 무시  ")).toBeNull();
    expect(invalidNameReason("a".repeat(80))).toBeNull();
  });

  it("빈 이름", () => {
    expect(invalidNameReason("")).toBe("이름을 입력하세요.");
    expect(invalidNameReason("   ")).toBe("이름을 입력하세요.");
  });

  it("경로 구분자·콜론", () => {
    for (const n of ["a/b", "a\\b", "a:b"])
      expect(invalidNameReason(n)).toBe("이름에 / \\ : 는 쓸 수 없어요.");
  });

  it("점으로 시작 (숨김 파일·상위 이동)", () => {
    expect(invalidNameReason(".git")).toBe("이름은 . 으로 시작할 수 없어요.");
    expect(invalidNameReason("..")).toBe("이름은 . 으로 시작할 수 없어요.");
  });

  it("길이 초과", () => {
    expect(invalidNameReason("a".repeat(81))).toBe("이름이 너무 길어요 (80자 이내).");
  });
});

describe("invalidPathReason", () => {
  it("다단계 경로는 구간마다 검사한다", () => {
    expect(invalidPathReason("CS/네트워크/TCP")).toBeNull();
    expect(invalidPathReason(" CS / 네트워크 ")).toBeNull();
    expect(invalidPathReason("//CS//네트워크//")).toBeNull(); // 빈 구간은 제거 후 검사
  });

  it("구간이 하나도 없으면 빈 이름", () => {
    for (const p of ["", "   ", "/", "///"])
      expect(invalidPathReason(p)).toBe("이름을 입력하세요.");
  });

  it("한 구간이라도 규칙을 어기면 그 사유를 돌려준다", () => {
    expect(invalidPathReason("CS/../vault")).toBe("이름은 . 으로 시작할 수 없어요.");
    expect(invalidPathReason("CS/a:b")).toBe("이름에 / \\ : 는 쓸 수 없어요.");
    expect(invalidPathReason(`CS/${"a".repeat(81)}`)).toBe("이름이 너무 길어요 (80자 이내).");
  });

  it("앞 구간의 사유가 우선한다", () => {
    expect(invalidPathReason(".a/b:c")).toBe("이름은 . 으로 시작할 수 없어요.");
  });
});

describe("normalizePath / parentOf", () => {
  it("구간 trim·빈 구간 제거", () => {
    expect(normalizePath(" CS / 네트워크 ")).toBe("CS/네트워크");
    expect(normalizePath("//a///b//")).toBe("a/b");
    expect(normalizePath("   ")).toBe("");
  });

  it("상위 폴더 ('' = 루트)", () => {
    expect(parentOf("CS/네트워크/tcp.md")).toBe("CS/네트워크");
    expect(parentOf("tcp.md")).toBe("");
  });
});

describe("searchFiles", () => {
  const TREE: VaultNode[] = [
    {
      name: "CS",
      path: "CS",
      isDir: true,
      children: [{ name: "TCP 핸드셰이크", path: "CS/tcp.md", isDir: false }],
    },
    { name: "회고", path: "회고.md", isDir: false },
    { name: "읽기실패", path: "broken.md", isDir: false },
  ];
  const BODY: Record<string, string> = {
    "CS/tcp.md": "# TCP\n세 번의 악수",
    "회고.md": "이번 주에 tcp 를 정리했다",
  };
  const src = {
    listTree: async () => TREE,
    readFile: async (p: string) => {
      const b = BODY[p];
      if (b === undefined) throw new Error("no such file");
      return b;
    },
  };

  it("파일명 일치가 본문 일치보다 앞", async () => {
    const hits = await searchFiles(src, "tcp");
    expect(hits.map((h) => h.path)).toEqual(["CS/tcp.md", "회고.md"]);
    expect(hits[1].snippet).toBe("이번 주에 tcp 를 정리했다");
  });

  it("대소문자를 무시하고 본문에서 처음 걸린 줄을 준다", async () => {
    const [hit] = await searchFiles(src, "악수");
    expect(hit.path).toBe("CS/tcp.md");
    expect(hit.snippet).toBe("세 번의 악수");
  });

  it("읽기에 실패한 파일은 조용히 빠진다", async () => {
    expect(await searchFiles(src, "실패")).toEqual([
      { name: "읽기실패", path: "broken.md", snippet: null },
    ]); // 이름 일치는 살아남고 snippet 만 null
    expect(await searchFiles(src, "없는말")).toEqual([]);
  });

  it("빈 질의는 즉시 빈 결과", async () => {
    expect(await searchFiles(src, "   ")).toEqual([]);
  });

  it("limit 으로 자른다", async () => {
    expect(await searchFiles(src, "tcp", 1)).toHaveLength(1);
  });

  it("이름 매칭만으로 limit 이 차면 나머지 파일은 읽지 않는다", async () => {
    // ⌘K 는 키 입력이 멈출 때마다 호출된다 — 8줄 그리자고 보관함 전체를 읽으면 안 된다
    const read: string[] = [];
    const many = {
      listTree: async () =>
        Array.from({ length: 50 }, (_, i) => ({
          name: `노트${i}`,
          path: `n${i}.md`,
          isDir: false,
        })),
      readFile: async (p: string) => {
        read.push(p);
        return "본문";
      },
    };
    const hits = await searchFiles(many, "노트", 3);
    expect(hits).toHaveLength(3);
    expect(read).toHaveLength(3); // 50개가 아니라 결과로 낸 3개만
  });

  it("본문 매칭도 limit 이 차면 남은 파일을 읽지 않는다", async () => {
    const read: string[] = [];
    const many = {
      listTree: async () =>
        Array.from({ length: 100 }, (_, i) => ({
          name: `f${i}`,
          path: `f${i}.md`,
          isDir: false,
        })),
      readFile: async (p: string) => {
        read.push(p);
        return "바늘";
      },
    };
    await searchFiles(many, "바늘", 2);
    // 8개 묶음 단위로 읽으므로 첫 묶음에서 멈춘다 — 100개 전부는 아니다
    expect(read.length).toBeLessThanOrEqual(8);
  });

  it("긴 줄은 매칭 지점을 살려 자른다", async () => {
    const long = {
      listTree: async () => [{ name: "긴줄", path: "long.md", isDir: false }],
      readFile: async () => `${"가".repeat(200)}바늘${"나".repeat(200)}`,
    };
    const [hit] = await searchFiles(long, "바늘");
    expect(hit.snippet).toContain("바늘");
    expect(hit.snippet!.startsWith("…")).toBe(true);
    expect(hit.snippet!.length).toBeLessThanOrEqual(122); // 120 + 양끝 말줄임
  });
});
