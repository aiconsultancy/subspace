use super::{InputDeclaration, InputValue, limits};
use anyhow::{Result, bail, ensure};

// Only floating-point roundoff is permitted when reconstructing slider steps.
const STEP_ROUNDOFF_MULTIPLIER: f64 = 4.0;
const STEP_ABSOLUTE_TOLERANCE: f64 = 1e-8;

pub(super) fn declaration(input: &InputDeclaration) -> Result<()> {
    match input {
        InputDeclaration::Number { min, max, step, .. } => {
            ensure!(
                min.is_finite()
                    && max.is_finite()
                    && step.is_finite()
                    && min <= max
                    && (max - min).is_finite()
                    && *step > 0.0,
                "number constraints must be finite, min <= max and step > 0"
            );
        }
        InputDeclaration::String { options, .. } => options_contract(options)?,
    }
    value(input, &input.default_value(), None)
}

pub(super) fn options_contract(options: &[String]) -> Result<()> {
    ensure!(
        !options.is_empty() && options.len() <= limits::OPTIONS,
        "options: expected 1 to 500 choices"
    );
    ensure!(
        options
            .iter()
            .all(|value| !value.is_empty() && value.len() <= 1024),
        "options: each choice must be 1 to 1024 bytes"
    );
    super::validation::unique(options)
}

pub(super) fn value(
    input: &InputDeclaration,
    value: &InputValue,
    options: Option<&[String]>,
) -> Result<()> {
    match (input, value) {
        (InputDeclaration::Number { min, max, step, .. }, InputValue::Number(number)) => {
            ensure!(
                number.is_finite() && number >= min && number <= max,
                "number is outside its finite range"
            );
            let steps = (number - min) / step;
            let tolerance = (steps.abs() * f64::EPSILON * STEP_ROUNDOFF_MULTIPLIER)
                .max(STEP_ABSOLUTE_TOLERANCE);
            ensure!(
                steps.is_finite() && (steps - steps.round()).abs() <= tolerance,
                "number does not match its declared step"
            );
        }
        (
            InputDeclaration::String {
                options: declared, ..
            },
            InputValue::String(string),
        ) => {
            ensure!(
                options.unwrap_or(declared).contains(string),
                "string is not an available option"
            );
        }
        _ => bail!("input value has the wrong type"),
    }
    Ok(())
}

/// Checks name syntax only. Private settings additionally reject reserved host
/// names; declaring a name here neither stores nor delegates a value.
pub fn environment_key(name: &str) -> Result<()> {
    ensure!(
        (1..=128).contains(&name.len())
            && name.as_bytes()[0].is_ascii_uppercase()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'),
        "environment key must be uppercase ASCII with a letter first"
    );
    Ok(())
}
