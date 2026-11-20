import assert from "node:assert/strict";
import { test } from "node:test";
import { createGlRenderer } from "../src/gl/renderer.js";
import { createCanvasRenderer } from "../src/canvas2d.js";
import { canvasFallback, createSceneRenderer } from "../src/renderers.js";

/**
 * A WebGL2 context that answers every call: methods return a fresh object,
 * constants are numbers. `compiles: false` makes every shader fail.
 */
function fakeGl({ compiles = true, onLose = () => {} } = {}) {
  const known = {
    isContextLost: () => false,
    getShaderParameter: () => compiles,
    getProgramParameter: (program, name) => (name === "ACTIVE_UNIFORMS" ? 0 : true),
    getShaderInfoLog: () => "fake compile error",
    getExtension: (name) => (name === "WEBGL_lose_context" ? { loseContext: onLose } : null),
    checkFramebufferStatus: () => "FRAMEBUFFER_COMPLETE",
    FRAMEBUFFER_COMPLETE: "FRAMEBUFFER_COMPLETE",
    ACTIVE_UNIFORMS: "ACTIVE_UNIFORMS",
  };
  return new Proxy(known, {
    get(target, prop) {
      if (prop in target) return target[prop];
      if (typeof prop !== "string") return undefined;
      return /^[A-Z0-9_]+$/.test(prop) ? 1 : () => ({});
    },
  });
}

/** A canvas that, like a real one, keeps the first kind of context it gave out. */
class FakeCanvas extends EventTarget {
  constructor(gl) {
    super();
    this.gl = gl;
    this.mode = null;
    this.replacedBy = null;
    this.style = {};
  }
  getContext(kind) {
    if (this.mode && this.mode !== kind) return null;
    if (kind === "webgl2" && !this.gl) return null;
    this.mode = kind;
    return kind === "webgl2" ? this.gl : { canvas: this };
  }
}

const replaceCanvas = (canvas) => {
  canvas.replacedBy = new FakeCanvas(null);
  return canvas.replacedBy;
};

test("a WebGL2 setup failure releases the context and still ends on a working Canvas 2D renderer", () => {
  let lost = 0;
  const canvas = new FakeCanvas(fakeGl({ compiles: false, onLose: () => (lost += 1) }));
  assert.equal(createGlRenderer(canvas), null, "setup fails");
  assert.equal(lost, 1, "the partly set-up context is released");
  assert.equal(createCanvasRenderer(canvas), null, "the same canvas cannot give a 2D context any more");

  const fresh = new FakeCanvas(fakeGl({ compiles: false }));
  const picked = createSceneRenderer({ canvas: fresh, preferGl: true, createGl: (c) => createGlRenderer(c), createCanvas: createCanvasRenderer, replaceCanvas });
  assert.equal(picked.renderer.kind, "canvas2d");
  assert.equal(picked.canvas, fresh.replacedBy, "Canvas 2D draws on a fresh canvas");
});

test("without WebGL2 at all, Canvas 2D uses the original canvas", () => {
  const canvas = new FakeCanvas(null);
  const picked = createSceneRenderer({ canvas, preferGl: true, createGl: (c) => createGlRenderer(c), createCanvas: createCanvasRenderer, replaceCanvas });
  assert.equal(picked.renderer.kind, "canvas2d");
  assert.equal(picked.canvas, canvas);
});

test("falling back after a lost context disposes the WebGL2 renderer and ignores its later events", () => {
  let lost = 0;
  let restored = 0;
  const canvas = new FakeCanvas(fakeGl({ onLose: () => (lost += 1) }));
  const gl = createGlRenderer(canvas, { onLost: () => {}, onRestored: () => (restored += 1) });
  assert.equal(gl.kind, "webgl2");
  const next = canvasFallback({ renderer: gl, canvas, createCanvas: createCanvasRenderer, replaceCanvas });
  assert.equal(next.renderer.kind, "canvas2d");
  assert.equal(next.canvas, canvas.replacedBy);
  assert.equal(lost, 1, "the old context is released");
  canvas.dispatchEvent(new Event("webglcontextrestored"));
  assert.equal(restored, 0, "a restore after the switch does nothing");
});
