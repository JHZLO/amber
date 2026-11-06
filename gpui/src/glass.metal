// resin viewer/glass.ts 의 WebGL2 셰이더를 Metal 로 — 무대(반 크기 텍스처), 화면(별, 격자, 결, 비네트), 카드 밑 유리.
// 좌표는 GL 판과 같게 CSS 픽셀, 왼쪽 위가 원점(px)이다. Metal 의 텍스처는 위에서 아래라 GL 판의 y 뒤집기는 없다.
// GLSL 의 mod 는 floor 로 자른다(Metal 의 fmod 는 0 쪽으로 잘라 음수 칸에서 갈린다). pow(음수, 2) 는 제곱으로.
#include <metal_stdlib>
using namespace metal;

constexpr sampler lin(filter::linear, mip_filter::linear, address::clamp_to_edge);

static float gmod(float x, float y) { return x - y * floor(x / y); }
static float2 gmod(float2 x, float y) { return x - y * floor(x / y); }

static float hash(float2 p) { p = fract(p * float2(123.34, 456.21)); p += dot(p, p + 45.32); return fract(p.x * p.y); }
static float noise(float2 p) {
  float2 i = floor(p), f = fract(p); float2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + float2(1.0, 0.0)), u.x), mix(hash(i + float2(0.0, 1.0)), hash(i + float2(1.0, 1.0)), u.x), u.y);
}
static float fbm(float2 p) {
  float v = 0.0, a = 0.5; float2x2 m = float2x2(float2(1.6, 1.2), float2(-1.2, 1.6));
  for (int i = 0; i < 5; i++) { v += a * noise(p); p = m * p; a *= 0.5; }
  return v;
}

struct Screen { float4 position [[position]]; float2 uv; };

// 화면을 덮는 삼각형 하나 — uv 는 GL 판과 같게 아래가 0
vertex Screen vs_screen(uint vid [[vertex_id]]) {
  float2 pos = float2(vid == 1 ? 3.0 : -1.0, vid == 2 ? 3.0 : -1.0);
  Screen o; o.position = float4(pos, 0.0, 1.0); o.uv = pos * 0.5 + 0.5; return o;
}

// ---- 1. 무대 ----

struct StageU {
  float4 size_pan;    // 판 크기 xy, 패닝 zw
  float4 time;        // 시간, 무늬(0 오로라, 1 비단, 2 물빛), 빛 세기
  float4 base, c0, c1, c2, k;
  float4 spot0, spot1, spot2;
  float4 light;       // x, y, 밝기, 반지름
  float4 light_color;
  float4 ripple[4];
};

static float caustic(float2 uv, float time) {
  float2 p = gmod(uv * 6.2831853, 6.2831853) - 250.0;
  float2 i = p; float c = 1.0;
  for (int n = 0; n < 4; n++) {
    float t = time * (1.0 - (3.5 / float(n + 1)));
    i = p + float2(cos(t - i.x) + sin(t + i.y), sin(t - i.y) + cos(t + i.x));
    c += 1.0 / length(float2(p.x / (sin(i.x + t) / 0.005), p.y / (cos(i.y + t) / 0.005)));
  }
  c /= 4.0; c = 1.17 - pow(c, 1.4);
  return clamp(pow(abs(c), 8.0), 0.0, 1.0);
}

static float2 ripples(float2 px, constant StageU& u) {
  float2 off = float2(0.0);
  for (int k = 0; k < 4; k++) {
    float4 r = u.ripple[k];
    if (r.w < 0.5) continue;
    float2 d = px - r.xy; float dist = length(d) + 1e-4;
    float front = r.z * 380.0;
    float x = (dist - front) / 36.0;
    float ring = exp(-x * x) * exp(-r.z * 1.7);
    off += d / dist * sin((dist - front) * 0.11) * 10.0 * ring;
  }
  return off;
}

static float3 field(float3 base, float3 col, float k, float2 q, float2 c, float r) {
  float2 d = q - c; return mix(base, col, k * exp(-dot(d, d) / (r * r)));
}

fragment float4 fs_stage(Screen in [[stage_in]], constant StageU& u [[buffer(0)]]) {
  float2 size = u.size_pan.xy, pan = u.size_pan.zw;
  float2 px = float2(in.uv.x, 1.0 - in.uv.y) * size;
  px += ripples(px, u);
  float R = max(size.x, size.y);
  float2 q = (px - pan) / R;
  float t = u.time.x;
  int style = int(u.time.y + 0.5);
  float3 C0 = u.c0.rgb, C1 = u.c1.rgb, C2 = u.c2.rgb;
  float3 K = u.k.xyz;
  float3 col = u.base.rgb;
  if (style == 0) {
    float2 w = q + 0.07 * float2(fbm(q * 2.6 + t * 0.04), fbm(q * 2.6 - t * 0.035 + 7.3)) - 0.035;
    col = field(col, C0, K.x * 0.35, w, u.spot0.xy + 0.07 * float2(sin(t * 0.07), cos(t * 0.05)), u.spot0.z * 0.62);
    col = field(col, C1, K.y * 0.35, w, u.spot1.xy + 0.07 * float2(cos(t * 0.06), sin(t * 0.08)), u.spot1.z * 0.62);
    col = field(col, C2, K.z * 0.35, w, u.spot2.xy + 0.07 * float2(sin(t * 0.05 + 1.0), cos(t * 0.07 + 2.0)), u.spot2.z * 0.62);
    // 띠 둘 — 둘째 색은 위에서 옅게, 셋째 색은 낮은 빛으로. 띠마다 밝은 아랫단이 접히고 흔들리며 빛살이 오른다
    for (int i = 0; i < 2; i++) {
      float fi = float(i);
      float3 c = i == 0 ? C1 : C0;
      float k = i == 0 ? K.y : K.x * 0.8;
      float x = q.x * (1.0 + 0.4 * fi) + fi * 3.7 + t * 0.012 * (1.0 + fi);
      float edge = (i == 0 ? 0.36 : 0.21) + 0.3 * (fbm(float2(x * 0.7 + t * 0.05, fi * 5.1)) - 0.5);
      float dy = q.y - edge;
      float fold = fbm(float2(x * 2.5, t * 0.12 + fi * 3.0));
      float rays = 0.55 + 0.45 * sin(x * 70.0 + fold * 14.0 + t * 0.4);
      rays = mix(rays, 1.0, 0.35) * (0.55 + 0.45 * fbm(float2(x * 4.0 - t * 0.15, fi * 2.3)));
      float z = dy / 0.024;
      float curtain = dy > 0.0 ? exp(-z * z) : exp(dy / (0.12 + 0.06 * fi));
      col = mix(col, c, clamp(curtain * rays * k * 1.3, 0.0, 1.0));
    }
  } else if (style == 1) {
    float2 p = q * 2.0;
    float a = fbm(p + float2(t * 0.025, -t * 0.02));
    float b = fbm(p * 1.3 + a * 1.8 + float2(-t * 0.02, t * 0.024) + 3.1);
    float c = fbm(p * 0.8 + b * 1.5 + 7.7);
    col = mix(col, C1, smoothstep(0.3, 0.8, a) * K.y);
    col = mix(col, C0, smoothstep(0.35, 0.85, c) * K.x);
    float sheen = pow(max(0.0, 1.0 - abs(b - 0.55) * 4.0), 5.0);
    col = mix(col, C2, sheen * K.z);
  } else {
    float depth = fbm(q * 1.6 + float2(t * 0.02, -t * 0.015));
    col = mix(col, C0, (0.35 + 0.65 * depth) * K.x);
    float c = caustic(q * 2.4 + float2(t * 0.01, t * 0.008), t * 0.32);
    col += C1 * c * K.y;
    col = mix(col, C2, smoothstep(0.55, 0.95, fbm(q * 1.2 - t * 0.02)) * K.z);
  }
  float2 dl = px - u.light.xy;
  col += u.light_color.rgb * u.time.z * u.light.z * exp(-dot(dl, dl) / (u.light.w * u.light.w));
  return float4(col, 1.0);
}

// ---- 2. 화면 — 무대 위에 별(오로라), 격자, 결, 비네트 ----

struct ScreenU {
  float4 size;        // 판 크기 xy, dpr, 시간
  float4 view;        // 패닝 xy, 배율, 어두운가
  float4 grid;        // rgb, 세기(꺼짐 0)
  float4 light;
  float4 stars;       // rgb, 세기(없음 0)
  float4 misc;        // 결, 비네트, 별 패닝 xy
};

// 칸마다 많아야 별 하나, 제 박자로 반짝이고 가장 밝은 것은 열십자로 번진다
static float star(float2 px, constant ScreenU& u) {
  float2 p = px - u.misc.zw;
  const float cell = 28.0;
  float2 id = floor(p / cell);
  if (hash(id) > 0.2) return 0.0;
  float2 at = id * cell + 8.0 + float2(hash(id + 11.3), hash(id + 27.1)) * (cell - 16.0);
  float2 a = abs(p - at);
  float d = length(a);
  float bright = hash(id + 3.7);
  float tw = 0.5 + 0.5 * sin(u.size.w * (0.7 + 2.3 * hash(id + 5.9)) + 6.2831 * hash(id + 8.2));
  float size = 0.55 + 0.85 * bright;
  float s = ((1.0 - smoothstep(size * 0.45, size, d)) + exp(-d * d / (size * size * 5.0)) * 0.3) * (0.3 + 0.7 * bright) * (0.35 + 0.65 * tw);
  if (bright > 0.88) s += (exp(-a.y * 1.4) * exp(-a.x / 4.5) + exp(-a.x * 1.4) * exp(-a.y / 4.5)) * 0.7 * tw * tw;
  return s;
}

fragment float4 fs_screen(Screen in [[stage_in]], constant ScreenU& u [[buffer(0)]], texture2d<float> stage [[texture(0)]]) {
  float2 size = u.size.xy;
  float2 px = float2(in.uv.x, 1.0 - in.uv.y) * size;
  float3 col = stage.sample(lin, px / size).rgb;
  if (u.stars.a > 0.0) {
    // 밤에는 오로라가 밝은 곳에서 별이 옅어진다
    float lum = dot(col, float3(0.2126, 0.7152, 0.0722));
    float sky = u.view.w > 0.5 ? 1.0 - smoothstep(0.12, 0.4, lum) : 1.0;
    col = mix(col, u.stars.rgb, clamp(star(px, u) * u.stars.a * sky, 0.0, 1.0));
  }
  if (u.grid.a > 0.0) {
    float step = 24.0 * u.view.z;
    float2 cell = floor((px - u.view.xy) / step + 0.5);
    float d = length(px - (cell * step + u.view.xy));
    bool major = gmod(cell.x, 5.0) == 0.0 && gmod(cell.y, 5.0) == 0.0;
    float fade = major ? 1.0 : smoothstep(9.0, 20.0, step);
    float sz = major ? 1.1 : 0.8;
    float a = 1.0 - smoothstep(sz - 0.5, sz + 0.5, d);
    float2 dl = px - u.light.xy;
    float wake = 1.0 + 1.8 * u.light.z * exp(-dot(dl, dl) / (230.0 * 230.0));
    col = mix(col, u.grid.rgb, clamp(a * u.grid.a * fade * wake, 0.0, 1.0));
  }
  col += (hash(floor(px * u.size.z)) - 0.5) * u.misc.x;
  float2 v = (px / size - 0.5) * float2(size.x, size.y) / max(size.x, size.y) * 1.4;
  col *= 1.0 - u.misc.y * smoothstep(0.35, 0.95, length(v));
  return float4(col, 1.0);
}

// ---- 3. 카드 밑 유리 ----

struct GlassU {
  float4 size;        // 판 크기 xy, 둘레 여백, 모서리
  float4 view;        // 패닝 xy, 배율, 렌즈
  float4 veil, tint, rim, shadow;
  float4 misc;        // 흐림 lod, 테두리 선, 어두운가
  float4 grid;
};

struct Card { float4 rect; float4 dim; };

struct GlassIn {
  float4 position [[position]];
  float2 px;
  float4 rect [[flat]];
  float dim [[flat]];
};

vertex GlassIn vs_glass(uint vid [[vertex_id]], uint iid [[instance_id]],
                        constant GlassU& u [[buffer(0)]], const device Card* cards [[buffer(1)]]) {
  float2 corner = float2(float(vid & 1u), float((vid >> 1) & 1u));
  Card c = cards[iid];
  float4 r = float4(c.rect.xy * u.view.z + u.view.xy, c.rect.zw * u.view.z);
  float margin = u.size.z;
  float2 p = r.xy - margin + corner * (r.zw + 2.0 * margin);
  float2 clip = p / u.size.xy * 2.0 - 1.0;
  GlassIn o; o.position = float4(clip.x, -clip.y, 0.0, 1.0); o.px = p; o.rect = r; o.dim = c.dim.x;
  return o;
}

static float sdRR(float2 p, float2 b, float r) { float2 q = abs(p) - b + r; return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r; }

static float3 frosted(texture2d<float> stage, float2 uv, float lod) {
  float2 texel = 1.0 / float2(stage.get_width(0), stage.get_height(0));
  float s = exp2(lod) * 0.6;
  float3 c = stage.sample(lin, uv, level(lod)).rgb * 0.4;
  c += stage.sample(lin, uv + float2(s, s) * texel, level(lod)).rgb * 0.15;
  c += stage.sample(lin, uv + float2(-s, s) * texel, level(lod)).rgb * 0.15;
  c += stage.sample(lin, uv + float2(s, -s) * texel, level(lod)).rgb * 0.15;
  c += stage.sample(lin, uv + float2(-s, -s) * texel, level(lod)).rgb * 0.15;
  return c;
}

// 유리 너머의 격자 — 맑은 곳에서만
static float dots(float2 px, constant GlassU& u) {
  float step = 24.0 * u.view.z;
  float2 cell = floor((px - u.view.xy) / step + 0.5);
  float d = length(px - (cell * step + u.view.xy));
  bool major = gmod(cell.x, 5.0) == 0.0 && gmod(cell.y, 5.0) == 0.0;
  float fade = major ? 1.0 : smoothstep(9.0, 20.0, step);
  float size = (major ? 1.1 : 0.8) * 1.35;
  return (1.0 - smoothstep(size - 0.6, size + 0.6, d)) * fade;
}

fragment float4 fs_glass(GlassIn in [[stage_in]], constant GlassU& u [[buffer(0)]], texture2d<float> stage [[texture(0)]]) {
  float2 size = u.size.xy;
  float scale = u.view.z;
  float2 hb = in.rect.zw * 0.5;
  float2 p = in.px - (in.rect.xy + hb);
  float r = min(u.size.w, min(hb.x, hb.y));
  float d = sdRR(p, hb, r);
  float cover = 1.0 - smoothstep(-0.6, 0.6, d);

  float ds = sdRR(p - float2(0.0, 12.0 * scale + 4.0), hb, r);
  float shadow = (1.0 - smoothstep(-16.0, 30.0 * scale + 8.0, ds)) * 0.75;
  shadow += (1.0 - smoothstep(-1.0, 3.0, sdRR(p - float2(0.0, 1.0), hb, r))) * 0.35;
  shadow = clamp(shadow, 0.0, 1.0) * u.shadow.a;
  shadow = max(shadow, (1.0 - smoothstep(0.2, 1.4, d)) * u.misc.y);
  shadow *= 1.0 - cover;

  float4 glass = float4(0.0);
  if (cover > 0.0) {
    float e = 0.75;
    float2 n = float2(sdRR(p + float2(e, 0.0), hb, r) - sdRR(p - float2(e, 0.0), hb, r),
                      sdRR(p + float2(0.0, e), hb, r) - sdRR(p - float2(0.0, e), hb, r));
    n /= max(length(n), 1e-4);
    // 테두리 띠는 렌즈다 — 가운데보다 맑고, 뒤를 바깥으로 휘어 무대와 격자 점이 가장자리를 따라 미끄러진다
    float band = 16.0 * sqrt(scale);
    float clear = pow(clamp(1.0 + d / band, 0.0, 1.0), 2.0);
    float bend = clear * clear;
    float2 shift = n * bend * u.view.w;
    float2 uv = in.px / size;
    float2 su = shift / size;
    float lod = mix(u.misc.x, u.misc.x * 0.3, clear);
    float3 col = float3(frosted(stage, uv + su * 1.15, lod).r, frosted(stage, uv + su, lod).g, frosted(stage, uv + su * 0.85, lod).b);
    float l = dot(col, float3(0.2126, 0.7152, 0.0722));
    col = mix(float3(l), col, 1.3);
    col = mix(col, u.grid.rgb, dots(in.px - shift * 2.2, u) * u.grid.a * 1.2 * clear);
    col = mix(col, u.veil.rgb, u.veil.a * (1.0 - 0.22 * clear));
    float ty = clamp((in.px.y - in.rect.y) / max(in.rect.w, 1.0), 0.0, 1.0);
    col = mix(col, u.tint.rgb, u.tint.a * (1.0 - ty) * (1.0 - ty));
    // 가장자리 — 고른 선 하나, 반짝임 없이
    float edge = 1.0 - smoothstep(0.0, 1.2, -d);
    col = mix(col, u.rim.rgb, edge * u.rim.a);
    glass = float4(col * cover, cover);
  }
  float4 sh = float4(u.shadow.rgb * shadow, shadow);
  return (glass + sh * (1.0 - glass.a)) * in.dim;
}

// ---- 4. YCbCr 4:2:0(GPUI 의 paint_surface 가 받는 꼴) — 전 범위 BT.601, 색차는 0.5 중심 ----

fragment float fs_luma(Screen in [[stage_in]], texture2d<float> color [[texture(0)]]) {
  float2 uv = float2(in.uv.x, 1.0 - in.uv.y);
  float3 c = color.sample(lin, uv).rgb;
  return dot(c, float3(0.299, 0.587, 0.114));
}

fragment float2 fs_chroma(Screen in [[stage_in]], texture2d<float> color [[texture(0)]]) {
  // 2x2 칸의 가운데를 선형으로 읽으면 넷의 평균
  float2 uv = float2(in.uv.x, 1.0 - in.uv.y);
  float3 c = color.sample(lin, uv).rgb;
  float cb = dot(c, float3(-0.168736, -0.331264, 0.5)) + 0.5;
  float cr = dot(c, float3(0.5, -0.418688, -0.081312)) + 0.5;
  return float2(cb, cr);
}
