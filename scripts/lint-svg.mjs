#!/usr/bin/env node
// 노트 안의 ```svg 그림이 한 벌로 읽히는지 기계로 확인한다. 눈으로만 보면 작은 겹침과 1~2 단위
// 어긋남을 놓친다 — 화면에서 "UI 가 깨졌다"로 돌아온 신고가 전부 여기서 잡히는 종류였다.
//
//   node scripts/lint-svg.mjs                    vault 의 모든 노트를 훑는다
//   node scripts/lint-svg.mjs a.svg b.svg        따로 뽑아 둔 svg 파일만
//   node scripts/lint-svg.mjs "~/…/노트.md"      특정 노트만
//
// 오류로 잡는 것: 캔버스 이탈, 텍스트/상자 겹침, 텍스트끼리 겹침, 선이 글자를 지나감,
// 선이 상자를 관통, 화살표 끝점이 닿은 상자 중앙과 어긋남, 이어 붙는 사각형의 y/height 와
// 이음매 모서리, 화살촉에 칠해진 강조색, 강조 상자 개수, 팔레트 밖의 색, 글자 크기와 톤 토큰.
//
// '확인 필요'로만 내는 것: 한 줄로 읽히는 글자의 y 어긋남과 너무 짧은 데이터 막대. 열의 리듬과
// 상자 높이가 다르면 y 가 달라야 맞는 경우가 있어 기계가 단정할 수 없다 — 사람이 판단할 후보다.
//
// 글자 폭은 재는 게 아니라 비례로 어림한다(한글 1.0em, 라틴 0.55em). 경계선에서 한두 단위
// 틀릴 수 있으니 임계값에 여유를 두었다 — 여기서 잡히면 거의 항상 진짜 겹침이다.

import { globSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";

const VAULT = join(homedir(), "Library/Application Support/dev.jhzlo.amber/vault/notes");
const ACCENTS = ["#3b82f6", "#ea580c"];
const INK = [1, 0.9, 0.85, 0.62, 0.4, 0.28, 0.12, 0.04];
const TYPE_SIZES = [11, 11.5, 12];

const { window } = new JSDOM("");

/** 글자 하나의 폭 어림 — 실측이 아니라 글자 종류별 비례다 */
export function glyphWidth(ch, fs) {
  if ((ch >= "가" && ch <= "힣") || (ch >= "ㄱ" && ch <= "ㆎ")) return fs * 1.0;
  if (ch === " ") return fs * 0.3;
  if (".,:/|()[]".includes(ch)) return fs * 0.32;
  if ("≥→←↑↓±√²".includes(ch)) return fs * 0.9;
  return fs * 0.55;
}

export function textWidth(s, fs) {
  let w = 0;
  for (const ch of s) w += glyphWidth(ch, fs);
  return w;
}

const num = (v, dflt = 0) => (v === null || v === undefined || v === "" ? dflt : Number(v));
const overlaps = (a, b) => !(a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]);
const inside = (t, r, pad = 2) => t[0] >= r[0] - pad && t[2] <= r[2] + pad && t[1] >= r[1] - pad && t[3] <= r[3] + pad;

/** 가로/세로 선분이 사각형 t 를 가로지르는가 (대각선은 보지 않는다) */
export function segmentHitsBox(t, [x1, y1, x2, y2]) {
  if (Math.abs(y1 - y2) < 0.5) return Math.min(x1, x2) < t[2] && Math.max(x1, x2) > t[0] && y1 > t[1] && y1 < t[3];
  if (Math.abs(x1 - x2) < 0.5) return Math.min(y1, y2) < t[3] && Math.max(y1, y2) > t[1] && x1 > t[0] && x1 < t[2];
  return false;
}

/** path 의 d 를 직선 구간 목록으로 (M/L/H/V 만 — 곡선은 연결선이 아니다) */
export function pathSegments(d) {
  const segs = [];
  let cx = null;
  let cy = null;
  for (const m of d.matchAll(/([MHVL])\s*(-?[\d.]+)(?:[ ,]+(-?[\d.]+))?/g)) {
    const [, cmd, a, b] = m;
    const p = Number(a);
    if (cmd === "M") {
      cx = p;
      cy = Number(b);
    } else if (cx === null) {
      continue;
    } else if (cmd === "L") {
      segs.push([cx, cy, p, Number(b)]);
      cx = p;
      cy = Number(b);
    } else if (cmd === "H") {
      segs.push([cx, cy, p, cy]);
      cx = p;
    } else if (cmd === "V") {
      segs.push([cx, cy, cx, p]);
      cy = p;
    }
  }
  return segs;
}

const INHERITED = ["font-size", "text-anchor", "font-weight", "stroke", "stroke-opacity", "opacity", "fill"];

function collect(el, inherited, out) {
  const attr = { ...inherited };
  for (const k of INHERITED) if (el.getAttribute(k)) attr[k] = el.getAttribute(k);
  const tag = el.tagName.toLowerCase();

  if (tag === "text") {
    const label = (el.textContent || "").trim();
    if (label) {
      const fs = num(attr["font-size"], 12);
      const x = num(el.getAttribute("x"));
      const y = num(el.getAttribute("y"));
      const w = textWidth(label, fs);
      const anchor = attr["text-anchor"] || "start";
      const x0 = anchor === "middle" ? x - w / 2 : anchor === "end" ? x - w : x;
      out.texts.push({
        label,
        box: [x0, y - fs * 0.78, x0 + w, y + fs * 0.22],
        x,
        y,
        fs,
        weight: attr["font-weight"] || "400",
        anchor,
        opacity: num(attr.opacity, 1),
      });
    }
  } else if (tag === "rect") {
    const x0 = num(el.getAttribute("x"));
    const y0 = num(el.getAttribute("y"));
    const w = num(el.getAttribute("width"));
    const h = num(el.getAttribute("height"));
    const stroke = el.getAttribute("stroke") || attr.stroke || "none";
    const rx = el.getAttribute("rx") || "0";
    const box = [x0, y0, x0 + w, y0 + h];
    out.rects.push(box);
    out.seams.push({ x: x0, y: y0, w, h, rx });
    if (stroke === "none" && h >= 12 && h <= 28 && w < 16) out.stubs.push({ x: x0, y: y0, w });
    if (stroke !== "none" && w > 24 && h > 12) out.nodes.push(box);
    if (ACCENTS.includes(stroke) && num(el.getAttribute("stroke-width"), 1) >= 1.5) out.accented.push({ box, hue: stroke });
  } else if (tag === "line") {
    out.segs.push([num(el.getAttribute("x1")), num(el.getAttribute("y1")), num(el.getAttribute("x2")), num(el.getAttribute("y2"))]);
  } else if (tag === "path") {
    out.segs.push(...pathSegments(el.getAttribute("d") || ""));
  }

  for (const child of el.children) collect(child, attr, out);
}

/** 한 줄로 읽히는 글자끼리 y 가 맞는지 — 확정이 아니라 사람이 볼 후보다 */
export function rowMismatches(texts) {
  const centre = (t) => (t.box[0] + t.box[2]) / 2;
  const roles = new Map();
  for (const t of texts) {
    const key = `${t.fs}|${t.weight}|${t.anchor}|${t.opacity}`;
    if (!roles.has(key)) roles.set(key, []);
    roles.get(key).push(t);
  }
  const out = [];
  for (const group of roles.values()) {
    // y 가 20 안쪽이면 한 줄로 읽힌다. 그보다 벌어지면 애초에 다른 줄이다.
    for (const band of cluster(group, (t) => t.y, 20)) {
      if (band.length < 2) continue;
      // 한 칸에 글자가 둘 이상이면 여러 줄짜리 라벨이다 — 줄 간격이지 어긋남이 아니다
      const columns = cluster(band, centre, 48);
      if (columns.length !== band.length) continue;
      const ys = [...new Set(band.map((t) => t.y))];
      if (ys.length === 1) continue;
      const [a, b] = [...band].sort((p, q) => p.y - q.y).filter((_, i, all) => i === 0 || i === all.length - 1);
      out.push(`한 줄로 읽히는데 y 가 어긋남 "${a.label.slice(0, 14)}" y=${a.y} / "${b.label.slice(0, 14)}" y=${b.y}`);
    }
  }
  return out;
}

/** 값이 gap 안쪽으로 이어지는 것끼리 묶는다 */
function cluster(items, value, gap) {
  const sorted = [...items].sort((a, b) => value(a) - value(b));
  const groups = [];
  for (const item of sorted) {
    const last = groups.at(-1);
    if (last && value(item) - value(last.at(-1)) <= gap) last.push(item);
    else groups.push([item]);
  }
  return groups;
}

/** svg 한 장을 검사한다 */
export function lintSvg(source) {
  const doc = new window.DOMParser().parseFromString(`<body>${source}</body>`, "text/html");
  const root = doc.querySelector("svg");
  if (!root) return { issues: ["<svg> 를 찾지 못했다"], notes: [], width: 0, height: 0 };
  const viewBox = (root.getAttribute("viewBox") || "").split(/\s+/).map(Number);
  if (viewBox.length !== 4 || viewBox.some(Number.isNaN)) return { issues: ["viewBox 가 없거나 형식이 아니다"], notes: [], width: 0, height: 0 };
  const [, , W, H] = viewBox;

  const out = { texts: [], rects: [], segs: [], nodes: [], stubs: [], seams: [], accented: [] };
  collect(root, {}, out);
  const issues = [];
  const notes = [];

  // 연결선은 회색이다. 화살촉(상대 m 으로 시작하는 짧은 path)에 강조색이 있으면 어긴 것.
  // 연결선 화살촉은 폭 1 이다. 폭이 다르면 마크의 일부(구간이 축을 넘어간 표시 등)라 예외.
  for (const [tag] of source.matchAll(/<path[^>]*\bd="m[^"]*"[^>]*>/g)) {
    if (!tag.includes('stroke-width="1"') && tag.includes("stroke-width")) continue;
    for (const hue of ACCENTS) {
      if (tag.includes(`stroke="${hue}"`) || tag.includes(`fill="${hue}"`)) issues.push(`화살촉에 강조색 ${hue} — 연결선은 ink.muted 회색이다`);
    }
  }

  // 강조는 그림당 하나. 전후 비교의 node.state 쌍만 예외이고, 그때도 두 색이 달라야 한다.
  if (out.accented.length === 2) {
    if (out.accented[0].hue === out.accented[1].hue) issues.push(`같은 강조색 ${out.accented[0].hue} 로 두 곳을 강조 — 하이라이트는 그림당 하나다`);
  } else if (out.accented.length > 2) {
    issues.push(`강조한 상자가 ${out.accented.length}개 — 하이라이트 하나, 또는 전후 한 쌍까지다`);
  }

  // 강조색은 둘뿐이다. 팔레트 밖의 색은 그 자리에서 잡는다.
  for (const [, hex] of source.matchAll(/(#[0-9a-fA-F]{6})/g)) {
    if (!ACCENTS.includes(hex.toLowerCase())) issues.push(`팔레트에 없는 색 ${hex} — accent.emphasis/#accent.contrast 둘뿐이다`);
  }

  // 글자 토큰 — 네 역할의 크기와 잉크 램프만 쓴다
  for (const t of out.texts) {
    if (!TYPE_SIZES.includes(t.fs)) issues.push(`글자 크기 ${t.fs} "${t.label.slice(0, 14)}" — type 은 12/11.5/11 뿐이다`);
    if (!INK.some((v) => Math.abs(v - t.opacity) < 1e-6)) issues.push(`글자 투명도 ${t.opacity} "${t.label.slice(0, 14)}" — ink 램프 밖이다`);
  }

  for (const t of out.texts) {
    const b = t.box;
    if (b[0] < 4 || b[2] > W - 4 || b[1] < 2 || b[3] > H - 2) {
      issues.push(`캔버스 벗어남 "${t.label.slice(0, 18)}" x ${b[0].toFixed(0)}..${b[2].toFixed(0)} / y ${b[1].toFixed(0)}..${b[3].toFixed(0)}`);
    }
    if (out.rects.some((r) => overlaps(b, r) && !inside(b, r))) issues.push(`텍스트/상자 겹침 "${t.label.slice(0, 18)}"`);
    if (out.segs.some((s) => segmentHitsBox(b, s))) issues.push(`선이 텍스트를 지나감 "${t.label.slice(0, 18)}"`);
  }
  for (let a = 0; a < out.texts.length; a++) {
    for (let b = a + 1; b < out.texts.length; b++) {
      if (overlaps(out.texts[a].box, out.texts[b].box)) issues.push(`텍스트끼리 겹침 "${out.texts[a].label.slice(0, 14)}" / "${out.texts[b].label.slice(0, 14)}"`);
    }
  }

  // 한 줄 정렬은 기계가 단정할 수 없다 — 8 단위로 흐르는 열과 64 짜리 상자가 만나면 y 가 달라야 맞는다.
  // 그래서 오류가 아니라 사람이 볼 목록으로 낸다. 캡션이 184 와 200 에 앉은 실제 사고가 여기 걸린다.
  notes.push(...rowMismatches(out.texts));

  // 연결선은 상자를 관통하지 않고, 끝점은 닿은 상자의 중앙에 선다
  const encloses = (o, i) => o !== i && o[0] <= i[0] + 1 && o[1] <= i[1] + 1 && o[2] >= i[2] - 1 && o[3] >= i[3] - 1;
  const nodes = out.nodes.filter((r) => !out.nodes.some((q) => encloses(r, q)) && r[2] - r[0] < 0.7 * W);
  const midY = (r) => (r[1] + r[3]) / 2;
  const midX = (r) => (r[0] + r[2]) / 2;
  for (const [x1, y1, x2, y2] of out.segs) {
    const horiz = Math.abs(y1 - y2) < 0.5;
    const vert = Math.abs(x1 - x2) < 0.5;
    const within = (px, py) => nodes.some((r) => px > r[0] + 1 && px < r[2] - 1 && py > r[1] + 1 && py < r[3] - 1);
    if (within(x1, y1) || within(x2, y2)) continue; // 상자 안에서 출발하면 연결선이 아니다 (격자 스포크, 측정선)
    for (const r of nodes) {
      if (horiz && y1 > r[1] + 1 && y1 < r[3] - 1 && Math.min(x1, x2) < r[2] - 1 && Math.max(x1, x2) > r[0] + 1) {
        const ends = [x1, x2].some((x) => Math.abs(x - r[0]) < 2 || Math.abs(x - r[2]) < 2);
        if (!ends) issues.push(`선이 상자를 관통 (가로 y=${y1.toFixed(0)})`);
      }
      if (vert && x1 > r[0] + 1 && x1 < r[2] - 1 && Math.min(y1, y2) < r[3] - 1 && Math.max(y1, y2) > r[1] + 1) {
        const ends = [y1, y2].some((y) => Math.abs(y - r[1]) < 2 || Math.abs(y - r[3]) < 2);
        if (!ends) issues.push(`선이 상자를 관통 (세로 x=${x1.toFixed(0)})`);
      }
    }
    for (const [ex, ey] of [[x1, y1], [x2, y2]]) {
      for (const r of nodes) {
        const touchesSide = horiz && (Math.abs(ex - r[0]) < 2 || Math.abs(ex - r[2]) < 2) && ey >= r[1] - 1 && ey <= r[3] + 1;
        const touchesCap = vert && (Math.abs(ey - r[1]) < 2 || Math.abs(ey - r[3]) < 2) && ex >= r[0] - 1 && ex <= r[2] + 1;
        if (touchesSide && Math.abs(ey - midY(r)) > 2) issues.push(`끝점 y=${ey.toFixed(0)} 이 닿은 상자의 세로 중앙 ${midY(r).toFixed(0)} 과 어긋남`);
        if (touchesCap && Math.abs(ex - midX(r)) > 2) issues.push(`끝점 x=${ex.toFixed(0)} 이 닿은 상자의 가로 중앙 ${midX(r).toFixed(0)} 과 어긋남`);
      }
    }
  }

  // 이어 붙는 사각형은 같은 y/height 이고 이음매에 둥근 모서리를 두지 않는다
  for (const a of out.seams) {
    for (const b of out.seams) {
      if (a === b || Math.abs(a.x + a.w - b.x) > 1.5) continue;
      if (!(a.y < b.y + b.h && b.y < a.y + a.h)) continue;
      if (Math.abs(a.y - b.y) > 0.5 || Math.abs(a.h - b.h) > 0.5) issues.push(`이어 붙는 사각형의 y/높이가 어긋남 (이음매 x=${b.x.toFixed(0)})`);
      else if (a.rx !== "0" && b.rx !== "0") issues.push(`이음매에 둥근 모서리 rx=${a.rx} (x=${b.x.toFixed(0)}, y=${a.y.toFixed(0)})`);
    }
  }

  for (const s of out.stubs) {
    notes.push(`데이터 막대가 ${s.w.toFixed(0)}단위로 짧다 (x=${s.x.toFixed(0)}, y=${s.y.toFixed(0)}) — 값이 0 에 가까운 경우가 아니면 축을 다시 잡아야 한다`);
  }

  return { issues: [...new Set(issues)], notes: [...new Set(notes)], width: W, height: H };
}

/** 마크다운 본문에서 ```svg 블록만 뽑는다 */
export function svgBlocks(markdown) {
  return [...markdown.matchAll(/```svg\n([\s\S]*?)\n```/g)].map((m) => m[1]);
}

function* sources(args) {
  const paths = args.length ? args.flatMap((a) => (a.includes("*") ? globSync(a) : [a])) : globSync(join(VAULT, "**/*.md"));
  for (const path of paths.sort()) {
    const body = readFileSync(path, "utf8");
    const label = basename(path).replace(/\.(md|svg)$/, "");
    if (path.endsWith(".svg")) yield { label, index: 1, source: body };
    else for (const [i, source] of svgBlocks(body).entries()) yield { label, index: i + 1, source };
  }
}

function main(args) {
  let problems = 0;
  let figures = 0;
  const notes = [];
  for (const { label, index, source } of sources(args)) {
    figures++;
    const { issues, notes: soft, width, height } = lintSvg(source);
    for (const n of soft) notes.push(`[${label}-${index}] ${n}`);
    if (!issues.length) continue;
    problems += issues.length;
    console.log(`\n[${label}-${index}]  ${width}x${height}`);
    for (const i of issues) console.log("   ", i);
  }
  console.log(`\n그림 ${figures}장, 문제 ${problems}건`);
  if (notes.length) {
    console.log("\n확인 필요 (오류 아님):");
    for (const n of new Set(notes)) console.log("   ", n);
  }
  return problems ? 1 : 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
