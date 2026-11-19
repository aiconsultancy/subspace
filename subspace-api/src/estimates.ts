import { Schema } from "effect";
import { estimateScales } from "./schema";

export const EstimateScale = Schema.Literals(estimateScales);
type Scale = typeof EstimateScale.Type;
export const estimatePoints: Record<Scale, readonly number[]> = {
  notUsed: [],
  fibonacci: [1, 2, 3, 5, 8],
  exponential: [1, 2, 4, 8, 16],
  linear: [1, 2, 3, 4, 5],
  tShirt: [1, 2, 3, 5, 8],
};

export function nearestEstimate(value: number, scale: Scale) {
  return [...estimatePoints[scale]].sort(
    (a, b) => Math.abs(value - a) - Math.abs(value - b) || b - a,
  )[0];
}
