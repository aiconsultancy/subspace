//! Opt-in draft limits, independent of any protocol or host.

use crate::badge::PillTone;

const BYTES_PER_KB: usize = 1024;
const BYTE_REVEAL_DIVISOR: usize = 2;
const WARNING_NUMERATOR: usize = 4;
const WARNING_DENOMINATOR: usize = 5;

/// Limits supplied by the host. Zero values are clamped to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftLimits {
    lines: usize,
    bytes: usize,
}

impl DraftLimits {
    pub fn new(lines: usize, bytes: usize) -> Self {
        Self {
            lines: lines.max(1),
            bytes: bytes.max(1),
        }
    }

    pub(super) fn counter(self, text: &str) -> Option<Counter> {
        let lines = text.lines().count().max(1);
        let bytes = text.len();
        Counter::new(lines, bytes, self)
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct Counter {
    pub text: String,
    pub label: String,
    pub tone: PillTone,
}

impl Counter {
    fn new(lines: usize, bytes: usize, limits: DraftLimits) -> Option<Self> {
        let text = counter_text(lines, bytes, limits);
        let tone = usage_tone(lines, limits.lines).max(usage_tone(bytes, limits.bytes));
        let label = format!("Draft limits: {text}. {}.", limit_status(tone));
        (!text.is_empty()).then_some(Self { text, label, tone })
    }
}

fn limit_status(tone: PillTone) -> &'static str {
    match tone {
        PillTone::Muted => "Within limits",
        PillTone::Warning => "Near the limit",
        PillTone::Danger => "Over the limit",
    }
}

fn counter_text(lines: usize, bytes: usize, limits: DraftLimits) -> String {
    let mut parts = Vec::new();
    if lines > 1 {
        parts.push(format!("{lines} / {} lines", limits.lines));
    }
    if bytes > limits.bytes / BYTE_REVEAL_DIVISOR {
        parts.push(format!(
            "{:.1} / {} KB",
            bytes as f64 / BYTES_PER_KB as f64,
            limits.bytes / BYTES_PER_KB,
        ));
    }
    parts.join(" · ")
}

fn usage_tone(used: usize, maximum: usize) -> PillTone {
    if used > maximum {
        PillTone::Danger
    } else if used.saturating_mul(WARNING_DENOMINATOR) >= maximum.saturating_mul(WARNING_NUMERATOR)
    {
        PillTone::Warning
    } else {
        PillTone::Muted
    }
}

#[cfg(test)]
mod tests;
