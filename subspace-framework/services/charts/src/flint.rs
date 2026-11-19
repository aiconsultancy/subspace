//! Flint under QuickJS (REQ-41, REQ-42).
//!
//! `flint-chart` is a TypeScript package, so Subspace embeds the published
//! Chart.js build of the pinned version and runs it in QuickJS, in process:
//! the chart input goes in as JSON and the Chart.js configuration comes out
//! as JSON. Nothing is installed, downloaded or spawned, so an agent can
//! render a chart on a machine with no Node.js and no network.
//!
//! Each compile gets a runtime of its own with a memory limit, a stack limit
//! and a deadline, so a pathological input costs one failed tool call rather
//! than the application. Built-in board charts need none of this; they build
//! a [`crate::ChartModel`] in Rust.
//!
//! The vendored bundle is `dist/chartjs/index.js` from
//! [flint-chart](https://github.com/microsoft/flint-chart) 0.5.1 (MIT),
//! unchanged. `scripts/refresh-flint.sh` replaces it.

use std::time::{Duration, Instant};

use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Function, Module, Object, Runtime};
use serde_json::Value;

/// The embedded Flint build. Only this version's output is tested.
pub const PACKAGE: &str = "flint-chart@0.5.1";
const BUNDLE: &str = include_str!("../vendor/flint-chart-0.5.1/chartjs.js");
/// The name the bundle is declared under; it appears in a JavaScript stack.
const MODULE: &str = "flint-chart";
/// The function the bundle exports.
const ASSEMBLE: &str = "assembleChartjs";

/// How long one compile may run before it is interrupted. A chart of the
/// size the tools allow takes about ten milliseconds.
pub const COMPILE_TIMEOUT: Duration = Duration::from_secs(5);
/// The heap one compile may take.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;
/// The JavaScript stack one compile may take.
const STACK_SIZE: usize = 2 * 1024 * 1024;

/// The one global the bundle needs that QuickJS has no built-in for. Flint
/// clones plain data with it, which a JSON round trip does exactly.
const PRELUDE: &str = "globalThis.structuredClone = (value) => \
    value === undefined ? value : JSON.parse(JSON.stringify(value));";

/// Compile a Flint `ChartAssemblyInput` to a Chart.js configuration, with
/// Flint's own private keys removed.
pub fn compile_chartjs(input: &Value) -> Result<Value, String> {
    let chart = evaluate(BUNDLE, &input.to_string(), COMPILE_TIMEOUT)?;
    let chart: Value = serde_json::from_str(&chart)
        .map_err(|error| format!("Flint wrote an answer Subspace cannot read: {error}"))?;
    Ok(strip_private(chart))
}

/// Flint marks its own layout and pivot data with a leading underscore, which
/// is not part of the Chart.js configuration.
pub fn strip_private(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter(|(name, _)| !name.starts_with('_'))
                .map(|(name, field)| (name, strip_private(field)))
                .collect(),
        ),
        Value::Array(entries) => Value::Array(entries.into_iter().map(strip_private).collect()),
        other => other,
    }
}

/// Run `source`'s [`ASSEMBLE`] over `input`, in a runtime of its own, and
/// return the answer as JSON. The runtime is dropped with the compile, so
/// nothing survives one chart.
fn evaluate(source: &str, input: &str, timeout: Duration) -> Result<String, String> {
    let runtime = Runtime::new().map_err(|error| format!("QuickJS did not start: {error}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(STACK_SIZE);
    let deadline = deadline_after(timeout);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context =
        Context::full(&runtime).map_err(|error| format!("QuickJS did not start: {error}"))?;
    context
        .with(|ctx| assemble(ctx, source, input))
        .map_err(|error| overran(error, deadline, timeout))
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now() + timeout
}

/// Declare the bundle, call its assembler and bring the answer back as JSON.
fn assemble(ctx: Ctx<'_>, source: &str, input: &str) -> Result<String, String> {
    let json: Object = ctx
        .globals()
        .get("JSON")
        .map_err(|error| caught(&ctx, error))?;
    let parse: Function = json.get("parse").map_err(|error| caught(&ctx, error))?;
    let stringify: Function = json.get("stringify").map_err(|error| caught(&ctx, error))?;
    ctx.eval::<(), _>(PRELUDE).catch(&ctx).map_err(message)?;
    let exports = exports(ctx.clone(), source)?;
    let assembler: Function = exports
        .get(ASSEMBLE)
        .map_err(|error| format!("{PACKAGE} exports no {ASSEMBLE}: {error}"))?;
    let argument: rquickjs::Value = parse
        .call((input,))
        .map_err(|error| format!("the chart input is not JSON: {error}"))?;
    let chart: rquickjs::Value = assembler.call((argument,)).catch(&ctx).map_err(message)?;
    stringify.call((chart,)).catch(&ctx).map_err(message)
}

/// The module's exported values, after it and its top-level await finish.
fn exports<'js>(ctx: Ctx<'js>, source: &str) -> Result<Object<'js>, String> {
    let declared = Module::declare(ctx.clone(), MODULE, source)
        .catch(&ctx)
        .map_err(message)?;
    let (module, promise) = declared.eval().catch(&ctx).map_err(message)?;
    promise.finish::<()>().catch(&ctx).map_err(message)?;
    module.namespace().catch(&ctx).map_err(message)
}

/// A plain rquickjs error, caught so its JavaScript side can be read.
fn caught(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    message(CaughtError::from_error(ctx, error))
}

/// What a caught JavaScript error says: its message, and the stack under it
/// when it carries one.
fn message(error: CaughtError<'_>) -> String {
    match error {
        CaughtError::Exception(exception) => {
            let message = exception.message().unwrap_or_default();
            let stack = exception.stack().unwrap_or_default();
            match (message.is_empty(), stack.trim().is_empty()) {
                (true, true) => "the chart was rejected without a message".to_owned(),
                (_, true) => message,
                (true, false) => stack.trim().to_owned(),
                _ => format!("{message}\n{}", stack.trim_end()),
            }
        }
        CaughtError::Value(value) => format!("{value:?}"),
        CaughtError::Error(error) => error.to_string(),
    }
}

/// The message a failure reads as. An interrupted compile reports no reason
/// of its own, so a failure past the deadline is reported as the timeout it
/// is.
fn overran(reason: String, deadline: Instant, timeout: Duration) -> String {
    match Instant::now() >= deadline {
        true => format!(
            "the chart did not compile within {} s and was stopped",
            timeout.as_secs_f32()
        ),
        false => format!("Flint could not compile the chart: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::chartjs;
    use crate::flint::{
        BUNDLE, COMPILE_TIMEOUT, PACKAGE, compile_chartjs, evaluate, strip_private,
    };
    use crate::model::ChartKind;
    use serde_json::{Value, json};
    use std::time::Duration;

    /// The input the parity fixture was generated from; synthetic data.
    const PARITY_INPUT: &str = include_str!("../tests/parity/stacked-bar-input.json");
    /// What `flint-chart` 0.5.1 answered under Node for that input.
    const PARITY_CHARTJS: &str = include_str!("../tests/parity/stacked-bar-chartjs.json");

    fn stacked_bar_input() -> Value {
        json!({
            "data": { "values": [
                { "date": "2026-10-01", "stream": "web", "cost": 120.5 },
                { "date": "2026-10-01", "stream": "api", "cost": 40.25 },
                { "date": "2026-10-02", "stream": "web", "cost": 131.0 },
                { "date": "2026-10-02", "stream": "api", "cost": 38.5 }
            ] },
            "semantic_types": { "date": "Date", "stream": "Category", "cost": "Price" },
            "chart_spec": {
                "chartType": "Stacked Bar Chart",
                "title": "Daily cost by work stream",
                "encodings": {
                    "x": { "field": "date" },
                    "y": { "field": "cost" },
                    "color": { "field": "stream" }
                }
            }
        })
    }

    #[test]
    fn flint_compiles_a_stacked_bar_chart_into_a_chart_model() {
        assert!(PACKAGE.ends_with("@0.5.1"), "the pinned version");
        assert!(
            BUNDLE.contains("export { assembleChartjs"),
            "the vendored file is the ES module Flint publishes"
        );
        assert_eq!(COMPILE_TIMEOUT.as_secs(), 5);

        let config = compile_chartjs(&stacked_bar_input()).expect("Flint compiles the chart");
        assert_eq!(config.get("_transform"), None, "private keys are stripped");

        let model = chartjs::to_model(&config).expect("the configuration converts");
        assert_eq!(model.kind, ChartKind::Bar);
        assert!(model.stacked);
        assert_eq!(model.labels, ["2026-10-01", "2026-10-02"]);
        assert_eq!(model.series.len(), 2);
        assert_eq!(model.series[0].values, [Some(120.5), Some(131.0)]);
    }

    /// QuickJS must answer exactly what Node answered for the same input:
    /// the fixture was taken from `flint-chart` 0.5.1 under Node v26.10.0.
    #[test]
    fn quickjs_and_node_compile_the_same_chart() {
        let input: Value = serde_json::from_str(PARITY_INPUT).expect("the fixture input parses");
        let expected: Value =
            serde_json::from_str(PARITY_CHARTJS).expect("the fixture answer parses");

        let compiled = compile_chartjs(&input).expect("Flint compiles the fixture");

        assert_eq!(compiled, strip_private(expected));
    }

    #[test]
    fn flints_private_keys_go_before_the_conversion() {
        let chart = json!({
            "type": "bar",
            "data": { "datasets": [{ "data": [1], "_source": "x" }] },
            "_width": 479,
            "_transform": { "arrange": {} },
            "_pivot": { "ids": ["default"] }
        });
        let stripped = strip_private(chart);

        assert_eq!(stripped.get("_width"), None);
        assert_eq!(stripped.get("_transform"), None);
        assert_eq!(stripped.get("_pivot"), None);
        assert_eq!(stripped.pointer("/data/datasets/0/_source"), None);
        assert_eq!(stripped.pointer("/data/datasets/0/data/0"), Some(&json!(1)));
    }

    #[test]
    fn a_chart_that_never_finishes_is_stopped_at_the_deadline() {
        let endless = "export function assembleChartjs() { for (;;) {} }";

        let error = evaluate(endless, "{}", Duration::from_millis(50))
            .expect_err("an endless compile is stopped");

        assert!(error.contains("did not compile within"), "{error}");
        assert!(error.contains("stopped"), "{error}");
    }

    #[test]
    fn a_chart_flint_rejects_reports_what_it_said() {
        let rejecting =
            "export function assembleChartjs() { throw new Error('Unknown Chart.js chart type'); }";

        let error = evaluate(rejecting, "{}", COMPILE_TIMEOUT)
            .expect_err("the module throws")
            .to_lowercase();

        assert!(error.contains("unknown chart.js chart type"), "{error}");

        let broken = evaluate("export const nothing = 1;", "{}", COMPILE_TIMEOUT)
            .expect_err("the module exports no assembler");
        assert!(broken.contains("assembleChartjs"), "{broken}");

        let unparsable = evaluate(BUNDLE, "not json", COMPILE_TIMEOUT)
            .expect_err("the input is not JSON")
            .to_lowercase();
        assert!(unparsable.contains("json"), "{unparsable}");
    }
}
