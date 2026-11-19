# Writing a Flint chart for Subspace

Adapted from the `flint-chart-author` agent skill in
<https://github.com/microsoft/flint-chart> (MIT licence, Microsoft
Corporation), cut to the Chart.js backend that Subspace renders.

You write a **`ChartAssemblyInput`**. Subspace compiles it with
`flint-chart` 0.5.1 in embedded QuickJS, converts the Chart.js output to its own
chart model and draws it natively. Subspace never renders Chart.js itself, so
backend styling, interactions and themes have no effect.

```ts
interface ChartAssemblyInput {
  data: { values: any[] };                      // rows, or a named dataset
  semantic_types: Record<string, string>;       // field -> semantic type
  chart_spec: {
    chartType: string;                          // an exact name from the table
    title?: string;                             // the finding, in a sentence
    subtitle?: string;                          // what is measured, of whom, in what units
    encodings: Record<string, { field: string }>;
    chartProperties?: Record<string, any>;
  };
}
```

## Steps

1. Pick the chart type from the table below, spelled exactly.
2. Map each channel to a column by name.
3. Give every encoded field a semantic type. This is the step that decides
   formatting, the zero baseline and the colour scheme.
4. Write a title that states the finding and a subtitle that says what is
   measured, over what period and in what units.

## Chart types Subspace draws

| chartType | Channels | Notes |
| --- | --- | --- |
| `"Bar Chart"` | x, y, color, opacity | One discrete category plus one measure. A second category on `color` stacks the segments. |
| `"Stacked Bar Chart"` | x, y, color | Second category on `color`, stacked within each bar. Use for parts of a whole. |
| `"Grouped Bar Chart"` | x, y, **group**, color | Second category on `group`, side by side. Use to compare values directly. |
| `"Line Chart"` | x, y, color, opacity | `interpolate` sets the curve. |
| `"Area Chart"` | x, y, color, opacity | `stackMode` stacks the areas. |
| `"Pie Chart"` | **size**, color | `size` is the slice value, `color` the category. |
| `"Doughnut Chart"` | **size**, color | A pie with a hole; `innerRadius` sets its size. |
| `"Scatter Plot"` | x, y, color, size, opacity | x and y are both measures. |

The second category reads from a different channel in each bar chart:
`color` stacks, `group` dodges. Comparing parts to a whole means Stacked;
comparing values side by side means Grouped; one series means Bar.

Subspace draws no other Chart.js type. Histogram, Bubble, Radar, Rose,
Lollipop, Bump, Slope, Combo, Range Area and Connected Scatter compile but
do not render, and asking for one returns an error that lists the types
above.

## Semantic types

Pick the most specific type for every encoded field.

| Family | Types |
| --- | --- |
| Temporal | `Date`, `DateTime`, `Time`, `Timestamp`, `Year`, `Quarter`, `Month`, `Week`, `Day`, `Hour`, `YearMonth` |
| Amount | `Amount`, `Price`, `Quantity`, `Count`, `Number` |
| Proportion | `Percentage` |
| Signed | `Profit`, `PercentageChange` |
| Discrete | `Rank`, `Score`, `ID` |
| Categorical | `Category`, `Name`, `Status`, `Boolean` |
| Fallback | `Unknown` |

A cost in USD is a `Price`. A day is a `Date`. A team, an environment, a
region and a service are each a `Category`. Do not invent type names; when none fits use
`Quantity` for a number, `Category` for a string and `Date` for a day.

## Chart properties

Set one only when it is asked for; the defaults are sensible.

| Chart type | Property | Range (default) | Effect |
| --- | --- | --- | --- |
| Bar | `cornerRadius` | 0-15 (0) | Round the bar corners |
| Area, Stacked Bar | `stackMode` | `stacked`, `normalize`, `layered` | `normalize` draws shares of 100% |
| Grouped Bar | `dodge` | `auto`, `local`, `global` (`auto`) | How sparse groups take their lanes |
| Line, Area | `interpolate` | `linear`, `monotone`, `step` (`linear`) | Curve shape |
| Pie, Doughnut | `innerRadius` | 0-100 (0) | Hole size |
| Pie, Doughnut | `sortSlices` | `none`, `descending`, `ascending` | Slice order |
| Scatter | `opacity` | 0.1-1 (1) | Point opacity |

Chart-wide: `logScale_x`, `logScale_y`, `includeZero_x`, `includeZero_y`.

## Rules

- **Reference columns by name.** Every `field` names a column of the rows
  in `data.values`.
- **Transform first.** Flint compiles charts; it does not aggregate, filter,
  join or pivot. Ask for the data you need in the shape you need it.
- **One reshape only.** An array on `x` or `y` folds several measure columns
  into series. There is no other transform, and the array form cannot be
  combined with `color`.
- **No styling.** Colours, fonts, tick counts and sizes come from the
  Subspace theme. `theme_spec`, `interaction_spec` and `baseSize` are ignored.
- **Check the data first.** A category column that mixes a total with its
  parts double-counts on a stacked or grouped channel.

## Examples

Daily cost by team:

```json
{
  "data": { "values": [] },
  "semantic_types": { "date": "Date", "team": "Category", "cost": "Price" },
  "chart_spec": {
    "chartType": "Stacked Bar Chart",
    "title": "Platform and Data carry most of the daily cost",
    "subtitle": "All environments, USD per day",
    "encodings": {
      "x": { "field": "date" },
      "y": { "field": "cost" },
      "color": { "field": "team" }
    }
  }
}
```

Totals by team, largest first:

```json
{
  "data": { "values": [] },
  "semantic_types": { "team": "Category", "cost": "Price" },
  "chart_spec": {
    "chartType": "Bar Chart",
    "title": "Cost by team",
    "encodings": {
      "x": { "field": "team", "sortBy": "y", "sortOrder": "descending" },
      "y": { "field": "cost" }
    }
  }
}
```

Share of the top services, as a doughnut:

```json
{
  "data": { "values": [] },
  "semantic_types": { "service": "Category", "cost": "Price" },
  "chart_spec": {
    "chartType": "Doughnut Chart",
    "title": "Compute is half the bill",
    "subtitle": "USD, month to date",
    "encodings": { "size": { "field": "cost" }, "color": { "field": "service" } },
    "chartProperties": { "innerRadius": 55, "sortSlices": "descending" }
  }
}
```

## Before you send it

1. `chartType` is one of the eight names above.
2. Every `field` names a real column.
3. Every encoded field has a semantic type.
4. Pie and Doughnut use `size` and `color`; Grouped Bar uses `group`.
5. The title states the finding and the subtitle names the units.
