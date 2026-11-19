"""Config-created HTTPX/Polars board. The HTTP endpoint is an explicit fixture."""
import json
from pathlib import Path
import httpx
import polars as pl

parameters = json.loads(Path("input.json").read_text())
fixture = [
    {"date": "2026-09-05", "service": "Transit", "location": "Oslo", "amount": 120.0},
    {"date": "2026-09-20", "service": "Storage", "location": "Boston", "amount": 80.0},
    {"date": "2026-09-25", "service": "Transit", "location": "Oslo", "amount": 60.0},
]
transport = httpx.MockTransport(lambda request: httpx.Response(200, json=fixture))
with httpx.Client(transport=transport) as client:
    rows = client.get("https://fixture.invalid/metrics").json()
frame = pl.DataFrame(rows).filter(
    (pl.col("date") >= parameters["start"]) & (pl.col("date") <= parameters["end"])
)
if parameters["service"] != "All":
    frame = frame.filter(pl.col("service") == parameters["service"])
rate = {"USD": 1.0, "EUR": 0.92, "GBP": 0.78}[parameters["currency"]]
group = parameters["group_by"]
data = frame.with_columns((pl.col("amount") * rate).alias("amount")).group_by(group).agg(pl.col("amount").sum()).sort(group).to_dicts()
labels = [str(row[group]) for row in data]
values = [round(float(row["amount"]), 2) for row in data]
fingerprint = ":".join(parameters[field] for field in ("start", "end", "currency", "service", "group_by"))
print(json.dumps({
    "columns": [group, "amount"], "rows": data, "labels": labels,
    "series": [{"name": "SDK fixture totals", "values": values}],
    "summary": "A new environment and recipe created through configuration, without new Rust. HTTPX MockTransport fixture plus Polars; not a live API or live FX rates.",
    "provenance": f"synthetic-fixture; engine=httpx-mock+polars; params={fingerprint}",
}))
