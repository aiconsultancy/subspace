import { writeFile } from "node:fs/promises";
import { extname } from "node:path";
import { text } from "node:stream/consumers";
import { renderChart } from "flint-chart-mcp/render";

const [output, backend = "vegalite", ...extra] = process.argv.slice(2);
const format = output && extname(output).slice(1);
if (!["png", "svg"].includes(format) || extra.length > 0) {
    console.error("Usage: flint-render OUTPUT.png|OUTPUT.svg [vegalite|echarts|chartjs] < CHART.json");
    process.exit(2);
}

try {
    const input = JSON.parse(await text(process.stdin));
    const { buffer, svg } = await renderChart(input, backend, { format, disableFileReference: true });
    await writeFile(output, format === "png" ? buffer : svg, { flag: "wx", mode: 0o600 });
} catch (error) {
    console.error(`flint-render: ${error.message}`);
    process.exitCode = 1;
}
