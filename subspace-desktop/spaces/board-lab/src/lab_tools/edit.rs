use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::environments::EnvironmentManager;
use crate::studio::Studio;

use super::helpers::{preset, required};
use super::schema::Input;

pub(super) fn run(
    studio: &Studio,
    _manager: &EnvironmentManager,
    _root: &Path,
    input: Input,
) -> Result<Value> {
    match input.action.as_str() {
        "add_control" => add_control(studio, input),
        "parameters" => parameters(studio, input),
        "save" => Ok(json!({"preset":studio.save(&preset(studio, &input)?)?,"saved":true})),
        "delete" => delete(studio, input),
        "arrange" => arrange(studio, input),
        "import_canvas" => import_canvas(input),
        _ => bail!("Unknown edit action"),
    }
}

fn add_control(studio: &Studio, input: Input) -> Result<Value> {
    let mut preset = preset(studio, &input)?;
    studio.add_control(
        &mut preset,
        required(input.parameter.as_deref(), "parameter")?,
    )?;
    Ok(json!({"preset":preset, "saved":false}))
}

fn parameters(studio: &Studio, input: Input) -> Result<Value> {
    let mut preset = preset(studio, &input)?;
    preset.parameters = input.parameters.context("parameters is required")?;
    Ok(json!({"preset":preset, "saved":false}))
}

fn delete(studio: &Studio, input: Input) -> Result<Value> {
    studio.delete(required(input.id.as_deref(), "id")?)?;
    Ok(json!({"deleted":true}))
}

fn arrange(studio: &Studio, input: Input) -> Result<Value> {
    Ok(json!({"layout":studio.arrange(&input.layout.context("layout is required")?)?}))
}

fn import_canvas(input: Input) -> Result<Value> {
    Ok(
        json!({"preset":crate::studio::import_canvas(input.canvas.context("canvas is required")?, input.title.as_deref().unwrap_or("Imported canvas"))?,"saved":false}),
    )
}
