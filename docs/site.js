// Amber landing — the amber drop (WebGL), pointer glow, scroll reveals, a pinned horizontal track,
// a changelog axis that draws itself, a typewriter terminal, and the latest tag from GitHub.
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;

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

/* ---- nav ---- */
const nav = $("#nav");
const onNav = () => nav.classList.toggle("scrolled", scrollY > 24);
onNav();

/* ---- reveals ---- */
const io = new IntersectionObserver(
  (entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  },
  { rootMargin: "0px 0px -12% 0px", threshold: 0.05 },
);
$$(".reveal").forEach((el) => io.observe(el));

/* ---- hero: pointer glow + magnetic button ---- */
const hero = $("#hero");
let gx = innerWidth / 2, gy = innerHeight / 2, tx = gx, ty = gy;
addEventListener("pointermove", (e) => { tx = e.clientX; ty = e.clientY; }, { passive: true });
const magnets = $$(".magnetic");
for (const m of magnets) {
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
    const px = (e.clientX - r.left) / r.width, py = (e.clientY - r.top) / r.height;
    card.style.setProperty("--ry", `${(px - 0.5) * 22}deg`);
    card.style.setProperty("--rx", `${(0.5 - py) * 16}deg`);
    card.style.setProperty("--gx", `${px * 100}%`);
    card.style.setProperty("--gy", `${py * 100}%`);
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

/* ---- horizontal track: vertical scroll drives translateX while the section is pinned ---- */
const hs = $("#features"), track = $("#track"), hprog = $("#hprog"), hidx = $("#hidx");
const shots = $$(".panel-shot", track);
function horizontal() {
  if (!hs || innerWidth <= 900) return;
  const rect = hs.getBoundingClientRect();
  const total = hs.offsetHeight - innerHeight;
  const p = clamp(-rect.top / total, 0, 1);
  const dist = track.scrollWidth - innerWidth;
  track.style.transform = `translate3d(${-p * dist}px, 0, 0)`;
  hprog.style.width = `${p * 100}%`;
  const n = shots.length;
  hidx.textContent = String(clamp(Math.round(p * (n - 1)) + 1, 1, n));
  // screenshots drift a little slower than their copy — depth without a library
  shots.forEach((s, i) => { const local = p * (n - 1) - i; s.style.transform = `translate3d(${clamp(local, -1, 1) * 40}px, 0, 0)`; });
}

/* ---- strata: the axis fills as the list scrolls through the viewport ---- */
const layers = $(".layers"), axisfill = $("#axisfill");
function strata() {
  if (!layers) return;
  const r = layers.getBoundingClientRect();
  const p = clamp((innerHeight * 0.85 - r.top) / r.height, 0, 1);
  axisfill.style.transform = `scaleY(${p})`;
}

/* ---- hero gem: settle upward and fade as the hero leaves ---- */
const gemHost = $("#gem");
function heroScroll() {
  if (!gemHost) return;
  const p = clamp(scrollY / (innerHeight * 0.9), 0, 1);
  gemHost.style.transform = `translate3d(0, ${-p * 80}px, 0) scale(${1 - p * 0.18})`;
  gemHost.style.opacity = String(1 - p * 1.1);
}

let ticking = false;
function onScroll() {
  if (ticking) return;
  ticking = true;
  requestAnimationFrame(() => { onNav(); horizontal(); strata(); heroScroll(); ticking = false; });
}
addEventListener("scroll", onScroll, { passive: true });
addEventListener("resize", onScroll);
onScroll();

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
  for (const b of $$(".copy", term)) {
    b.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = "copied"; b.classList.add("did"); }
      catch { b.textContent = "select"; }
      setTimeout(() => { b.textContent = "copy"; b.classList.remove("did"); }, 1400);
    });
  }
}

/* ---- latest tag from GitHub (falls back to the baked-in version) ---- */
(async () => {
  try {
    const res = await fetch("https://api.github.com/repos/JHZLO/amber/tags?per_page=100", { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) return;
    const tags = await res.json();
    const latest = tags[0]?.name;
    if (!latest) return;
    $$("[data-latest]").forEach((el) => { el.textContent = latest; });
    $$("[data-tarball]").forEach((a) => { a.href = `https://github.com/JHZLO/amber/archive/refs/tags/${latest}.tar.gz`; });
    const more = /rel="next"/.test(res.headers.get("Link") || "");
    const line = $("#tagline");
    if (line) line.firstChild.textContent = `${more ? "100+" : tags.length} tags since 2026-07-16 · `;
  } catch { /* offline or rate-limited: the page already carries a real version */ }
})();

/* ---- the drop: a lathe of the logo's teardrop, amber glass with inclusions, tilting to the pointer ---- */
(async () => {
  if (!gemHost) return;
  const fallback = () => gemHost.classList.add("fallback");
  const canvas = document.createElement("canvas");
  if (!canvas.getContext("webgl2") && !canvas.getContext("webgl")) { fallback(); return; }
  let THREE, RoomEnvironment;
  try {
    THREE = await import("https://cdn.jsdelivr.net/npm/three@0.160.0/+esm");
    ({ RoomEnvironment } = await import("https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/environments/RoomEnvironment.js/+esm"));
  } catch { fallback(); return; }

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: "high-performance" });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.15;
  gemHost.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 50);
  camera.position.set(0, 0.1, 4.8);
  const pmrem = new THREE.PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;

  // teardrop profile (radius, height) → lathe around Y. Same silhouette as the app icon, in 3D.
  const profile = [
    [0, 1.42], [0.09, 1.24], [0.2, 1.02], [0.34, 0.76], [0.5, 0.46], [0.66, 0.14], [0.78, -0.18],
    [0.85, -0.5], [0.84, -0.78], [0.74, -1.0], [0.55, -1.17], [0.3, -1.27], [0.1, -1.31], [0, -1.32],
  ].map(([r, y]) => new THREE.Vector2(r, y));
  const geo = new THREE.LatheGeometry(profile, 128);
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

  // size to the host
  const fit = () => {
    const w = gemHost.clientWidth || 1, h = gemHost.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h; camera.updateProjectionMatrix();
    gem.scale.setScalar(Math.min(1, w / 520) * (innerWidth <= 900 ? 0.86 : 1));
  };
  fit();
  new ResizeObserver(fit).observe(gemHost);

  // pointer → tilt, light and glow follow with easing
  let nx = 0, ny = 0, cx = 0, cy = 0, near = 0;
  addEventListener("pointermove", (e) => {
    nx = (e.clientX / innerWidth) * 2 - 1; ny = -((e.clientY / innerHeight) * 2 - 1);
  }, { passive: true });

  let visible = true;
  new IntersectionObserver((es) => { visible = es[0].isIntersecting; }).observe(gemHost);
  document.addEventListener("visibilitychange", () => { visible = !document.hidden && visible; });

  let t = 0;
  const loop = () => {
    requestAnimationFrame(loop);
    // page glow follows the pointer even when the gem is idle
    gx = lerp(gx, tx, 0.12); gy = lerp(gy, ty, 0.12);
    hero.style.setProperty("--mx", `${gx}px`); hero.style.setProperty("--my", `${gy + scrollY}px`);
    if (!visible) return;
    t += 0.016;
    cx = lerp(cx, nx, 0.06); cy = lerp(cy, ny, 0.06);
    gem.rotation.y += reduce ? 0 : 0.0035;
    gem.rotation.x = lerp(gem.rotation.x, -cy * 0.38, 0.08);
    gem.rotation.z = lerp(gem.rotation.z, -cx * 0.3, 0.08);
    gem.position.y = reduce ? 0 : Math.sin(t * 0.8) * 0.06;
    key.position.set(2.5 + cx * 1.6, 3 + cy * 1.2, 2.2);
    // motes glimmer, more when the pointer sits on the drop
    const r = gemHost.getBoundingClientRect();
    const hx = ((tx - r.left) / r.width) * 2 - 1, hy = ((ty - r.top) / r.height) * 2 - 1;
    near = lerp(near, clamp(1 - Math.hypot(hx, hy) / 1.1, 0, 1), 0.08);
    moteMat.opacity = 0.32 + 0.18 * Math.sin(t * 2.1) + near * 0.35;
    moteMat.size = 0.03 + near * 0.025;
    back.intensity = 22 + near * 14;
    renderer.render(scene, camera);
  };
  loop();
})();
