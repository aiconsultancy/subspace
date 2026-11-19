# Flint parity fixture

`stacked-bar-input.json` is a Flint `ChartAssemblyInput` over synthetic
cost rows, and `stacked-bar-chartjs.json` is what `flint-chart`
0.5.1 answered for it under Node.js v26.10.0 on 2026-10-18.

`flint::tests::quickjs_and_node_compile_the_same_chart` compiles the input
with the embedded bundle under QuickJS and compares the two, so the move off
Node.js cannot change a chart quietly. Regenerate both files together, with
the same Flint version, only when the pinned version changes:

```bash
npm install flint-chart@0.5.1
node parity.mjs stacked-bar-input.json stacked-bar-chartjs.json
```

The fixture holds no real cost data.
