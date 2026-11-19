// Builds the decorative particle layers and switches scenes. Positions come
// from a fixed-seed generator so the scene looks the same on every load.

function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 2 ** 32;
  };
}

function fill(container, count, seed, build) {
  const next = random(seed);
  const items = [];
  for (let i = 0; i < count; i += 1) {
    const element = document.createElement("i");
    build(element, next, i);
    items.push(element);
  }
  container.replaceChildren(...items);
}

const vars = (element, values) => {
  for (const [name, value] of Object.entries(values)) element.style.setProperty(name, value);
};

// Constant markup, no data is interpolated.
const CLOUD_SVG = '<svg viewBox="0 0 240 120" focusable="false"><use href="#cloud-shape"/></svg>';

/** Fill the star, cloud, rain and snow layers once. */
export function buildScene(root = document) {
  fill(root.getElementById("stars"), 70, 11, (star, next) => {
    vars(star, {
      "--x": `${(next() * 100).toFixed(2)}%`,
      "--y": `${(next() * 62).toFixed(2)}%`,
      "--size": `${(1 + next() * 2).toFixed(2)}px`,
      "--delay": `${(-next() * 6).toFixed(2)}s`,
      "--dur": `${(3 + next() * 4).toFixed(2)}s`,
    });
  });

  const clouds = root.getElementById("clouds");
  fill(clouds, 8, 29, (cloud, next, index) => {
    cloud.className = "cloud";
    cloud.dataset.tier = String(index < 3 ? 1 : index < 6 ? 2 : 3);
    cloud.innerHTML = CLOUD_SVG;
    const place = next();
    const duration = 90 + next() * 90;
    vars(cloud, {
      "--x": place.toFixed(3),
      "--y": `${(4 + (index % 4) * 11 + next() * 8).toFixed(1)}%`,
      "--width": `${(16 + next() * 16).toFixed(1)}rem`,
      "--dur": `${duration.toFixed(0)}s`,
      "--delay": `${(-place * duration).toFixed(1)}s`,
      "--fade": (0.55 + next() * 0.45).toFixed(2),
    });
  });

  fill(root.getElementById("rain"), 72, 47, (drop, next) => {
    vars(drop, {
      "--x": `${(next() * 112 - 6).toFixed(2)}%`,
      "--y": `${(next() * 100).toFixed(1)}`,
      "--len": `${(26 + next() * 34).toFixed(0)}px`,
      "--dur": `${(0.75 + next() * 0.6).toFixed(2)}s`,
      "--delay": `${(-next() * 2).toFixed(2)}s`,
    });
  });

  fill(root.getElementById("snow"), 64, 83, (flake, next) => {
    vars(flake, {
      "--x": `${(next() * 100).toFixed(2)}%`,
      "--y": `${(next() * 100).toFixed(1)}`,
      "--size": `${(3 + next() * 6).toFixed(1)}px`,
      "--dur": `${(9 + next() * 9).toFixed(1)}s`,
      "--delay": `${(-next() * 14).toFixed(1)}s`,
      "--sway": `${(next() * 14 - 7).toFixed(1)}vh`,
    });
  });
}

/** Switch the page to a scene. CSS handles the cross-fade. */
export function applyScene(scene, flag = "", body = document.body) {
  body.dataset.kind = scene.kind;
  body.dataset.time = scene.time;
  body.dataset.intensity = String(scene.intensity ?? 0);
  body.dataset.flag = flag; // "synthetic", "stale" or ""
}
