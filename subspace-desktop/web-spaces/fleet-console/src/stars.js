// The star field: points in a unit cube flying toward (or away from) the
// vanishing point. The warp speed is the camera's zoom velocity, so streaks
// stretch while the camera dives and reverse when it pulls out.

export function createStars(count, random = Math.random) {
  return Array.from({ length: count }, () => ({ x: random() * 2 - 1, y: random() * 2 - 1, z: random() * 0.95 + 0.05 }));
}

/** Move the stars for `dt` seconds at zoom velocity `velocity`; returns the speed and streak length used. */
export function stepStars(stars, dt, velocity, reduced, random = Math.random) {
  const speed = reduced ? 0 : 0.02 + velocity * 0.5;
  const streak = reduced ? 0 : Math.min(0.45, Math.abs(velocity) * 0.05);
  for (const star of stars) {
    star.z -= speed * dt;
    if (star.z < 0.05) {
      star.x = random() * 2 - 1;
      star.y = random() * 2 - 1;
      star.z = 1;
    } else if (star.z > 1) {
      star.x = random() * 2 - 1;
      star.y = random() * 2 - 1;
      star.z = 0.06 + random() * 0.25;
    }
  }
  return { speed, streak };
}

/** Screen position of a star at depth `z` for vanishing point `vp` and focal length `f`. */
export function projectStar(star, z, vp, f) {
  return { x: vp.x + (star.x / z) * f, y: vp.y + (star.y / z) * f };
}
