// Deterministic synthetic TypeScript notebook profile. Fixture data, not live jobs.
// Native TypeScript for bun-notebook. Reads input.json from the job cwd.

import { readFileSync } from "node:fs";
import { stderr, stdout, exit } from "node:process";

interface Parameters {
  start: string;
  end: string;
  currency: string;
  service: string;
  group_by: string;
}

interface PlotSeries {
  name: string;
  values: number[];
}

interface Dataset {
  columns: string[];
  rows: Array<Record<string, string | number>>;
  labels: string[];
  series: PlotSeries[];
  summary: string;
  provenance: string;
}

const FX_FIXTURE: Record<string, number> = { USD: 1.0, EUR: 0.92, GBP: 0.78 };
const SERVICES = ["compile", "test", "lint", "docs"] as const;
const LOCATIONS = ["local", "ci", "staging", "prod"] as const;
const GROUP_BY = new Set(["location", "service", "date"]);

function fail(message: string): never {
  stderr.write(`${message}\n`);
  exit(1);
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

function addDays(offset: number): string {
  const date = new Date(Date.UTC(2026, 7, 31 + offset));
  return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`;
}

function fixtureRows(): Array<{
  date: string;
  service: string;
  location: string;
  amount: number;
}> {
  const rows = [];
  for (let day = 0; day < 34; day += 1) {
    const iso = addDays(day);
    const service = SERVICES[day % 4];
    const location = LOCATIONS[Math.floor(day / 2) % 4];
    rows.push({
      date: iso,
      service,
      location,
      amount: 60 + day * 8 + SERVICES.indexOf(service) * 14,
    });
    const other = SERVICES[(day + 3) % 4];
    const otherLocation = LOCATIONS[(day + 2) % 4];
    rows.push({
      date: iso,
      service: other,
      location: otherLocation,
      amount: 45 + day * 6 + SERVICES.indexOf(other) * 10,
    });
  }
  return rows;
}

function required(payload: Record<string, unknown>, key: keyof Parameters): string {
  const value = payload[key];
  if (typeof value !== "string" || value.trim() === "") {
    fail(`input.json missing string field ${key}`);
  }
  return value.trim();
}

const raw = JSON.parse(readFileSync("input.json", "utf8")) as Record<string, unknown>;
const input: Parameters = {
  start: required(raw, "start"),
  end: required(raw, "end"),
  currency: required(raw, "currency"),
  service: required(raw, "service"),
  group_by: required(raw, "group_by"),
};

if (!(input.currency in FX_FIXTURE)) {
  fail(`unsupported currency ${input.currency}; fixture FX rates cover USD, EUR, GBP`);
}
if (!GROUP_BY.has(input.group_by)) {
  fail(`unsupported group_by ${input.group_by}`);
}
if (input.service !== "All" && !SERVICES.includes(input.service as (typeof SERVICES)[number])) {
  fail(`unknown service ${input.service}`);
}

const rate = FX_FIXTURE[input.currency];
const filtered = fixtureRows().filter((row) => {
  if (row.date < input.start || row.date > input.end) {
    return false;
  }
  return input.service === "All" || row.service === input.service;
});
const totals = new Map<string, number>();
for (const row of filtered) {
  const key = String(row[input.group_by as "date" | "service" | "location"]);
  totals.set(key, (totals.get(key) ?? 0) + row.amount * rate);
}
const labels = Array.from(totals.keys()).sort((a, b) => a.localeCompare(b));
const values = labels.map((label) => Math.round((totals.get(label) ?? 0) * 100) / 100);
const rows = labels.map((label, index) => ({
  [input.group_by]: label,
  amount: values[index],
  currency: input.currency,
}));
const params = `${input.start}:${input.end}:${input.currency}:${input.service}:${input.group_by}`;
const dataset: Dataset = {
  columns: [input.group_by, "amount", "currency"],
  rows,
  labels,
  series: [{ name: `amount (${input.currency})`, values }],
  summary: `Synthetic TypeScript notebook fixture (not live jobs). Grouped ${filtered.length} rows by ${input.group_by}. Amounts in ${input.currency} using fixture FX rates USD=1.0 EUR=0.92 GBP=0.78, not live quotes. params=${params}`,
  provenance: `synthetic-fixture; source=recipes/typescript.ts; fx=fixture-rates; engine=bun-typescript; params=${params}`,
};

stdout.write(`${JSON.stringify(dataset)}\n`);
