// GLSL ES 3.00 sources for the WebGL2 renderer. Every scene shader runs in
// two passes: the colour pass straight into the canvas, and the emissive pass
// (u_pass = 1) into a small target for bloom. Colours are premultiplied.
// Positions arrive in a tier's CSS-pixel space and are mapped to the screen by
// a per-layer scale and translation.

const HEADER = `#version 300 es
precision highp float;
`;

const TRANSFORM = `
uniform vec2 u_res;     // drawing buffer in device pixels
uniform float u_dpr;
uniform float u_scale;  // layer scale
uniform vec2 u_trans;   // layer translation, CSS px
vec4 toClip(vec2 world) {
  vec2 screen = (world * u_scale + u_trans) * u_dpr;
  vec2 clip = screen / u_res * 2.0 - 1.0;
  return vec4(clip.x, -clip.y, 0.0, 1.0);
}
`;

const OUTPUTS = `
out vec4 o_frag;
uniform float u_pass;
void emit(vec4 color, vec4 glow) { o_frag = u_pass > 0.5 ? glow : color; }
`;

/** Full-screen triangle; v_uv covers 0..1. */
export const FULLSCREEN_VS = `${HEADER}
out vec2 v_uv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  v_uv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
`;

/** Stage colour, a soft accent vignette and the warp tunnel glow around the vanishing point. */
export const BACKGROUND_FS = `${HEADER}${OUTPUTS}
in vec2 v_uv;
uniform vec2 u_res;
uniform vec3 u_stage;
uniform vec3 u_accent;
uniform vec2 u_vp;       // device px, top-left origin
uniform float u_vignette;
uniform float u_tunnel;
float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
void main() {
  vec2 p = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);
  float span = max(u_res.x, u_res.y);
  float v = 1.0 - clamp(length(p - u_res * 0.5) / (span * 0.7), 0.0, 1.0);
  vec3 c = mix(u_stage, u_accent, u_vignette * v);
  float t = (1.0 - clamp(length(p - u_vp) / (span * 0.6), 0.0, 1.0)) * u_tunnel;
  c = mix(c, u_accent, t);
  c += (hash(gl_FragCoord.xy) - 0.5) / 255.0;
  emit(vec4(c, 1.0), vec4(u_accent * t * 0.5, 0.0));
}
`;

/** Stars and warp streaks: one instance per star, projected toward the vanishing point. */
export const STARS_VS = `${HEADER}
layout(location = 0) in vec2 a_corner;  // x: 0..1 along, y: -1..1 across
layout(location = 1) in vec3 a_star;    // x, y in -1..1, depth z
uniform vec2 u_res;
uniform float u_dpr;
uniform vec2 u_vp;
uniform float u_focal;
uniform float u_streak;
uniform float u_dir;
uniform float u_bright;
out vec2 v_pos;
flat out float v_len;
flat out float v_width;
flat out float v_alpha;
void main() {
  float z = a_star.z;
  float z2 = clamp(z + u_dir * u_streak, 0.05, 1.2);
  vec2 head = u_vp + a_star.xy / z * u_focal;
  vec2 tail = u_vp + a_star.xy / z2 * u_focal;
  float depth = 1.0 - z;
  float width = u_streak > 0.004 ? 0.6 + depth * 1.6 : 0.6 + depth * 1.4;
  vec2 d = head - tail;
  float len = length(d);
  vec2 dir = len > 0.001 ? d / len : vec2(1.0, 0.0);
  vec2 n = vec2(-dir.y, dir.x);
  float pad = width * 0.5 + 1.0 / u_dpr;
  float along = mix(-pad, len + pad, a_corner.x);
  vec2 world = tail + dir * along + n * a_corner.y * pad;
  v_pos = vec2(along, a_corner.y * pad);
  v_len = len;
  v_width = width;
  v_alpha = u_bright * depth * (u_streak > 0.004 ? 1.0 : 0.9);
  vec2 clip = world * u_dpr / u_res * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
}
`;

export const STARS_FS = `${HEADER}${OUTPUTS}
in vec2 v_pos;
flat in float v_len;
flat in float v_width;
flat in float v_alpha;
uniform vec3 u_color;
uniform float u_dpr;
uniform float u_glow;
void main() {
  float s = v_pos.x;
  float outside = max(0.0, max(-s, s - v_len));
  float dist = length(vec2(outside, v_pos.y)) - v_width * 0.5;
  float cov = clamp(0.5 - dist * u_dpr, 0.0, 1.0);
  float grad = v_len > 0.5 ? mix(0.12, 1.0, clamp(s / v_len, 0.0, 1.0)) : 1.0;
  float a = v_alpha * cov * grad;
  emit(vec4(u_color * a, a), vec4(u_color * a * u_glow, a));
}
`;

/** Instanced signed-distance shapes: discs, rings, arcs, hexagons, rounded boxes, diamonds, the ship arrow and the radar sweep. */
export const SHAPE_VS = `${HEADER}${TRANSFORM}
layout(location = 0) in vec2 a_corner;   // -1..1
layout(location = 1) in vec2 a_center;
layout(location = 2) in vec2 a_half;
layout(location = 3) in float a_rot;
layout(location = 4) in float a_type;
layout(location = 5) in vec4 a_fill;
layout(location = 6) in vec4 a_stroke;
layout(location = 7) in vec4 a_p0;
layout(location = 8) in vec4 a_p1;
layout(location = 9) in vec4 a_glow;
out vec2 v_local;
flat out vec2 v_half;
flat out float v_type;
flat out vec4 v_fill;
flat out vec4 v_stroke;
flat out vec4 v_p0;
flat out vec4 v_p1;
flat out vec4 v_glow;
void main() {
  float px = 1.0 / (u_scale * u_dpr);
  vec2 ext = a_half + vec2(2.0 * px);
  vec2 local = a_corner * ext;
  float c = cos(a_rot);
  float s = sin(a_rot);
  vec2 world = a_center + vec2(c * local.x - s * local.y, s * local.x + c * local.y);
  gl_Position = toClip(world);
  v_local = local;
  v_half = a_half;
  v_type = a_type;
  v_fill = a_fill;
  v_stroke = a_stroke;
  v_p0 = a_p0;
  v_p1 = a_p1;
  v_glow = a_glow;
}
`;

export const SHAPE_FS = `${HEADER}${OUTPUTS}
in vec2 v_local;
flat in vec2 v_half;
flat in float v_type;
flat in vec4 v_fill;
flat in vec4 v_stroke;
flat in vec4 v_p0;
flat in vec4 v_p1;
flat in vec4 v_glow;
uniform float u_scale;
uniform float u_dpr;
uniform float u_alpha;
const float TAU = 6.28318530718;
float px;
float cov(float d) { return clamp(0.5 - d / px, 0.0, 1.0); }
float ndot(vec2 a, vec2 b) { return a.x * b.x - a.y * b.y; }
float sdHex(vec2 p, float r) {
  p = abs(p.yx);
  const vec3 k = vec3(-0.866025404, 0.5, 0.577350269);
  p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
  p -= vec2(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
float sdRoundBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}
float sdRhombus(vec2 p, vec2 b) {
  p = abs(p);
  float h = clamp((-2.0 * ndot(p, b) + ndot(b, b)) / dot(b, b), -1.0, 1.0);
  float d = length(p - 0.5 * b * vec2(1.0 - h, 1.0 + h));
  return d * sign(p.x * b.y + p.y * b.x - b.x * b.y);
}
float sdArrow(vec2 p, float s) {
  vec2 v[4] = vec2[4](vec2(9.0, 0.0) * s, vec2(-6.0, -6.5) * s, vec2(-2.5, 0.0) * s, vec2(-6.0, 6.5) * s);
  float d = dot(p - v[0], p - v[0]);
  float sgn = 1.0;
  for (int i = 0; i < 4; i++) {
    int j = (i + 3) % 4;
    vec2 e = v[j] - v[i];
    vec2 w = p - v[i];
    vec2 b = w - e * clamp(dot(w, e) / dot(e, e), 0.0, 1.0);
    d = min(d, dot(b, b));
    bvec3 c = bvec3(p.y >= v[i].y, p.y < v[j].y, e.x * w.y > e.y * w.x);
    if (all(c) || all(not(c))) sgn *= -1.0;
  }
  return sgn * sqrt(d);
}
float dashCov(float s, vec4 dash) {
  if (dash.x <= 0.0) return 1.0;
  float period = dash.x + dash.y;
  float m = mod(s + dash.z, period);
  float sd = m < dash.x ? -min(m, dash.x - m) : min(m - dash.x, period - m);
  return clamp(0.5 - sd / px, 0.0, 1.0);
}
void main() {
  px = 1.0 / (u_scale * u_dpr);
  vec2 p = v_local;
  int type = int(v_type + 0.5);
  float fillCov = 0.0;
  float strokeCov = 0.0;
  vec4 color = vec4(0.0);
  if (type == 0) {
    float r = v_p0.x;
    float w = v_p0.y;
    float len = length(p);
    fillCov = cov(len - r);
    float d = abs(len - r) - w * 0.5;
    float t = mod(atan(p.y, p.x) - v_p0.z, TAU);
    float span = v_p0.w;
    float along = 1.0;
    if (span > 0.0 && span < TAU - 0.0001) {
      if (v_p1.w > 0.5 && t > span) {
        vec2 e0 = r * vec2(cos(v_p0.z), sin(v_p0.z));
        vec2 e1 = r * vec2(cos(v_p0.z + span), sin(v_p0.z + span));
        d = min(length(p - e0), length(p - e1)) - w * 0.5;
      } else {
        float sd = t <= span ? -min(t, span - t) * len : min(t - span, TAU - t) * len;
        along = clamp(0.5 - sd / px, 0.0, 1.0);
      }
    }
    strokeCov = cov(d) * along * dashCov(t * r, vec4(v_p1.xyz, 0.0));
  } else if (type == 1) {
    float d = sdHex(p, v_p0.x * 0.866025404);
    fillCov = cov(d);
    strokeCov = cov(abs(d) - v_p0.y * 0.5);
  } else if (type == 2) {
    float d = sdRoundBox(p, v_p0.xy, v_p0.z);
    fillCov = cov(d);
    strokeCov = cov(abs(d) - v_p0.w * 0.5);
  } else if (type == 3) {
    fillCov = cov(sdRhombus(p, v_p0.xy));
  } else if (type == 4) {
    fillCov = cov(sdArrow(p, v_p0.x));
  } else if (type == 7) {
    // One slice of a ring: p1.w is the ring centre's distance along the
    // slice's middle angle; fragments outside the slice belong to a neighbour.
    vec2 q = p + vec2(v_p1.w, 0.0);
    float phi = atan(q.y, q.x);
    float halfSpan = v_p0.w * 0.5;
    if (phi < -halfSpan || phi >= halfSpan) discard;
    float len = length(q);
    float t = mod(v_p0.z + phi, TAU);
    strokeCov = cov(abs(len - v_p0.x) - v_p0.y * 0.5) * dashCov(t * v_p0.x, vec4(v_p1.xyz, 0.0));
  } else if (type == 6) {
    // The quad covers only the wedge; p0.zw is the ellipse centre relative to
    // the quad, p1.xy the ellipse radii.
    vec2 rel = p - v_p0.zw;
    vec2 q = rel / v_p1.xy;
    float l = length(q);
    float inside = clamp(0.5 - (l - 1.0) * min(v_p1.x, v_p1.y) / px, 0.0, 1.0);
    float diff = mod(v_p0.x - atan(q.y, q.x), TAU);
    float wedge = diff < v_p0.y ? 1.0 - diff / v_p0.y : 0.0;
    float edge = min(diff, TAU - diff) * length(rel);
    float lineCov = cov(edge - 0.5) * inside;
    vec4 f = vec4(v_fill.rgb, 1.0) * v_fill.a * wedge * inside;
    vec4 s = vec4(v_stroke.rgb, 1.0) * v_stroke.a * lineCov;
    color = s + f * (1.0 - s.a);
    emit(color * u_alpha, vec4(v_glow.rgb * v_glow.a * (lineCov * 0.5 + wedge * inside * 0.06), color.a) * u_alpha);
    return;
  }
  vec4 f = vec4(v_fill.rgb, 1.0) * v_fill.a * fillCov;
  vec4 s = vec4(v_stroke.rgb, 1.0) * v_stroke.a * strokeCov;
  color = s + f * (1.0 - s.a);
  // Stroked shapes glow from their rim, with only a little from the face, so
  // a node's halo surrounds it without washing out its interior.
  float filled = v_fill.a > 0.001 ? fillCov : 0.0;
  float g = v_stroke.a > 0.001 ? max(strokeCov, filled * 0.22) : filled;
  emit(color * u_alpha, vec4(v_glow.rgb * v_glow.a * g, color.a) * u_alpha);
}
`;

/** Instanced line segments with butt ends, dashes measured along the whole polyline, and hairline fading. */
export const LINE_VS = `${HEADER}${TRANSFORM}
layout(location = 0) in vec2 a_corner;  // x: 0..1 along, y: -1..1 across
layout(location = 1) in vec4 a_ends;    // x0, y0, x1, y1
layout(location = 2) in vec4 a_color;
layout(location = 3) in vec4 a_style;   // width, dash on, dash off, dash offset
layout(location = 4) in float a_dist;   // distance along the polyline at x0, y0
layout(location = 5) in vec4 a_glow;
out vec2 v_pos;
flat out vec4 v_color;
flat out vec4 v_style;
flat out float v_dist;
flat out vec4 v_glow;
void main() {
  float px = 1.0 / (u_scale * u_dpr);
  vec2 a = a_ends.xy;
  vec2 b = a_ends.zw;
  vec2 d = b - a;
  float len = length(d);
  vec2 dir = len > 0.00001 ? d / len : vec2(1.0, 0.0);
  vec2 n = vec2(-dir.y, dir.x);
  float hw = max(a_style.x, px) * 0.5 + px;
  vec2 world = a + dir * (a_corner.x * len) + n * (a_corner.y * hw);
  gl_Position = toClip(world);
  v_pos = vec2(a_corner.x * len, a_corner.y * hw);
  v_color = a_color;
  v_style = a_style;
  v_dist = a_dist;
  v_glow = a_glow;
}
`;

export const LINE_FS = `${HEADER}${OUTPUTS}
in vec2 v_pos;
flat in vec4 v_color;
flat in vec4 v_style;
flat in float v_dist;
flat in vec4 v_glow;
uniform float u_scale;
uniform float u_dpr;
uniform float u_alpha;
void main() {
  float px = 1.0 / (u_scale * u_dpr);
  float w = v_style.x;
  float wEff = max(w, px);
  float c = clamp(0.5 - (abs(v_pos.y) - wEff * 0.5) / px, 0.0, 1.0) * (w / wEff);
  if (v_style.y > 0.0) {
    float period = v_style.y + v_style.z;
    float m = mod(v_dist + v_pos.x + v_style.w, period);
    float sd = m < v_style.y ? -min(m, v_style.y - m) : min(m - v_style.y, period - m);
    c *= clamp(0.5 - sd / px, 0.0, 1.0);
  }
  float a = v_color.a * c;
  emit(vec4(v_color.rgb * a, a) * u_alpha, vec4(v_glow.rgb * v_glow.a * c, a) * u_alpha);
}
`;

/** Dispatch and report packets: a fading tail and a bright head, instanced. */
export const PACKET_VS = `${HEADER}${TRANSFORM}
layout(location = 0) in vec2 a_corner;  // x: 0..1 along, y: -1..1 across
layout(location = 1) in vec4 a_head;    // x, y, ux, uy
layout(location = 2) in vec4 a_tone;    // r, g, b, tail length
layout(location = 3) in float a_radius;
out vec2 v_pos;
flat out vec4 v_tone;
flat out float v_radius;
void main() {
  vec2 dir = a_head.zw;
  vec2 n = vec2(-dir.y, dir.x);
  float len = a_tone.w;
  float reach = a_radius * 2.5;
  float along = mix(-len, reach, a_corner.x);
  vec2 world = a_head.xy + dir * along + n * a_corner.y * reach;
  gl_Position = toClip(world);
  v_pos = vec2(along, a_corner.y * reach);
  v_tone = a_tone;
  v_radius = a_radius;
}
`;

export const PACKET_FS = `${HEADER}${OUTPUTS}
in vec2 v_pos;
flat in vec4 v_tone;
flat in float v_radius;
uniform float u_scale;
uniform float u_dpr;
uniform float u_alpha;
void main() {
  float px = 1.0 / (u_scale * u_dpr);
  float len = v_tone.w;
  float t = clamp((v_pos.x + len) / len, 0.0, 1.0);
  float tail = v_pos.x <= 0.0 ? clamp(0.5 - (abs(v_pos.y) - 1.0) / px, 0.0, 1.0) * 0.95 * t * t : 0.0;
  float head = clamp(0.5 - (length(v_pos) - v_radius) / px, 0.0, 1.0);
  float halo = exp(-dot(v_pos, v_pos) / (v_radius * v_radius * 1.6)) * 0.35;
  float a = max(head, max(tail, halo));
  emit(vec4(v_tone.rgb * a, a) * u_alpha, vec4(v_tone.rgb * (head * 1.4 + tail * 0.6 + halo), a) * u_alpha);
}
`;

/** The gate ring: two rings, 39 glyph ticks, nine chevrons that light in turn, and a faint event horizon. */
export const GATE_VS = `${HEADER}
layout(location = 0) in vec2 a_corner;
uniform vec2 u_res;
uniform float u_dpr;
uniform vec2 u_center;
uniform float u_radius;
out vec2 v_local;
void main() {
  vec2 local = a_corner * (u_radius * 1.1 + 14.0);
  vec2 clip = (u_center + local) * u_dpr / u_res * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_local = local;
}
`;

export const GATE_FS = `${HEADER}${OUTPUTS}
in vec2 v_local;
uniform float u_dpr;
uniform float u_radius;
uniform float u_progress;
uniform float u_amp;
uniform float u_rot;
uniform float u_time;
uniform float u_horizon;
uniform vec3 u_accent;
uniform vec3 u_line;
uniform vec3 u_strong;
const float TAU = 6.28318530718;
float px;
float cov(float d) { return clamp(0.5 - d / px, 0.0, 1.0); }
float sdSegment(vec2 p, vec2 a, vec2 b) {
  vec2 pa = p - a;
  vec2 ba = b - a;
  return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}
vec4 over(vec4 top, vec4 base) { return top + base * (1.0 - top.a); }
void main() {
  px = 1.0 / u_dpr;
  float R = u_radius;
  vec2 q = v_local;
  float len = length(q);
  // Without the event horizon, nothing inside the ring is drawn.
  if (u_horizon <= 0.0 && len < R * 0.95 - 2.0) discard;
  float ang = atan(q.y, q.x);
  vec4 col = vec4(0.0);
  vec3 glow = vec3(0.0);

  float inside = (1.0 - smoothstep(R * 0.15, R, len)) * step(len, R);
  float ripple = 0.5 + 0.5 * sin(len * 0.09 - u_time * 5.0 + sin(ang * 3.0 + u_time) * 0.6);
  float horizon = u_amp * inside * (0.35 + 0.65 * ripple) * u_horizon;
  col = over(vec4(u_accent, 1.0) * horizon, col);
  glow += u_accent * horizon * 0.6;

  float band = cov(abs(len - R * 1.04) - R * 0.035);
  col = over(vec4(u_line, 1.0) * 0.16 * u_amp * band, col);

  float seg = TAU / 39.0;
  float tickAngle = mod(ang + u_rot * 0.6, seg) - seg * 0.5;
  float tick = cov(abs(tickAngle * len) - 0.6) * step(R * 1.012, len) * step(len, R * 1.068);
  col = over(vec4(u_line, 1.0) * 0.55 * u_amp * tick, col);

  float ring1 = cov(abs(len - R) - 1.0) * 0.75 * u_amp;
  float ring2 = cov(abs(len - R * 1.08) - 0.5) * 0.3 * u_amp;
  col = over(vec4(u_accent, 1.0) * max(ring1, ring2), col);
  glow += u_accent * (ring1 * 0.7 + ring2 * 0.3);

  float step9 = TAU / 9.0;
  float k = floor((ang - u_rot) / step9 + 0.5);
  float ca = u_rot + k * step9;
  vec2 dir = vec2(cos(ca), sin(ca));
  vec2 tangent = vec2(-dir.y, dir.x);
  float s = clamp(R * 0.035, 5.0, 11.0);
  vec2 cp = vec2(dot(q, tangent), -(dot(q, dir) - R * 1.04));
  float d = min(sdSegment(cp, vec2(-s, -0.6 * s), vec2(0.0, 0.6 * s)), sdSegment(cp, vec2(0.0, 0.6 * s), vec2(s, -0.6 * s))) - 1.1;
  float index = mod(k, 9.0);
  bool lit = index / 9.0 < u_progress * 1.25;
  float chevron = cov(d) * u_amp;
  vec3 tone = lit ? u_accent : u_line;
  col = over(vec4(tone, 1.0) * chevron * (lit ? 0.95 : 0.6), col);
  if (lit) glow += mix(u_accent, u_strong, 0.25) * chevron * 1.4;

  emit(col, vec4(glow, col.a));
}
`;

/** Box downsample: four bilinear taps, a 4x4 texel average. */
export const DOWNSAMPLE_FS = `${HEADER}
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;
out vec4 o_color;
void main() {
  vec2 h = u_texel;
  vec3 c = texture(u_src, v_uv + vec2(-h.x, -h.y)).rgb + texture(u_src, v_uv + vec2(h.x, -h.y)).rgb
         + texture(u_src, v_uv + vec2(-h.x, h.y)).rgb + texture(u_src, v_uv + vec2(h.x, h.y)).rgb;
  o_color = vec4(c * 0.25, 1.0);
}
`;

/** Separable Gaussian blur, nine taps folded into five bilinear reads. */
export const BLUR_FS = `${HEADER}
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_step;
out vec4 o_color;
void main() {
  vec3 c = texture(u_src, v_uv).rgb * 0.2270270270;
  c += texture(u_src, v_uv + u_step * 1.3846153846).rgb * 0.3162162162;
  c += texture(u_src, v_uv - u_step * 1.3846153846).rgb * 0.3162162162;
  c += texture(u_src, v_uv + u_step * 3.2307692308).rgb * 0.0702702703;
  c += texture(u_src, v_uv - u_step * 3.2307692308).rgb * 0.0702702703;
  o_color = vec4(c, 1.0);
}
`;

/**
 * Bloom over the canvas, by blending: dark themes screen the light in
 * (ONE_MINUS_DST_COLOR, ONE); light themes tint toward it (DST_COLOR, ZERO),
 * so glows never blow out to white.
 */
export const COMPOSITE_FS = `${HEADER}
in vec2 v_uv;
uniform sampler2D u_near;
uniform sampler2D u_far;
uniform float u_nearWeight;
uniform float u_farWeight;
uniform float u_strength;
uniform float u_dark;
out vec4 o_color;
void main() {
  vec3 b = (texture(u_near, v_uv).rgb * u_nearWeight + texture(u_far, v_uv).rgb * u_farWeight) * u_strength;
  if (u_dark > 0.5) {
    o_color = vec4(clamp(b, 0.0, 1.0), 1.0);
  } else {
    float m = max(b.r, max(b.g, b.b));
    vec3 tint = m > 0.0001 ? b / m : vec3(1.0);
    float a = clamp(m * 0.55, 0.0, 0.4);
    o_color = vec4(vec3(1.0 - a) + tint * a, 1.0);
  }
}
`;
