#!/usr/bin/env node
// GPUI 앱을 Amber.app 으로 묶는다 — 지금까지의 `pnpm tauri build --bundles app` 자리.
// 묶음 정보(이름, 식별자, 아이콘)는 지금 앱과 같다: 같은 식별자라 같은 데이터 폴더(dev.jhzlo.amber)를 쓰고,
// /Applications/Amber.app 을 갈아 끼우면 Dock, 실행 중 확인(pgrep) 같은 것이 그대로 이어진다.
//
//   node scripts/build-app.mjs      → gpui/target/release/bundle/Amber.app
//
// 서명은 ad-hoc(지금 앱과 같다) — 받은 사람이 직접 빌드하는 소스 배포라 공증은 없다.

import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const run = (cmd, args, dir = ".") => execFileSync(cmd, args, { cwd: join(root, dir), stdio: "inherit" });

// gpui/ 안에서 — 그 폴더의 rust-toolchain.toml(더 새 rustc)을 rustup 이 작업 폴더로 고른다
run("cargo", ["build", "--release", "--bin", "amber-gpui"], "gpui");

const app = join(root, "gpui/target/release/bundle/Amber.app");
rmSync(app, { recursive: true, force: true });
mkdirSync(join(app, "Contents/MacOS"), { recursive: true });
mkdirSync(join(app, "Contents/Resources"), { recursive: true });
cpSync(join(root, "gpui/target/release/amber-gpui"), join(app, "Contents/MacOS/amber"));
cpSync(join(root, "src-tauri/icons/icon.icns"), join(app, "Contents/Resources/icon.icns"));

const plist = {
  CFBundleDevelopmentRegion: "English",
  CFBundleDisplayName: "Amber",
  CFBundleExecutable: "amber",
  CFBundleIconFile: "icon.icns",
  CFBundleIdentifier: "dev.jhzlo.amber",
  CFBundleInfoDictionaryVersion: "6.0",
  CFBundleName: "Amber",
  CFBundlePackageType: "APPL",
  CFBundleShortVersionString: version,
  CFBundleVersion: version,
  // Rust 의 aarch64-apple-darwin 기본 배포 대상
  LSMinimumSystemVersion: "11.0",
  NSHighResolutionCapable: true,
};
const entries = Object.entries(plist)
  .map(([k, v]) => `\t<key>${k}</key>\n\t${typeof v === "boolean" ? `<${v}/>` : `<string>${v}</string>`}`)
  .join("\n");
writeFileSync(
  join(app, "Contents/Info.plist"),
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
${entries}
</dict>
</plist>
`,
);

run("codesign", ["--force", "--sign", "-", app]);
console.log(`\n${app}  (v${version})`);
