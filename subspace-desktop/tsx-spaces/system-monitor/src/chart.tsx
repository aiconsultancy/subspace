// A history graph: one line per series over the last CAPACITY samples,
// right-aligned so the newest sample is at the right edge. Colors come from
// the theme's tokens, so the graph follows the Subspace theme.
import { Canvas, token, type Tokens } from "nsh";
import { CAPACITY, type Series } from "./model";

export interface GraphProps {
  series: () => Series[];
  colors: (keyof Tokens)[];
  /** The value at the top; `"peak"` scales to the largest sample. */
  max: number | "peak";
  label: string;
  testID?: string;
  height?: number;
}

/** The value at the top of a graph. */
export function top(series: Series[], max: number | "peak"): number {
  if (max !== "peak") return max;
  const values = series.flat().filter((v): v is number => v !== null);
  return Math.max(1, ...values) * 1.1;
}

export function Graph(props: GraphProps) {
  return (
    <Canvas
      testID={props.testID}
      accessibilityLabel={props.label}
      class="w-full rounded-(--radius) border border-border bg-card"
      style={{ height: props.height ?? 56 }}
      draw={(cx, { width, height }) => {
        const series = props.series();
        const ceiling = top(series, props.max);
        const step = width / (CAPACITY - 1);
        cx.strokeStyle = token("border");
        cx.lineWidth = 1;
        cx.beginPath();
        cx.moveTo(0, height / 2);
        cx.lineTo(width, height / 2);
        cx.stroke();
        series.forEach((values, index) => {
          cx.strokeStyle = token(props.colors[index % props.colors.length]!) as string;
          cx.lineWidth = 1.5;
          cx.beginPath();
          let drawing = false;
          const offset = CAPACITY - values.length;
          values.forEach((value, i) => {
            if (value === null) {
              drawing = false;
              return;
            }
            const x = (offset + i) * step;
            const y = height - 2 - (Math.min(value, ceiling) / ceiling) * (height - 4);
            if (drawing) cx.lineTo(x, y);
            else cx.moveTo(x, y);
            drawing = true;
          });
          cx.stroke();
        });
      }}
    />
  );
}
