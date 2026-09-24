// 노트 이미지(_assets) 회귀 테스트. 여기서 새는 값이 곧 디스크 경로가 되거나(저장/복사),
// 웹뷰가 읽을 파일이 된다(렌더) — 특히 `..` 로 루트 밖을 읽거나 원격 URL 을 로컬로 착각하는 경우.

import { beforeEach, describe, expect, it, vi } from "vitest";

const fs = vi.hoisted(() => {
  const files = new Map<string, Uint8Array>();
  const dirs = new Set<string>();
  return {
    files,
    dirs,
    exists: vi.fn(async (p: string) => files.has(p) || dirs.has(p)),
    mkdir: vi.fn(async (p: string) => {
      dirs.add(p);
    }),
    readFile: vi.fn(async (p: string) => {
      const f = files.get(p);
      if (!f) throw new Error(`no such file: ${p}`);
      return f;
    }),
    writeFile: vi.fn(async (p: string, data: Uint8Array) => {
      files.set(p, data);
    }),
  };
});

vi.mock("@tauri-apps/plugin-fs", () => ({
  BaseDirectory: { AppData: 1 },
  exists: fs.exists,
  mkdir: fs.mkdir,
  readFile: fs.readFile,
  writeFile: fs.writeFile,
}));

const {
  ASSET_DIR,
  MAX_IMAGE_BYTES,
  assetFileName,
  assetRefs,
  copyAssetsForMove,
  extForMime,
  imageInsertText,
  imageMarkdown,
  mimeForName,
  resolveLocalSrc,
  saveNoteImage,
} = await import("./noteAssets");

beforeEach(() => {
  fs.files.clear();
  fs.dirs.clear();
  vi.clearAllMocks();
});

describe("이미지 종류", () => {
  it("래스터 이미지만 받는다 — svg 는 스크립트를 품을 수 있어 제외", () => {
    expect(extForMime("image/png")).toBe("png");
    expect(extForMime("image/jpeg")).toBe("jpg");
    expect(extForMime("IMAGE/WEBP")).toBe("webp");
    expect(extForMime("image/svg+xml")).toBeNull();
    expect(extForMime("image/tiff")).toBeNull();
    expect(extForMime("text/plain")).toBeNull();
  });

  it("확장자로 종류를 정한다", () => {
    expect(mimeForName("/Users/me/Desktop/스크린샷 2026.PNG")).toBe("image/png");
    expect(mimeForName("a.jpeg")).toBe("image/jpeg");
    expect(mimeForName("a.svg")).toBeNull();
    expect(mimeForName("README")).toBeNull();
  });
});

describe("파일 이름과 본문 링크", () => {
  it("시각이 앞, 꼬리 네 글자 — 붙인 순서대로 정렬된다", () => {
    const name = assetFileName(new Date(2026, 8, 24, 9, 5, 7), "png", "a1b2");
    expect(name).toBe("20260924-090507-a1b2.png");
  });

  it("링크는 노트 폴더 기준 상대경로", () => {
    expect(imageMarkdown("x.png")).toBe(`![](${ASSET_DIR}/x.png)`);
  });
});

describe("커서 자리에 끼울 텍스트", () => {
  const link = "![](_assets/a.png)";

  it("줄 중간이면 앞뒤로 줄을 바꿔 그림이 혼자 선다", () => {
    const v = "앞글자뒷글자";
    expect(imageInsertText(v, 3, 3, [link])).toBe(`\n${link}\n`);
  });

  it("빈 줄에 붙이면 줄바꿈을 더하지 않는다", () => {
    const v = "첫 줄\n\n다음";
    expect(imageInsertText(v, 4, 4, [link])).toBe(link);
  });

  it("문서 맨 앞과 맨 끝", () => {
    expect(imageInsertText("", 0, 0, [link])).toBe(link);
    expect(imageInsertText("글", 1, 1, [link])).toBe(`\n${link}`);
  });

  it("여러 장은 한 줄에 하나씩", () => {
    expect(imageInsertText("", 0, 0, ["![](_assets/a.png)", "![](_assets/b.png)"])).toBe(
      "![](_assets/a.png)\n![](_assets/b.png)",
    );
  });

  it("선택 구간이 있으면 그 뒤 글자를 기준으로 본다", () => {
    const v = "가나다라";
    expect(imageInsertText(v, 1, 3, [link])).toBe(`\n${link}\n`);
  });
});

describe("본문이 가리키는 파일", () => {
  it("여러 표기를 모두 잡고 중복은 한 번만", () => {
    const md = [
      "![](_assets/a.png)",
      "![설명](./_assets/b.jpg)",
      '![](<_assets/c.webp> "제목")',
      "![](_assets/a.png)",
    ].join("\n\n");
    expect(assetRefs(md)).toEqual(["a.png", "b.jpg", "c.webp"]);
  });

  it("_assets 밖이나 하위 경로, 원격은 무시한다", () => {
    const md = [
      "![](other/a.png)",
      "![](_assets/sub/b.png)",
      "![](https://example.com/_assets/c.png)",
      "![](../_assets/d.png)",
      "![](_assets/.hidden.png)",
    ].join("\n");
    expect(assetRefs(md)).toEqual([]);
  });
});

describe("이미지 src 를 로컬 경로로 풀기", () => {
  const base = "vault/notes/TIL/블로그글";

  it("노트 폴더 기준으로 푼다", () => {
    expect(resolveLocalSrc(base, "_assets/a.png")).toBe(`${base}/_assets/a.png`);
    expect(resolveLocalSrc(base, "./_assets/a.png")).toBe(`${base}/_assets/a.png`);
  });

  it("절대경로 루트('폴더 열기')도 앞의 / 를 지킨다", () => {
    expect(resolveLocalSrc("/Users/me/notes", "_assets/a.png")).toBe(
      "/Users/me/notes/_assets/a.png",
    );
  });

  it("퍼센트 인코딩을 푼다", () => {
    expect(resolveLocalSrc(base, "_assets/%EC%83%B7.png")).toBe(`${base}/_assets/샷.png`);
  });

  it("상위 폴더 이동은 기준 안에서만", () => {
    expect(resolveLocalSrc(base, "../pics/a.png")).toBe("vault/notes/TIL/pics/a.png");
    expect(resolveLocalSrc("a", "../../x.png")).toBeNull();
  });

  it("원격, 스킴, 절대경로, 이미지 아닌 파일은 로컬로 읽지 않는다", () => {
    expect(resolveLocalSrc(base, "https://example.com/a.png")).toBeNull();
    expect(resolveLocalSrc(base, "//cdn.example.com/a.png")).toBeNull();
    expect(resolveLocalSrc(base, "data:image/png;base64,AAAA")).toBeNull();
    expect(resolveLocalSrc(base, "file:///etc/passwd")).toBeNull();
    expect(resolveLocalSrc(base, "/Users/me/a.png")).toBeNull();
    expect(resolveLocalSrc(base, "_assets/notes.md")).toBeNull();
    expect(resolveLocalSrc(base, "")).toBeNull();
  });

  it("쿼리/해시는 떼고 본다", () => {
    expect(resolveLocalSrc(base, "_assets/a.png?v=2#x")).toBe(`${base}/_assets/a.png`);
  });
});

describe("저장", () => {
  const dir = "vault/notes/TIL";

  it("노트 폴더의 _assets 에 쓰고 이름을 돌려준다", async () => {
    const name = await saveNoteImage(dir, new Uint8Array([1, 2, 3]), "image/png");
    expect(name).toMatch(/^\d{8}-\d{6}-[a-z0-9]{4}\.png$/);
    expect(fs.dirs.has(`${dir}/_assets`)).toBe(true);
    expect(fs.files.get(`${dir}/_assets/${name}`)).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("지원하지 않는 종류는 쓰기 전에 거부한다", async () => {
    await expect(saveNoteImage(dir, new Uint8Array([1]), "image/svg+xml")).rejects.toThrow();
    expect(fs.writeFile).not.toHaveBeenCalled();
  });

  it("상한을 넘으면 쓰기 전에 거부한다", async () => {
    const big = new Uint8Array(MAX_IMAGE_BYTES + 1);
    await expect(saveNoteImage(dir, big, "image/png")).rejects.toThrow();
    expect(fs.writeFile).not.toHaveBeenCalled();
  });
});

describe("노트 이동 시 그림 복사", () => {
  const from = "vault/notes/A";
  const to = "vault/notes/B";

  it("본문이 가리키고 실제로 있는 파일만 새 폴더로 복사한다(원본은 남긴다)", async () => {
    fs.files.set(`${from}/_assets/a.png`, new Uint8Array([9]));
    fs.files.set(`${from}/_assets/unused.png`, new Uint8Array([7]));
    const md = "![](_assets/a.png)\n![](_assets/gone.png)";
    await copyAssetsForMove(md, from, to);
    expect(fs.files.get(`${to}/_assets/a.png`)).toEqual(new Uint8Array([9]));
    expect(fs.files.has(`${to}/_assets/gone.png`)).toBe(false);
    expect(fs.files.has(`${to}/_assets/unused.png`)).toBe(false);
    // 같은 폴더의 다른 노트가 같은 그림을 가리킬 수 있어 원본은 지우지 않는다
    expect(fs.files.has(`${from}/_assets/a.png`)).toBe(true);
  });

  it("대상에 같은 이름이 있으면 덮지 않는다", async () => {
    fs.files.set(`${from}/_assets/a.png`, new Uint8Array([1]));
    fs.files.set(`${to}/_assets/a.png`, new Uint8Array([2]));
    await copyAssetsForMove("![](_assets/a.png)", from, to);
    expect(fs.files.get(`${to}/_assets/a.png`)).toEqual(new Uint8Array([2]));
  });

  it("그림이 없거나 같은 폴더면 아무것도 하지 않는다", async () => {
    await copyAssetsForMove("글만 있다", from, to);
    await copyAssetsForMove("![](_assets/a.png)", from, from);
    expect(fs.mkdir).not.toHaveBeenCalled();
    expect(fs.writeFile).not.toHaveBeenCalled();
  });
});
