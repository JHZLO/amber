#!/usr/bin/env node
// 랜딩 페이지(docs/index.html)의 정적 정보를 git 태그에서 다시 쓴다 — 최신 버전 표기, tarball 링크, 변경 기록 다섯 줄,
// 태그 개수. GitHub API 를 부르지 않는다: 릴리스마다 release.mjs 가 이 스크립트를 돌려 bump 커밋에 함께 실으니,
// 태그를 푸시하면 Pages 가 재배포되면서 사이트가 저절로 최신이 된다.
//
//   node scripts/site-sync.mjs                 기존 태그만으로 다시 쓴다
//   node scripts/site-sync.mjs --next v0.21.0  아직 없는 다음 태그를 맨 위에 넣는다(release.mjs 가 bump 직후 호출 — 태그는 커밋 뒤에 생기므로)
//
// 변경 기록 한 줄 = 그 태그에 들어간 커밋 제목(conventional 접두어 제거, build/ci 제외) 최대 두 개.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";

const REPO = "https://github.com/JHZLO/amber";
const KEEP = 5; // 변경 기록에 보이는 태그 수

/** "feat(notes): Run whole-note AI writing in the background" → "Run whole-note AI writing in the background" */
export function displaySubject(subject) {
  const m = subject.match(/^\w+(?:\([^)]*\))?!?:\s*(.+)$/);
  const text = (m ? m[1] : subject).trim();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** 한 태그의 커밋 제목들(최신 순) → 변경 기록 한 줄. 버전 bump·CI 커밋은 내용이 아니라 뺀다 */
export function summarize(subjects) {
  const kept = subjects.filter((s) => s.trim() && !/^(build|ci)(\(|:|!)/.test(s)).map(displaySubject);
  if (!kept.length) return "Maintenance release";
  return kept.slice(0, 2).join(" · ");
}

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const endsPunct = (s) => /[.!?]$/.test(s);

export function renderChangelog(entries) {
  return entries
    .map((e, i) => {
      const text = esc(endsPunct(e.text) ? e.text : `${e.text}.`);
      return `        <li class="layer reveal${i === 0 ? " now" : ""}"><span class="mono tag">${esc(e.tag)}</span><time datetime="${esc(e.date)}">${esc(e.date)}</time><p>${text}</p><a class="mono" href="${REPO}/archive/refs/tags/${esc(e.tag)}.tar.gz">tar.gz</a></li>`;
    })
    .join("\n");
}

/** HTML 안의 표식만 바꾼다: 변경 기록 마커 사이, data-latest 스팬, data-tarball 링크, #tagline 앞 문구. 두 번 돌려도 같다 */
export function syncHtml(html, { entries, tagCount, since }) {
  const START = "<!-- changelog:start -->", END = "<!-- changelog:end -->";
  const a = html.indexOf(START), b = html.indexOf(END);
  if (a < 0 || b < 0 || b < a) throw new Error("docs/index.html 에 changelog:start / changelog:end 마커가 없습니다");
  const latest = entries[0].tag;
  let out = html.slice(0, a + START.length) + "\n" + renderChangelog(entries) + "\n        " + html.slice(b);
  out = out.replace(/(<span[^>]*\bdata-latest\b[^>]*>)[^<]*(<\/span>)/g, `$1${esc(latest)}$2`);
  out = out.replace(/(<a[^>]*\bdata-tarball\b[^>]*\bhref=")[^"]*(")/g, `$1${REPO}/archive/refs/tags/${esc(latest)}.tar.gz$2`);
  out = out.replace(/(<p[^>]*\bid="tagline"[^>]*>)[^<]*/, `$1${tagCount} tags since ${esc(since)} · `);
  return out;
}

function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const git = (args) => execFileSync("git", args, { cwd: root, encoding: "utf8" });
  const nextIdx = process.argv.indexOf("--next");
  const next = nextIdx > 0 ? process.argv[nextIdx + 1] : null;
  if (nextIdx > 0 && !/^v\d+\.\d+\.\d+$/.test(next || "")) {
    console.error("--next 뒤에는 vX.Y.Z 형식의 태그가 와야 합니다");
    process.exit(1);
  }

  // 최신순 태그 (vX.Y.Z 만)
  const tags = git(["for-each-ref", "--sort=-creatordate", "--format=%(refname:short)|%(creatordate:short)", "refs/tags"])
    .trim()
    .split("\n")
    .map((l) => l.split("|"))
    .filter(([t]) => /^v\d+\.\d+\.\d+$/.test(t))
    .map(([tag, date]) => ({ tag, date }));
  if (!tags.length) {
    console.error("태그가 없습니다");
    process.exit(1);
  }
  const subjects = (range) => git(["log", "--format=%s", range]).trim().split("\n").filter(Boolean);

  const entries = [];
  if (next) {
    const d = new Date();
    const today = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    entries.push({ tag: next, date: today, text: summarize(subjects(`${tags[0].tag}..HEAD`)) });
  }
  for (let i = 0; i < tags.length && entries.length < KEEP; i++) {
    const cur = tags[i], prev = tags[i + 1];
    entries.push({ tag: cur.tag, date: cur.date, text: summarize(subjects(prev ? `${prev.tag}..${cur.tag}` : cur.tag)) });
  }

  const file = join(root, "docs/index.html");
  const before = readFileSync(file, "utf8");
  const after = syncHtml(before, { entries, tagCount: tags.length + (next ? 1 : 0), since: tags[tags.length - 1].date });
  if (after !== before) writeFileSync(file, after);
  console.log(`  ✓ docs/index.html  (${entries[0].tag}, 변경 기록 ${entries.length}줄, 태그 ${tags.length + (next ? 1 : 0)}개${after === before ? ", 변화 없음" : ""})`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
