// Amber landing — one frame loop drives everything scroll-linked from a smoothed scroll value, so the
// pinned track, the changelog axis and the hero drop glide instead of stepping with each wheel notch.
// Lenis (loaded before this module, optional) gives the page itself inertia; without it the effects
// still ease toward the native scroll position.
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
const desktop = () => innerWidth > 900;

/* ---- smooth scroll (Lenis) — native scroll position, eased each frame; sticky keeps working ---- */
let lenis = null;
if (!reduce && typeof Lenis !== "undefined") {
  lenis = new Lenis({ lerp: 0.085, smoothWheel: true, wheelMultiplier: 0.9, touchMultiplier: 1.3 });
}
for (const a of $$('a[href^="#"]')) {
  a.addEventListener("click", (e) => {
    const id = a.getAttribute("href");
    const el = id.length > 1 ? document.querySelector(id) : document.body;
    if (!el) return;
    e.preventDefault();
    if (lenis) lenis.scrollTo(el, { offset: -72, duration: 1.4 });
    else el.scrollIntoView({ behavior: reduce ? "auto" : "smooth" });
  });
}

/* ---- headline: split into words so each rises on its own ---- */
for (const h of $$("[data-split]")) {
  const words = h.textContent.trim().split(/\s+/);
  h.textContent = "";
  words.forEach((w, i) => {
    const outer = document.createElement("span");
    outer.className = "w";
    const inner = document.createElement("span");
    inner.style.setProperty("--i", i);
    inner.textContent = w;
    outer.appendChild(inner);
    h.appendChild(outer);
    if (i < words.length - 1) h.appendChild(document.createTextNode(" "));
  });
}

/* ---- reveals ---- */
const io = new IntersectionObserver(
  (entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  },
  { rootMargin: "0px 0px -12% 0px", threshold: 0.05 },
);
$$(".reveal").forEach((el) => io.observe(el));

/* ---- pointer: one source for the hero glow, the magnetic button and the drop ---- */
let px = innerWidth / 2, py = innerHeight / 2, gx = px, gy = py;
addEventListener("pointermove", (e) => { px = e.clientX; py = e.clientY; }, { passive: true });
for (const m of $$(".magnetic")) {
  m.addEventListener("pointermove", (e) => {
    const r = m.getBoundingClientRect();
    const dx = e.clientX - (r.left + r.width / 2), dy = e.clientY - (r.top + r.height / 2);
    m.style.transform = `translate(${dx * 0.18}px, ${dy * 0.28}px)`;
  });
  m.addEventListener("pointerleave", () => { m.style.transform = ""; });
}

/* ---- card: tilt to the pointer, flip on click, sheen follows ---- */
const card = $("#card3d");
if (card) {
  card.addEventListener("pointermove", (e) => {
    const r = card.getBoundingClientRect();
    const u = (e.clientX - r.left) / r.width, v = (e.clientY - r.top) / r.height;
    card.style.setProperty("--ry", `${(u - 0.5) * 22}deg`);
    card.style.setProperty("--rx", `${(0.5 - v) * 16}deg`);
    card.style.setProperty("--gx", `${u * 100}%`);
    card.style.setProperty("--gy", `${v * 100}%`);
    card.style.setProperty("--sheen", "1");
  });
  card.addEventListener("pointerleave", () => {
    card.style.setProperty("--ry", "0deg"); card.style.setProperty("--rx", "0deg"); card.style.setProperty("--sheen", "0");
  });
  const flip = () => card.classList.toggle("flipped");
  card.addEventListener("click", flip);
  card.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); flip(); } });
  // flip once by itself when it first scrolls into view — the answer is the point
  if (!reduce) new IntersectionObserver((es, obs) => {
    if (es.some((x) => x.isIntersecting)) { setTimeout(() => card.classList.add("flipped"), 900); obs.disconnect(); }
  }, { threshold: 0.6 }).observe(card);
}

/* ---- scroll-linked pieces, all fed from `sy` (smoothed scroll) in the frame loop ---- */
const nav = $("#nav");
const hero = $("#hero"), gemHost = $("#gem");
const hs = $("#features"), track = $("#track"), hprog = $("#hprog"), hidx = $("#hidx");
const shots = $$(".panel-shot", track);
const layers = $(".layers"), axisfill = $("#axisfill");
let heroP = 0; // 0 = hero fully on screen, 1 = scrolled past

// the pinned section is exactly as tall as the sideways distance plus one viewport: 1:1, no rush
function fitHorizontal() {
  if (!hs) return;
  if (!desktop()) { hs.style.height = ""; track.style.transform = ""; return; }
  hs.style.height = `${innerHeight + (track.scrollWidth - innerWidth)}px`;
}
fitHorizontal();
addEventListener("resize", fitHorizontal);

function horizontal(sy) {
  if (!hs || !desktop()) return;
  const top = hs.offsetTop, total = hs.offsetHeight - innerHeight;
  const p = clamp((sy - top) / total, 0, 1);
  const dist = track.scrollWidth - innerWidth;
  track.style.transform = `translate3d(${-p * dist}px, 0, 0)`;
  hprog.style.width = `${p * 100}%`;
  const n = shots.length;
  hidx.textContent = String(clamp(Math.round(p * (n - 1)) + 1, 1, n));
  // screenshots drift a little slower than their copy — depth without a library
  shots.forEach((s, i) => { const local = p * (n - 1) - i; s.style.transform = `translate3d(${clamp(local, -1, 1) * 40}px, 0, 0)`; });
}
function strata(sy) {
  if (!layers) return;
  const r = layers.getBoundingClientRect();
  const docTop = r.top + scrollY; // document position, independent of the eased value
  const p = clamp((sy + innerHeight * 0.85 - docTop) / r.height, 0, 1);
  axisfill.style.transform = `scaleY(${p})`;
}
function heroScroll(sy) {
  heroP = clamp(sy / (innerHeight * 0.9), 0, 1);
  if (!gemHost) return;
  gemHost.style.transform = `translate3d(0, ${-heroP * 80}px, 0) scale(${1 - heroP * 0.18})`;
  gemHost.style.opacity = String(clamp(1 - heroP * 1.1, 0, 1));
}

let sy = scrollY, renderGem = null;
function frame(t) {
  requestAnimationFrame(frame);
  if (lenis) lenis.raf(t);
  // with Lenis the native position is already eased; without it, ease the effects ourselves
  sy = lenis ? scrollY : lerp(sy, scrollY, 0.12);
  if (Math.abs(sy - scrollY) < 0.5) sy = scrollY;
  nav.classList.toggle("scrolled", sy > 24);
  heroScroll(sy);
  horizontal(sy);
  strata(sy);
  // glow eases toward the pointer
  gx = lerp(gx, px, 0.12); gy = lerp(gy, py, 0.12);
  hero.style.setProperty("--mx", `${gx}px`); hero.style.setProperty("--my", `${gy + scrollY}px`);
  if (renderGem) renderGem(t);
}
requestAnimationFrame(frame);

/* ---- terminal: type the commands when the block comes into view ---- */
const term = $("#terminal");
if (term) {
  const lines = $$(".line", term);
  const typeLine = (line) => new Promise((done) => {
    const cmd = $(".cmd", line), text = cmd.dataset.cmd;
    if (reduce) { cmd.textContent = text; done(); return; }
    line.classList.add("typing");
    let i = 0;
    const tick = () => {
      cmd.textContent = text.slice(0, ++i);
      if (i < text.length) setTimeout(tick, 14 + Math.random() * 22);
      else { line.classList.remove("typing"); setTimeout(done, 160); }
    };
    tick();
  });
  new IntersectionObserver(async (es, obs) => {
    if (!es.some((x) => x.isIntersecting)) return;
    obs.disconnect();
    for (const l of lines) await typeLine(l);
  }, { threshold: 0.35 }).observe(term);
  const all = $("#copyall");
  if (all) {
    const script = () => $$(".line:not(.done) .cmd", term).map((c) => c.dataset.cmd).join("\n");
    all.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(script()); all.textContent = "Copied"; all.classList.add("did"); }
      catch { all.textContent = "Select and copy"; }
      setTimeout(() => { all.textContent = "Copy all"; all.classList.remove("did"); }, 1600);
    });
  }
  for (const b of $$(".copy", term)) {
    b.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = "copied"; b.classList.add("did"); }
      catch { b.textContent = "select"; }
      setTimeout(() => { b.textContent = "copy"; b.classList.remove("did"); }, 1400);
    });
  }
}

/* ---- the drop: a lathe of the logo's teardrop, amber glass with inclusions, tilting to the pointer ---- */
(async () => {
  if (!gemHost) return;
  const fallback = () => gemHost.classList.add("fallback");
  const probe = document.createElement("canvas");
  if (!probe.getContext("webgl2") && !probe.getContext("webgl")) { fallback(); return; }
  let THREE, RoomEnvironment;
  try {
    THREE = await import("https://cdn.jsdelivr.net/npm/three@0.160.0/+esm");
    ({ RoomEnvironment } = await import("https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/environments/RoomEnvironment.js/+esm"));
  } catch { fallback(); return; }

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: "high-performance" });
  // glass is rendered twice (transmission pass); keep the pixel budget modest so scrolling stays at 60fps on Retina
  renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
  if ("transmissionResolutionScale" in renderer) renderer.transmissionResolutionScale = 0.6;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.15;
  gemHost.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 50);
  const pmrem = new THREE.PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;

  // teardrop profile (radius, height) → lathe around Y. Same silhouette as the app icon, in 3D.
  const profile = [
    [0, 1.42], [0.09, 1.24], [0.2, 1.02], [0.34, 0.76], [0.5, 0.46], [0.66, 0.14], [0.78, -0.18],
    [0.85, -0.5], [0.84, -0.78], [0.74, -1.0], [0.55, -1.17], [0.3, -1.27], [0.1, -1.31], [0, -1.32],
  ].map(([r, y]) => new THREE.Vector2(r, y));
  const geo = new THREE.LatheGeometry(profile, 96);
  geo.computeVertexNormals();
  // honey, not chocolate: on a dark page a transmissive body reads dark, so the glass itself carries warmth
  // (light attenuation stays long) and a backlight glows through it.
  const amber = new THREE.MeshPhysicalMaterial({
    color: 0xe6a53f, metalness: 0, roughness: 0.2, transmission: 0.82, thickness: 1.1, ior: 1.54,
    attenuationColor: new THREE.Color(0xc27a22), attenuationDistance: 3.2,
    clearcoat: 0.55, clearcoatRoughness: 0.18, envMapIntensity: 0.65, specularIntensity: 0.8,
    emissive: new THREE.Color(0x8a4a10), emissiveIntensity: 0.32,
  });
  const gem = new THREE.Group();
  gem.add(new THREE.Mesh(geo, amber));

  // inclusions: what fell in and stayed. Dark specks and a few bright motes, kept inside the silhouette.
  const radiusAt = (y) => {
    for (let i = 1; i < profile.length; i++) {
      const a = profile[i - 1], b = profile[i];
      if (y <= a.y && y >= b.y) { const t = (a.y - y) / (a.y - b.y || 1); return lerp(a.x, b.x, t); }
    }
    return 0;
  };
  const inside = () => {
    const y = lerp(-1.15, 1.1, Math.random());
    const rMax = radiusAt(y) * 0.78, r = Math.sqrt(Math.random()) * rMax, a = Math.random() * Math.PI * 2;
    return new THREE.Vector3(Math.cos(a) * r, y, Math.sin(a) * r);
  };
  const specks = new THREE.InstancedMesh(new THREE.SphereGeometry(1, 10, 10), new THREE.MeshStandardMaterial({ color: 0x2a1706, roughness: 0.7 }), 46);
  const m = new THREE.Matrix4();
  for (let i = 0; i < 46; i++) { const s = lerp(0.014, 0.05, Math.random() ** 2); m.compose(inside(), new THREE.Quaternion(), new THREE.Vector3(s, s * lerp(0.6, 1.4, Math.random()), s)); specks.setMatrixAt(i, m); }
  gem.add(specks);
  const motePos = new Float32Array(22 * 3);
  for (let i = 0; i < 22; i++) { const v = inside(); motePos.set([v.x, v.y, v.z], i * 3); }
  const moteGeo = new THREE.BufferGeometry();
  moteGeo.setAttribute("position", new THREE.BufferAttribute(motePos, 3));
  const moteMat = new THREE.PointsMaterial({ color: 0xffcf7a, size: 0.032, sizeAttenuation: true, transparent: true, opacity: 0.5, blending: THREE.AdditiveBlending, depthWrite: false });
  gem.add(new THREE.Points(moteGeo, moteMat));
  scene.add(gem);

  const key = new THREE.DirectionalLight(0xffe0b0, 1.4); key.position.set(2.5, 3, 2.2); scene.add(key);
  const rim = new THREE.DirectionalLight(0xbfc5d6, 0.3); rim.position.set(-3, -1, 2.5); scene.add(rim);
  const back = new THREE.PointLight(0xffb356, 26, 9, 2); back.position.set(0.4, 0.3, -1.8); scene.add(back);
  const fill = new THREE.PointLight(0xffd27a, 6, 8, 2); fill.position.set(-2.2, 1.2, 2.4); scene.add(fill);

  // 물방울을 감싸는 경계 '구' — 회전 불변이라 어떻게 돌아가도 이 안에 들어온다
  const yMid = (Math.min(...profile.map((p) => p.y)) + Math.max(...profile.map((p) => p.y))) / 2;
  const gemRadius = Math.max(...profile.map((p) => Math.hypot(p.x, p.y - yMid)));
  const FIT_MARGIN = 1.14; // 숨쉬는 여백 + 위아래로 흔들리는 진폭(0.06)까지 덮는다
  camera.position.set(0, yMid, 5);

  const fit = () => {
    const w = gemHost.clientWidth || 1, h = gemHost.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    // 카메라를 물방울에 맞춘다 — 거리를 4.8 로 박아 두니 세로 화각이 물방울보다 좁아 아래가 잘렸다.
    // 좁은 쪽 화각(세로/가로 중 작은 것)에 구를 맞춰야 어떤 창 비율에서도 온전히 들어온다.
    const vFov = (camera.fov * Math.PI) / 180;
    const hFov = 2 * Math.atan(Math.tan(vFov / 2) * camera.aspect);
    camera.position.z = (gemRadius * FIT_MARGIN) / Math.sin(Math.min(vFov, hFov) / 2);
    camera.updateProjectionMatrix();
    gem.scale.setScalar(1);
  };
  fit();
  new ResizeObserver(fit).observe(gemHost);

  let cx = 0, cy = 0, near = 0, t = 0;
  renderGem = () => {
    // nothing to draw once the hero has scrolled away or the tab is hidden
    if (document.hidden || heroP > 0.92) return;
    t += 0.016;
    const nx = (px / innerWidth) * 2 - 1, ny = -((py / innerHeight) * 2 - 1);
    cx = lerp(cx, nx, 0.06); cy = lerp(cy, ny, 0.06);
    gem.rotation.y += reduce ? 0 : 0.0035;
    gem.rotation.x = lerp(gem.rotation.x, -cy * 0.38, 0.08);
    gem.rotation.z = lerp(gem.rotation.z, -cx * 0.3, 0.08);
    gem.position.y = reduce ? 0 : Math.sin(t * 0.8) * 0.06;
    key.position.set(2.5 + cx * 1.6, 3 + cy * 1.2, 2.2);
    // motes glimmer, more when the pointer sits on the drop
    const r = gemHost.getBoundingClientRect();
    const hx = ((px - r.left) / r.width) * 2 - 1, hy = ((py - r.top) / r.height) * 2 - 1;
    near = lerp(near, clamp(1 - Math.hypot(hx, hy) / 1.1, 0, 1), 0.08);
    moteMat.opacity = 0.32 + 0.18 * Math.sin(t * 2.1) + near * 0.35;
    moteMat.size = 0.03 + near * 0.025;
    back.intensity = 22 + near * 14;
    renderer.render(scene, camera);
  };
})();
