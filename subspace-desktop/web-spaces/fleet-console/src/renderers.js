// Choosing the scene renderer, and switching to Canvas 2D when WebGL2 fails.
// A canvas keeps the first kind of context it hands out, so once WebGL2 has
// touched it (a setup failure, a lost context) Canvas 2D needs a fresh
// canvas. The factories are passed in, so this runs in tests without a DOM.

/**
 * @param {object} options
 * @param {HTMLCanvasElement} options.canvas
 * @param {boolean} options.preferGl                 try WebGL2 first
 * @param {(canvas) => object|null} options.createGl
 * @param {(canvas) => object|null} options.createCanvas
 * @param {(canvas) => HTMLCanvasElement} options.replaceCanvas  puts a fresh canvas in its place
 * @returns {{renderer: object, canvas: HTMLCanvasElement}}
 */
export function createSceneRenderer({ canvas, preferGl, createGl, createCanvas, replaceCanvas }) {
  if (preferGl) {
    const renderer = createGl(canvas);
    if (renderer) return { renderer, canvas };
  }
  const renderer = createCanvas(canvas);
  if (renderer) return { renderer, canvas };
  // WebGL2 took the canvas before giving up: draw in 2D on a new one.
  const fresh = replaceCanvas(canvas);
  return { renderer: createCanvas(fresh), canvas: fresh };
}

/** Give up on a WebGL2 renderer: dispose of it and draw in 2D on a fresh canvas. */
export function canvasFallback({ renderer, canvas, createCanvas, replaceCanvas }) {
  renderer?.dispose?.();
  const fresh = replaceCanvas(canvas);
  return { renderer: createCanvas(fresh), canvas: fresh };
}
