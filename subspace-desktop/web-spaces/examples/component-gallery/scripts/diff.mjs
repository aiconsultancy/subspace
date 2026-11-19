import pixelmatch from 'pixelmatch';

export function compareImages(reference, actual) {
  const { width, height } = reference;
  if (width !== actual.width || height !== actual.height) {
    return { pass: false, error: 'Raster dimensions differ; no resize or crop is allowed.' };
  }
  const difference = Buffer.alloc(width * height * 4);
  const changedPixels = pixelmatch(reference.data, actual.data, difference, width, height, {
    threshold: 0,
    includeAA: false,
  });
  const exactChangedPixels = pixelmatch(reference.data, actual.data, null, width, height, {
    threshold: 0,
    includeAA: true,
  });
  return {
    pass: changedPixels === 0,
    changedPixels,
    exactChangedPixels,
    ignoredEdgePixels: exactChangedPixels - changedPixels,
    changedPercent: 100 * changedPixels / (width * height),
    difference,
  };
}
