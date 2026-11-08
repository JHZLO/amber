#!/usr/bin/env node
// 라이브 유리(glass.rs, glass.metal)의 기준 — amber 가 쓰는 @jhzlo/resin 의 viewer/glass.js 셰이더를 그대로 꺼내
// 정해 둔 프레임(시간, 보기, 빛, 고리, 카드)을 WebGL2 로 그리는 쪽을 만든다. glass.ts 의 draw() 와 같은 순서와 값이다.
//
//   node gpui/tools/glass-ref.mjs <amber 의 node_modules> <쓸 html>
//
// 만든 html 을 브라우저에서 열면 window.glassRef 에 프레임마다 8x8 칸 평균(RGB 0..255)이 남는다.
// 그것을 gpui/tests/glass_ref.json 으로 옮기면 glass.rs 의 matches_webgl_reference 가 Metal 판과 견준다.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const nm = process.argv[2] ?? new URL("../../node_modules", import.meta.url).pathname;
const out = process.argv[3] ?? "glass-ref.html";
const src = readFileSync(join(nm, "@jhzlo/resin/dist/viewer/glass.js"), "utf8");
const { stageOf, glassOf, GRID_STEP, CARD_RADIUS } = await import(join(nm, "@jhzlo/resin/dist/src/svg.js"));

// `const NAME = \`…\`;` 를 그 모듈의 상수로 펼친다(NOISE 먼저 — 다른 셰이더가 끼워 쓴다)
const RIPPLES = Number(/const RIPPLES = (\d+);/.exec(src)[1]);
const raw = Object.fromEntries([...src.matchAll(/const (\w+) = `([\s\S]*?)`;/g)].map((m) => [m[1], m[2]]));
const expand = (body, scope) => new Function(...Object.keys(scope), `return \`${body}\`;`)(...Object.values(scope));
const NOISE = expand(raw.NOISE, {});
const shaders = Object.fromEntries(
  ["VS_SCREEN", "FS_STAGE", "FS_SCREEN", "VS_GLASS", "FS_GLASS"].map((k) => [k, expand(raw[k], { RIPPLES, NOISE, GRID_STEP })]),
);

const card = (x, y, w, h, dim = 1) => [x, y, w, h, dim];
const frames = [
  { look: "aurora-dark", size: [320, 200], time: 7.3, view: [30, 12, 1], light: [160, 50, 1], ripples: [], grid: true, cards: [card(20, 30, 120, 80), card(170, 60, 110, 90, 0.14)] },
  { look: "aurora-light", size: [320, 200], time: 21.5, view: [-40, 5, 1.4], light: [80, 120, 0.7], ripples: [[150, 90, 0.6]], grid: true, cards: [card(10, 10, 90, 60)] },
  { look: "silk-dark", size: [320, 200], time: 3.1, view: [12, -20, 0.6], light: [200, 30, 0.5], ripples: [], grid: true, cards: [card(40, 60, 200, 120), card(300, 40, 80, 60)] },
  { look: "silk-light", size: [320, 200], time: 55, view: [0, 0, 1], light: [160, 24, 0.5], ripples: [[60, 60, 1.2], [250, 150, 0.3]], grid: false, cards: [] },
  { look: "caustic-dark", size: [320, 200], time: 12.7, view: [-15, 30, 1.8], light: [100, 100, 1], ripples: [], grid: true, cards: [card(5, 5, 60, 40), card(80, -10, 70, 50, 0.5)] },
  { look: "caustic-light", size: [320, 200], time: 0, view: [8, 8, 1], light: [160, 24, 0.5], ripples: [], grid: false, cards: [card(30, 30, 140, 100)] },
].map((f) => ({ ...f, stage: stageOf(f.look), glass: glassOf(f.look) }));

const page = `<!doctype html><meta charset="utf-8"><title>glass ref</title>
<body style="margin:0;background:#000">
<script>
const S = ${JSON.stringify(shaders)};
const FRAMES = ${JSON.stringify(frames)};
const RIPPLES = ${RIPPLES}, CARD_RADIUS = ${CARD_RADIUS}, PARALLAX = 0.3;
const STYLE = { aurora: 0, silk: 1, caustic: 2 };
const rgb = (hex) => { const n = parseInt(hex.slice(1), 16); return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]; };
const rgba = ([c, a]) => [...rgb(c), a];
function compile(gl, vs, fs) {
  const p = gl.createProgram();
  for (const [t, s] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
    const sh = gl.createShader(t); gl.shaderSource(sh, s); gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh));
    gl.attachShader(p, sh);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  return p;
}
const u = (gl, p, n) => gl.getUniformLocation(p, n);
function render(f) {
  const [w, h] = f.size;
  const canvas = document.createElement("canvas"); canvas.width = w; canvas.height = h;
  const gl = canvas.getContext("webgl2", { antialias: false, alpha: false, premultipliedAlpha: true, preserveDrawingBuffer: true });
  const stage = compile(gl, S.VS_SCREEN, S.FS_STAGE), screen = compile(gl, S.VS_SCREEN, S.FS_SCREEN), glass = compile(gl, S.VS_GLASS, S.FS_GLASS);
  const screenVao = gl.createVertexArray(); gl.bindVertexArray(screenVao);
  const tri = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, tri);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  const glassVao = gl.createVertexArray(); gl.bindVertexArray(glassVao);
  const quad = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  const inst = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, inst);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(f.cards.flat()), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 20, 0); gl.vertexAttribDivisor(1, 1);
  gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 1, gl.FLOAT, false, 20, 16); gl.vertexAttribDivisor(2, 1);
  gl.bindVertexArray(null);
  const tw = Math.ceil(w / 2), th = Math.ceil(h / 2);
  const tex = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, tw, th, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.generateMipmap(gl.TEXTURE_2D);
  const fbo = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
  gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
  // glass.ts draw()
  const St = f.stage, G = f.glass, [x, y, scale] = f.view;
  const light = [f.light[0], f.light[1], f.light[2], St.light[2]];
  gl.viewport(0, 0, tw, th); gl.disable(gl.BLEND);
  gl.useProgram(stage);
  gl.uniform2f(u(gl, stage, "uSize"), w, h);
  gl.uniform2f(u(gl, stage, "uPan"), x * PARALLAX, y * PARALLAX);
  gl.uniform1f(u(gl, stage, "uTime"), f.time);
  gl.uniform1i(u(gl, stage, "uStyle"), STYLE[St.style]);
  gl.uniform3fv(u(gl, stage, "uBase"), rgb(St.base));
  gl.uniform3fv(u(gl, stage, "uC0"), rgb(St.colors[0][0]));
  gl.uniform3fv(u(gl, stage, "uC1"), rgb(St.colors[1][0]));
  gl.uniform3fv(u(gl, stage, "uC2"), rgb(St.colors[2][0]));
  gl.uniform3f(u(gl, stage, "uK"), St.colors[0][1], St.colors[1][1], St.colors[2][1]);
  gl.uniform3f(u(gl, stage, "uSpot0"), ...St.spots[0]);
  gl.uniform3f(u(gl, stage, "uSpot1"), ...St.spots[1]);
  gl.uniform3f(u(gl, stage, "uSpot2"), ...St.spots[2]);
  gl.uniform4fv(u(gl, stage, "uLight"), light);
  gl.uniform3fv(u(gl, stage, "uLightColor"), rgb(St.light[0]));
  gl.uniform1f(u(gl, stage, "uLightK"), St.light[1]);
  const rip = new Float32Array(RIPPLES * 4);
  f.ripples.forEach((r, i) => rip.set([r[0], r[1], r[2], 1], i * 4));
  gl.uniform4fv(u(gl, stage, "uRipple"), rip);
  gl.bindVertexArray(screenVao); gl.drawArrays(gl.TRIANGLES, 0, 3);
  gl.bindTexture(gl.TEXTURE_2D, tex); gl.generateMipmap(gl.TEXTURE_2D);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.viewport(0, 0, w, h);
  gl.useProgram(screen);
  gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.uniform1i(u(gl, screen, "uStage"), 0);
  gl.uniform2f(u(gl, screen, "uSize"), w, h);
  gl.uniform3f(u(gl, screen, "uView"), x, y, scale);
  gl.uniform4fv(u(gl, screen, "uGrid"), f.grid ? rgba(St.grid) : [0, 0, 0, 0]);
  gl.uniform4fv(u(gl, screen, "uLight"), light);
  gl.uniform1f(u(gl, screen, "uGrain"), St.grain);
  gl.uniform1f(u(gl, screen, "uVignette"), St.vignette);
  gl.uniform1f(u(gl, screen, "uDpr"), 1);
  gl.uniform1f(u(gl, screen, "uTime"), f.time);
  gl.uniform4fv(u(gl, screen, "uStars"), St.stars ? rgba(St.stars) : [0, 0, 0, 0]);
  gl.uniform2f(u(gl, screen, "uStarPan"), x * 0.1, y * 0.1);
  gl.uniform1f(u(gl, screen, "uDark"), St.dark ? 1 : 0);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  if (f.cards.length) {
    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(glass);
    gl.uniform1i(u(gl, glass, "uStage"), 0);
    gl.uniform2f(u(gl, glass, "uSize"), w, h);
    gl.uniform3f(u(gl, glass, "uView"), x, y, scale);
    gl.uniform1f(u(gl, glass, "uMargin"), 30 * scale + 26);
    gl.uniform1f(u(gl, glass, "uRadius"), CARD_RADIUS * scale);
    gl.uniform1f(u(gl, glass, "uScale"), scale);
    gl.uniform4fv(u(gl, glass, "uVeil"), rgba(G.veil));
    gl.uniform4fv(u(gl, glass, "uTint"), rgba(G.tint));
    gl.uniform4fv(u(gl, glass, "uRim"), rgba(G.rim));
    gl.uniform1f(u(gl, glass, "uLens"), G.lens * Math.sqrt(scale));
    gl.uniform1f(u(gl, glass, "uLod"), Math.log2(Math.max(1, (G.frost * Math.sqrt(scale)) / 2)));
    gl.uniform4fv(u(gl, glass, "uShadow"), rgba(G.shadow));
    gl.uniform1f(u(gl, glass, "uOutline"), St.dark ? 0.2 : 0);
    gl.uniform1f(u(gl, glass, "uDark"), St.dark ? 1 : 0);
    gl.uniform4fv(u(gl, glass, "uGrid"), f.grid ? rgba(St.grid) : [0, 0, 0, 0]);
    gl.bindVertexArray(glassVao); gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, f.cards.length);
  }
  const px = new Uint8Array(w * h * 4);
  gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
  // 8x8 칸 평균 — GL 은 아래에서 위로 읽으니 뒤집어 위에서 아래로
  const B = 8, blocks = [];
  for (let by = 0; by < h / B; by++)
    for (let bx = 0; bx < w / B; bx++) {
      const sum = [0, 0, 0];
      for (let yy = 0; yy < B; yy++)
        for (let xx = 0; xx < B; xx++) {
          const row = h - 1 - (by * B + yy), i = (row * w + bx * B + xx) * 4;
          sum[0] += px[i]; sum[1] += px[i + 1]; sum[2] += px[i + 2];
        }
      blocks.push(...sum.map((v) => Math.round((v / (B * B)) * 10) / 10));
    }
  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return blocks;
}
window.glassRef = FRAMES.map((f) => ({ frame: f, blocks: render(f) }));
document.body.append(Object.assign(document.createElement("pre"), { textContent: "done " + window.glassRef.length, style: "color:#fff" }));
</script>`;
writeFileSync(out, page);
console.log(`${frames.length} frames → ${out}`);
