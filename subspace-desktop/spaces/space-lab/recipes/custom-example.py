"""Custom capacity analysis. Deterministic synthetic fixture, not live capacity."""

from __future__ import annotations

import json
import sys
from datetime import date, timedelta
from pathlib import Path

import polars as pl

FX_FIXTURE = {"USD": 1.0, "EUR": 0.92, "GBP": 0.78}
SERVICES = ["Terminal", "Pipeline", "Depot", "Berth"]
LOCATIONS = ["Antwerp", "CorpusChristi", "Fujairah", "SullomVoe"]
GROUP_BY = {"location", "service", "date"}


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def fixture_rows() -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    start = date(2026, 8, 31)
    for day in range(34):
        iso = (start + timedelta(days=day)).isoformat()
        service = SERVICES[day % 4]
        location = LOCATIONS[(day // 2) % 4]
        rows.append(
            {
                "date": iso,
                "service": service,
                "location": location,
                "amount": 50.0 + day * 9.0 + SERVICES.index(service) * 20.0,
            }
        )
        other = SERVICES[(day + 2) % 4]
        rows.append(
            {
                "date": iso,
                "service": other,
                "location": LOCATIONS[(day + 1) % 4],
                "amount": 35.0 + day * 7.0 + SERVICES.index(other) * 12.0,
            }
        )
    return rows


def required_str(payload: dict[str, object], key: str) -> str:
    value = payload.get(key)
    if not isinstance(value, str) or not value.strip():
        fail(f"input.json missing string field {key}")
    return value.strip()


def main() -> None:
    path = Path("input.json")
    if not path.is_file():
        fail("input.json is missing from the job workspace")
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        fail("input.json must be an object")
    start = required_str(raw, "start")
    end = required_str(raw, "end")
    currency = required_str(raw, "currency")
    service = required_str(raw, "service")
    group_by = required_str(raw, "group_by")
    if currency not in FX_FIXTURE:
        fail(f"unsupported currency {currency}; fixture FX rates cover USD, EUR, GBP")
    if group_by not in GROUP_BY:
        fail(f"unsupported group_by {group_by}")
    if service != "All" and service not in SERVICES:
        fail(f"unknown service {service}")
    rate = FX_FIXTURE[currency]
    frame = pl.DataFrame(fixture_rows()).with_columns(pl.col("amount").cast(pl.Float64))
    filtered = frame.filter((pl.col("date") >= start) & (pl.col("date") <= end))
    if service != "All":
        filtered = filtered.filter(pl.col("service") == service)
    converted = filtered.with_columns((pl.col("amount") * rate).round(2).alias("amount"))
    grouped = (
        converted.group_by(group_by)
        .agg(pl.col("amount").sum().round(2).alias("amount"))
        .sort(group_by)
    )
    table = grouped.to_dicts()
    labels = [str(row[group_by]) for row in table]
    values = [round(float(row["amount"]), 2) for row in table]
    rows = [
        {group_by: row[group_by], "amount": round(float(row["amount"]), 2), "currency": currency}
        for row in table
    ]
    params = f"{start}:{end}:{currency}:{service}:{group_by}"
    print(
        json.dumps(
            {
                "columns": [group_by, "amount", "currency"],
                "rows": rows,
                "labels": labels,
                "series": [{"name": f"capacity ({currency})", "values": values}],
                "summary": (
                    "Custom capacity analysis. Synthetic fixture, not live capacity. "
                    f"Polars grouped {converted.height} rows by {group_by}. Amounts in "
                    f"{currency} using fixture FX rates, not live quotes. params={params}"
                ),
                "provenance": f"synthetic-fixture; source=recipes/custom-example.py; fx=fixture-rates; params={params}",
            },
            allow_nan=False,
        ),
        flush=True,
    )


if __name__ == "__main__":
    main()
