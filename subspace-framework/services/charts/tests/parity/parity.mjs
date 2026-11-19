// Generate the parity fixture Subspace's QuickJS compile is checked against.
// Synthetic cost rows; no real data.
import { assembleChartjs } from 'flint-chart';
import { writeFileSync } from 'node:fs';

const streams = ['platform', 'data', 'web', 'api'];
const values = [];
for (let day = 1; day <= 7; day += 1) {
  streams.forEach((stream, index) => {
    values.push({
      date: `2026-09-0${day}`,
      stream,
      environment: index % 2 === 0 ? 'prd' : 'qat',
      cost_usd: Math.round((120 + index * 37 + day * 11.5) * 100) / 100,
    });
  });
}

const input = {
  data: { values },
  semantic_types: {
    date: 'Date',
    stream: 'Category',
    environment: 'Category',
    cost_usd: 'Price',
  },
  chart_spec: {
    chartType: 'Stacked Bar Chart',
    title: 'Daily cost by work stream',
    subtitle: 'synthetic rows, amortized USD per day',
    encodings: {
      x: { field: 'date' },
      y: { field: 'cost_usd' },
      color: { field: 'stream' },
    },
  },
};

const [inputPath, chartPath] = process.argv.slice(2);
writeFileSync(inputPath, `${JSON.stringify(input, null, 2)}\n`);
writeFileSync(chartPath, `${JSON.stringify(assembleChartjs(input), null, 2)}\n`);
console.log(`node ${process.version}`);
