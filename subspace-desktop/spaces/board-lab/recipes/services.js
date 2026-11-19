// Deterministic synthetic service profile. Fixture data, not live traffic.
// Uses the real semver dependency to filter synthetic service releases.
const fs = require("node:fs");
const semver = require("semver");

const FX_FIXTURE = { USD: 1.0, EUR: 0.92, GBP: 0.78 };
const SERVICES = ["api-gateway", "billing", "ingest", "search"];
const LOCATIONS = ["eu-west", "us-east", "ap-southeast", "uk-south"];

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function pad(value) {
  return String(value).padStart(2, "0");
}

function isoDate(year, month, day) {
  return `${year}-${pad(month)}-${pad(day)}`;
}

function addDays(year, month, day, offset) {
  const date = new Date(Date.UTC(year, month - 1, day + offset));
  return isoDate(date.getUTCFullYear(), date.getUTCMonth() + 1, date.getUTCDate());
}

function fixtureRows() {
  const rows = [];
  for (let day = 0; day < 34; day += 1) {
    const iso = addDays(2026, 8, 31, day);
    const service = SERVICES[day % 4];
    const location = LOCATIONS[Math.floor(day / 2) % 4];
    rows.push({
      date: iso,
      service,
      location,
      amount: 90 + day * 13 + SERVICES.indexOf(service) * 30,
      release: `1.${day % 3}.${day % 5}`,
    });
    const other = SERVICES[(day + 2) % 4];
    const otherLocation = LOCATIONS[(day + 1) % 4];
    rows.push({
      date: iso,
      service: other,
      location: otherLocation,
      amount: 70 + day * 9 + SERVICES.indexOf(other) * 18,
      release: `1.${(day + 1) % 3}.${day % 5}`,
    });
  }
  return rows;
}

function main() {
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync("input.json", "utf8"));
    if (!FX_FIXTURE[parsed.currency] || !["location", "service", "date"].includes(parsed.group_by)) throw new Error("Unsupported fixture parameters");
  } catch (error) {
    fail(`invalid input.json: ${error instanceof Error ? error.message : error}`);
  }
  if (parsed.service !== "All" && !SERVICES.includes(parsed.service)) {
    fail(`unknown service ${parsed.service}`);
  }
  const rate = FX_FIXTURE[parsed.currency];
  const filtered = fixtureRows().filter((row) => {
    if (!semver.satisfies(row.release, ">=1.1.0")) return false;
    if (row.date < parsed.start || row.date > parsed.end) {
      return false;
    }
    return parsed.service === "All" || row.service === parsed.service;
  });
  const totals = new Map();
  for (const row of filtered) {
    const key = String(row[parsed.group_by]);
    const current = totals.get(key) || 0;
    totals.set(key, current + row.amount * rate);
  }
  const labels = Array.from(totals.keys()).sort((a, b) => a.localeCompare(b));
  const values = labels.map((label) => Math.round((totals.get(label) || 0) * 100) / 100);
  const rows = labels.map((label, index) => ({
    [parsed.group_by]: label,
    amount: values[index],
    currency: parsed.currency,
  }));
  const params = `${parsed.start}:${parsed.end}:${parsed.currency}:${parsed.service}:${parsed.group_by}`;
  const dataset = {
    columns: [parsed.group_by, "amount", "currency"],
    rows,
    labels,
    series: [{ name: `amount (${parsed.currency})`, values }],
    summary:
      `Synthetic service fixture (not live traffic). semver filtered releases >=1.1.0, then grouped ${filtered.length} rows by ${parsed.group_by}. Amounts in ${parsed.currency} using fixture FX rates USD=1.0 EUR=0.92 GBP=0.78, not live quotes. params=${params}`,
    provenance: `synthetic-fixture; source=recipes/services.js; fx=fixture-rates; engine=node+semver; params=${params}`,
  };
  process.stdout.write(`${JSON.stringify(dataset)}\n`);
}

main();
