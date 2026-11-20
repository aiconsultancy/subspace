// The WebGL2 renderer. It draws a composed frame (see scene/index.js) in two
// passes over the same instance data:
//
// 1. Colour, straight into the canvas at the quality level's internal
//    resolution (at most 2x, adaptive): background, instanced stars, the tier
//    layers as batches of SDF shapes, line segments and packets, and the gate.
// 2. Emissive light, into a small target at a quarter or a sixteenth of the
//    CSS pixel count, then a separable blur at one or two scales, blended over
//    the canvas.
//
// There is no full-resolution offscreen target, so a frame at DPR 2 moves a
// fraction of the bytes it used to. A lost context waits for restoration
// before giving up to the Canvas 2D renderer.

import { TONES } from "../theme.js";
import { compileProgram, createTarget, deleteTarget } from "./util.js";
import { createBatcher, createInstances } from "./batches.js";
import { createGpuTimer } from "../perf.js";
import * as S from "./shaders.js";

const STAR_FLOATS = 3;
export const RESTORE_WAIT_MS = 3000;
const DEFAULT_QUALITY = { scale: 2, bloom: 2, horizon: true };

function attrib(gl, location, size, stride, offset, divisor) {
  gl.enableVertexAttribArray(location);
  gl.vertexAttribPointer(location, size, gl.FLOAT, false, stride, offset);
  gl.vertexAttribDivisor(location, divisor);
}

export function createGlRenderer(canvas, { onLost, onRestored } = {}) {
  const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false, stencil: false, premultipliedAlpha: true, powerPreference: "default" });
  if (!gl) return null;
  let impl;
  try {
    impl = setup(gl, canvas);
  } catch (error) {
    console.warn("Fleet Console: WebGL2 setup failed, using Canvas 2D.", error);
    // Release whatever was created; the caller draws in 2D on a fresh canvas.
    gl.getExtension("WEBGL_lose_context")?.loseContext();
    return null;
  }
  // Removed on dispose, so a replaced renderer ignores its canvas's events.
  const listening = new AbortController();
  let lost = false;
  let lostTimer = 0;
  let size = { cssW: 1, cssH: 1, dpr: 1 };
  let quality = { ...DEFAULT_QUALITY };
  let palette = null;
  let timerResult = null;
  let gpuTimer = null;

  canvas.addEventListener(
    "webglcontextlost",
    (event) => {
      event.preventDefault();
      lost = true;
      clearTimeout(lostTimer);
      lostTimer = setTimeout(() => {
        if (lost) onLost?.();
      }, RESTORE_WAIT_MS);
    },
    { signal: listening.signal },
  );
  canvas.addEventListener(
    "webglcontextrestored",
    () => {
      try {
        impl = setup(gl, canvas);
        impl.configure(size, quality);
        if (palette) impl.setPalette(palette);
        if (timerResult) gpuTimer = createGpuTimer(gl, timerResult);
        lost = false;
        clearTimeout(lostTimer);
        onRestored?.();
      } catch {
        onLost?.();
      }
    },
    { signal: listening.signal },
  );

  return {
    kind: "webgl2",
    get lost() {
      return lost || gl.isContextLost();
    },
    /** The drawing buffer's size in pixels and its ratio to CSS pixels. */
    get pixels() {
      return impl.pixels();
    },
    get gpuTiming() {
      return Boolean(gpuTimer);
    },
    setPalette(next) {
      palette = next;
      impl.setPalette(next);
    },
    resize(cssW, cssH, dpr) {
      size = { cssW, cssH, dpr };
      if (!this.lost) impl.configure(size, quality);
    },
    /** {scale, bloom: 0|1|2, horizon} from the quality level. */
    setQuality(next) {
      quality = { ...quality, ...next };
      if (!this.lost) impl.configure(size, quality);
    },
    /** Report GPU time per frame through `onResult(ms)`, where the engine allows it. */
    enableGpuTimer(onResult) {
      timerResult = onResult;
      gpuTimer = createGpuTimer(gl, onResult);
      return Boolean(gpuTimer);
    },
    /** `options.reuseBloom` blends the previous frame's light instead of recomputing it. */
    render(frame, stars, motion, t, options = {}) {
      if (this.lost) return;
      gpuTimer?.poll();
      gpuTimer?.begin();
      impl.render(frame, stars, motion, t, quality, options);
      gpuTimer?.end();
    },
    dispose() {
      listening.abort();
      clearTimeout(lostTimer);
      try {
        if (!gl.isContextLost()) impl.dispose();
      } catch {
        // A context that is already gone has nothing left to delete.
      }
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}

function setup(gl, canvas) {
  const halfFloat = Boolean(gl.getExtension("EXT_color_buffer_float"));
  const programs = {
    background: compileProgram(gl, S.FULLSCREEN_VS, S.BACKGROUND_FS),
    stars: compileProgram(gl, S.STARS_VS, S.STARS_FS),
    shape: compileProgram(gl, S.SHAPE_VS, S.SHAPE_FS),
    line: compileProgram(gl, S.LINE_VS, S.LINE_FS),
    packet: compileProgram(gl, S.PACKET_VS, S.PACKET_FS),
    gate: compileProgram(gl, S.GATE_VS, S.GATE_FS),
    down: compileProgram(gl, S.FULLSCREEN_VS, S.DOWNSAMPLE_FS),
    blur: compileProgram(gl, S.FULLSCREEN_VS, S.BLUR_FS),
    composite: compileProgram(gl, S.FULLSCREEN_VS, S.COMPOSITE_FS),
  };

  const staticBuffer = (values) => {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(values), gl.STATIC_DRAW);
    return buffer;
  };
  const centred = staticBuffer([-1, -1, 1, -1, -1, 1, 1, 1]);
  const along = staticBuffer([0, -1, 1, -1, 0, 1, 1, 1]);

  const instances = { shape: createInstances(26), line: createInstances(17), packet: createInstances(9), stars: createInstances(STAR_FLOATS) };
  const glBuffers = { shape: gl.createBuffer(), line: gl.createBuffer(), packet: gl.createBuffer(), stars: gl.createBuffer() };

  function vao(corners, kind, layout) {
    const object = gl.createVertexArray();
    gl.bindVertexArray(object);
    gl.bindBuffer(gl.ARRAY_BUFFER, corners);
    attrib(gl, 0, 2, 0, 0, 0);
    const stride = (instances[kind]?.stride ?? 1) * 4;
    // Enabling and divisors are VAO state, set once; a batch only re-points.
    if (layout.length) {
      gl.bindBuffer(gl.ARRAY_BUFFER, glBuffers[kind]);
      for (const [location, count, at] of layout) attrib(gl, location, count, stride, at * 4, 1);
    }
    let bound = 0;
    const bind = (offset) => {
      if (offset === bound) return;
      gl.bindBuffer(gl.ARRAY_BUFFER, glBuffers[kind]);
      for (const [location, count, at] of layout) gl.vertexAttribPointer(location, count, gl.FLOAT, false, stride, offset + at * 4);
      bound = offset;
    };
    gl.bindVertexArray(null);
    return { object, bind };
  }
  const vaos = {
    shape: vao(centred, "shape", [[1, 2, 0], [2, 2, 2], [3, 1, 4], [4, 1, 5], [5, 4, 6], [6, 4, 10], [7, 4, 14], [8, 4, 18], [9, 4, 22]]),
    line: vao(along, "line", [[1, 4, 0], [2, 4, 4], [3, 4, 8], [4, 1, 12], [5, 4, 13]]),
    packet: vao(along, "packet", [[1, 4, 0], [2, 4, 4], [3, 1, 8]]),
    stars: vao(along, "stars", [[1, 3, 0]]),
    gate: vao(centred, "packet", []),
  };
  const emptyVao = gl.createVertexArray();

  let cssW = 1;
  let cssH = 1;
  let scale = 1;
  let width = 1;
  let height = 1;
  let targets = null;
  let targetKey = "";

  function makeTargets() {
    const key = `${cssW}x${cssH}`;
    if (key === targetKey && targets) return;
    for (const target of Object.values(targets ?? {})) deleteTarget(gl, target);
    // Bloom targets are small, so they may keep half floats for soft highlights.
    const make = (w, h) => (halfFloat && createTarget(gl, w, h, gl.RGBA16F)) || createTarget(gl, w, h, gl.RGBA8);
    const nearW = Math.max(1, Math.round(cssW / 2));
    const nearH = Math.max(1, Math.round(cssH / 2));
    const farW = Math.max(1, Math.round(cssW / 4));
    const farH = Math.max(1, Math.round(cssH / 4));
    targets = { near: make(nearW, nearH), nearTmp: make(nearW, nearH), far: make(farW, farH), farTmp: make(farW, farH) };
    if (Object.values(targets).some((target) => !target)) throw new Error("Framebuffers are not supported.");
    targetKey = key;
  }

  let palette = null;
  let colors = {};
  const rgba = (spec) => {
    if (!spec) return [0, 0, 0, 0];
    const c = colors[spec[0]] ?? colors.muted;
    return [c[0], c[1], c[2], spec[1]];
  };
  const glowOf = (spec) => {
    if (!spec) return [0, 0, 0, 0];
    const c = colors[spec[0]] ?? colors.accent;
    return [c[0], c[1], c[2], spec[1] * (palette.dark ? 1 : 0.55)];
  };
  const build = createBatcher(instances, { rgba, glowOf, rgb: (tone) => colors[tone] ?? colors.accent });

  function upload(kind) {
    const buffer = instances[kind];
    gl.bindBuffer(gl.ARRAY_BUFFER, glBuffers[kind]);
    gl.bufferData(gl.ARRAY_BUFFER, buffer.data.subarray(0, Math.max(1, buffer.count) * buffer.stride), gl.DYNAMIC_DRAW);
  }

  /** A pass's destination: the canvas or a bloom target, with its pixel ratio. */
  const canvasPass = () => ({ framebuffer: null, w: width, h: height, dpr: scale, emissive: false });
  const targetPass = (target) => ({ framebuffer: target.framebuffer, w: target.width, h: target.height, dpr: target.width / cssW, emissive: true });

  function bindPass(pass) {
    gl.bindFramebuffer(gl.FRAMEBUFFER, pass.framebuffer);
    gl.viewport(0, 0, pass.w, pass.h);
  }

  function common(program, pass) {
    const u = program.uniforms;
    gl.useProgram(program.program);
    gl.uniform2f(u.u_res, pass.w, pass.h);
    gl.uniform1f(u.u_dpr, pass.dpr);
    if (u.u_pass) gl.uniform1f(u.u_pass, pass.emissive ? 1 : 0);
    return u;
  }

  function drawBackground(frame, pass) {
    const u = common(programs.background, pass);
    gl.uniform3fv(u.u_stage, colors.stage);
    gl.uniform3fv(u.u_accent, colors.accent);
    gl.uniform2f(u.u_vp, frame.vp.x * pass.dpr, frame.vp.y * pass.dpr);
    gl.uniform1f(u.u_vignette, palette.dark ? 0.06 : 0.035);
    gl.uniform1f(u.u_tunnel, (palette.dark ? 0.16 : 0.09) * frame.tunnel);
    gl.bindVertexArray(emptyVao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  function uploadStars(stars) {
    const buffer = instances.stars;
    buffer.count = 0;
    buffer.reserve(stars.length);
    for (let i = 0; i < stars.length; i += 1) {
      buffer.data[i * 3] = stars[i].x;
      buffer.data[i * 3 + 1] = stars[i].y;
      buffer.data[i * 3 + 2] = stars[i].z;
    }
    buffer.count = stars.length;
    upload("stars");
  }

  function drawStars(frame, motion, pass) {
    const streaking = motion.streak > 0.004;
    const u = common(programs.stars, pass);
    gl.uniform2f(u.u_vp, frame.vp.x, frame.vp.y);
    gl.uniform1f(u.u_focal, Math.max(cssW, cssH) * 0.5);
    gl.uniform1f(u.u_streak, motion.streak);
    gl.uniform1f(u.u_dir, motion.speed < 0 ? -1 : 1);
    gl.uniform1f(u.u_bright, palette.dark ? 0.7 : 0.4);
    gl.uniform3fv(u.u_color, colors[palette.dark ? "strong" : streaking ? "accent" : "muted"]);
    gl.uniform1f(u.u_glow, palette.dark ? (streaking ? 0.7 : 0.12) : 0);
    gl.bindVertexArray(vaos.stars.object);
    vaos.stars.bind(0);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, instances.stars.count);
  }

  function drawBatches(frame, batches, pass) {
    for (const batch of batches) {
      // Lines without glow add nothing to the light; they are skipped there.
      if (pass.emissive && batch.kind === "line" && !batch.glow) continue;
      const program = programs[batch.kind];
      const layer = frame.layers[batch.layer];
      const u = common(program, pass);
      gl.uniform1f(u.u_scale, layer.transform.s);
      gl.uniform2f(u.u_trans, layer.transform.tx, layer.transform.ty);
      gl.uniform1f(u.u_alpha, layer.alpha);
      gl.bindVertexArray(vaos[batch.kind].object);
      vaos[batch.kind].bind(batch.start * instances[batch.kind].stride * 4);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, batch.count);
    }
  }

  function drawGate(gate, t, pass, horizon) {
    const u = common(programs.gate, pass);
    gl.uniform2f(u.u_center, gate.x, gate.y);
    gl.uniform1f(u.u_radius, gate.r);
    gl.uniform1f(u.u_progress, gate.p);
    gl.uniform1f(u.u_amp, gate.amp);
    gl.uniform1f(u.u_rot, t / 1600);
    gl.uniform1f(u.u_time, t / 1000);
    gl.uniform1f(u.u_horizon, horizon ? (palette.dark ? 0.05 : 0.03) : 0);
    gl.uniform3fv(u.u_accent, colors.accent);
    gl.uniform3fv(u.u_line, colors.line);
    gl.uniform3fv(u.u_strong, colors.strong);
    gl.bindVertexArray(vaos.gate.object);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  function drawScene(frame, batches, motion, t, pass, quality) {
    bindPass(pass);
    gl.disable(gl.BLEND);
    if (pass.emissive && frame.tunnel <= 0.001) {
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    } else {
      drawBackground(frame, pass);
    }
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    if (!pass.emissive || (palette.dark && motion.streak > 0.004)) drawStars(frame, motion, pass);
    drawBatches(frame, batches, pass);
    if (frame.gate && frame.gate.amp > 0.001) drawGate(frame.gate, t, pass, quality.horizon);
  }

  function fullscreen(program, target, setUniforms) {
    gl.bindFramebuffer(gl.FRAMEBUFFER, target ? target.framebuffer : null);
    gl.viewport(0, 0, target ? target.width : width, target ? target.height : height);
    gl.useProgram(program.program);
    setUniforms(program.uniforms);
    gl.bindVertexArray(emptyVao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  function bindTexture(unit, texture, location) {
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.uniform1i(location, unit);
  }

  function blur(source, temp, spread) {
    fullscreen(programs.blur, temp, (u) => {
      bindTexture(0, source.textures[0], u.u_src);
      gl.uniform2f(u.u_step, spread / source.width, 0);
    });
    fullscreen(programs.blur, source, (u) => {
      bindTexture(0, temp.textures[0], u.u_src);
      gl.uniform2f(u.u_step, 0, spread / source.height);
    });
  }

  // What the stored light was made from; reuse is allowed only while it matches.
  let lightKey = "";

  function bloom(frame, batches, motion, t, quality, reuse) {
    const { near, nearTmp, far, farTmp } = targets;
    const two = quality.bloom >= 2;
    const key = `${quality.bloom}|${targetKey}|${palette.dark}|${colors.accent}`;
    if (!reuse || key !== lightKey) {
      drawScene(frame, batches, motion, t, targetPass(two ? near : far), quality);
      gl.disable(gl.BLEND);
      if (two) {
        blur(near, nearTmp, 1);
        fullscreen(programs.down, far, (u) => {
          bindTexture(0, near.textures[0], u.u_src);
          gl.uniform2f(u.u_texel, 0.5 / far.width, 0.5 / far.height);
        });
      }
      blur(far, farTmp, 1);
      blur(far, farTmp, 2);
      lightKey = key;
    }
    gl.enable(gl.BLEND);
    if (palette.dark) gl.blendFunc(gl.ONE_MINUS_DST_COLOR, gl.ONE);
    else gl.blendFunc(gl.DST_COLOR, gl.ZERO);
    fullscreen(programs.composite, null, (u) => {
      bindTexture(0, (two ? near : far).textures[0], u.u_near);
      bindTexture(1, far.textures[0], u.u_far);
      gl.uniform1f(u.u_nearWeight, two ? 0.9 : 0);
      gl.uniform1f(u.u_farWeight, two ? 1.3 : 1.75);
      gl.uniform1f(u.u_strength, palette.dark ? 0.85 : 0.8);
      gl.uniform1f(u.u_dark, palette.dark ? 1 : 0);
    });
    gl.disable(gl.BLEND);
  }

  return {
    pixels: () => ({ width, height, scale }),
    setPalette(next) {
      palette = next;
      colors = {};
      for (const tone of [...TONES, "stage"]) {
        const rgb = next.rgb[tone] ?? [128, 128, 128];
        colors[tone] = [rgb[0] / 255, rgb[1] / 255, rgb[2] / 255];
      }
    },
    configure(size, quality) {
      cssW = Math.max(1, Math.round(size.cssW));
      cssH = Math.max(1, Math.round(size.cssH));
      scale = Math.max(1, Math.min(size.dpr, quality.scale));
      const nextW = Math.max(1, Math.round(cssW * scale));
      const nextH = Math.max(1, Math.round(cssH * scale));
      if (nextW !== width || nextH !== height || canvas.width !== nextW) {
        width = nextW;
        height = nextH;
        canvas.width = width;
        canvas.height = height;
      }
      makeTargets();
      lightKey = "";
    },
    render(frame, stars, motion, t, quality, options) {
      if (!palette || !targets) return;
      const batches = build(frame.layers);
      for (const kind of ["shape", "line", "packet"]) upload(kind);
      uploadStars(stars);
      drawScene(frame, batches, motion, t, canvasPass(), quality);
      if (quality.bloom > 0) bloom(frame, batches, motion, t, quality, Boolean(options.reuseBloom));
      else lightKey = "";
      gl.bindVertexArray(null);
    },
    dispose() {
      for (const target of Object.values(targets ?? {})) deleteTarget(gl, target);
      targets = null;
    },
  };
}
