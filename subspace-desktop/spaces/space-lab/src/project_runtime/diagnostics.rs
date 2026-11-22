const DIAGNOSTIC_BYTES: usize = 16 * 1024;

#[derive(Default)]
pub(super) struct Diagnostics {
    tail: String,
    incomplete: Vec<u8>,
}

impl Diagnostics {
    pub(super) fn text(&self) -> &str {
        &self.tail
    }

    pub(super) fn append(&mut self, bytes: &[u8], selected: &[String]) {
        self.incomplete.extend_from_slice(bytes);
        let (redacted, consumed) = redact_chunk(&self.incomplete, selected);
        self.incomplete.drain(..consumed);
        self.tail.push_str(&String::from_utf8_lossy(&redacted));
        let mut remove = self.tail.len().saturating_sub(DIAGNOSTIC_BYTES);
        while !self.tail.is_char_boundary(remove) {
            remove += 1;
        }
        self.tail.drain(..remove);
    }
}

fn redact_chunk(bytes: &[u8], selected: &[String]) -> (Vec<u8>, usize) {
    let values: Vec<_> = selected
        .iter()
        .filter(|value| !value.is_empty())
        .map(String::as_bytes)
        .collect();
    let mut redacted = Vec::new();
    let mut ix = 0;
    while ix < bytes.len() {
        let remainder = &bytes[ix..];
        if let Some(value) = values
            .iter()
            .filter(|value| remainder.starts_with(value))
            .max_by_key(|value| value.len())
        {
            redacted.extend_from_slice(b"[redacted]");
            ix += value.len();
        } else if values.iter().any(|value| value.starts_with(remainder)) {
            break;
        } else {
            redacted.push(bytes[ix]);
            ix += 1;
        }
    }
    (redacted, ix)
}

pub(super) fn redact(text: &str, selected: &[String]) -> String {
    let mut values: Vec<_> = selected.iter().filter(|value| !value.is_empty()).collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.into_iter().fold(text.to_owned(), |text, value| {
        text.replace(value, "[redacted]")
    })
}
