struct List {
    ordered: bool,
    next: usize,
    width: usize,
}
pub(crate) struct Table {
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) saved: Option<String>,
}

#[derive(Default)]
pub(crate) struct Writer {
    pub(crate) out: String,
    lists: Vec<List>,
    pub(crate) quote: usize,
    pub(crate) fresh: bool,
    pub(crate) space: bool,
    pub(crate) pending: Vec<&'static str>,
    pub(crate) open: Vec<&'static str>,
    pub(crate) links: Vec<Option<String>>,
    pub(crate) code: Option<String>,
    pub(crate) pre: Option<String>,
    pub(crate) heading: bool,
    pub(crate) table: Option<Table>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self {
            fresh: true,
            ..Self::default()
        }
    }

    pub(crate) fn finish(mut self) -> String {
        if self.table.is_some() {
            self.end_table();
        }
        self.close_marks();
        self.out.trim().to_string()
    }

    pub(crate) fn in_cell(&self) -> bool {
        self.table
            .as_ref()
            .is_some_and(|table| table.saved.is_some())
    }

    pub(crate) fn prefix(&self) -> String {
        let indent: usize = self.lists.iter().map(|list| list.width).sum();
        format!("{}{}", "> ".repeat(self.quote), " ".repeat(indent))
    }

    pub(crate) fn trim_end(&mut self) {
        let kept = self.out.trim_end_matches([' ', '\t']).len();
        self.out.truncate(kept);
    }

    pub(crate) fn block(&mut self) {
        self.space = false;
        if self.fresh || self.out.is_empty() {
            return;
        }
        if self.in_cell() {
            self.out.push(' ');
            return;
        }
        self.close_marks();
        self.trim_end();
        let prefix = self.prefix();
        self.out.push('\n');
        self.out.push_str(prefix.trim_end());
        self.out.push('\n');
        self.out.push_str(&prefix);
        self.fresh = true;
    }

    pub(crate) fn line_break(&mut self) {
        if self.in_cell() || self.heading {
            self.space = true;
            return;
        }
        self.trim_end();
        let prefix = self.prefix();
        self.out.push_str("\\\n");
        self.out.push_str(&prefix);
        self.space = false;
    }

    pub(crate) fn text(&mut self, text: &str) {
        if let Some(code) = &mut self.code {
            code.push_str(text);
            return;
        }
        if let Some(pre) = &mut self.pre {
            pre.push_str(text);
            return;
        }
        for (index, word) in text.split(char::is_whitespace).enumerate() {
            if index > 0 {
                self.space = true;
            }
            if word.is_empty() {
                continue;
            }
            self.word_start();
            escape_markdown(word, &mut self.out);
        }
    }

    pub(crate) fn word_start(&mut self) {
        if self.space && !self.fresh {
            self.out.push(' ');
        }
        self.space = false;
        self.fresh = false;
        for mark in self.pending.drain(..) {
            self.out.push_str(mark);
            self.open.push(mark);
        }
    }

    pub(crate) fn open_mark(&mut self, mark: &'static str) {
        self.pending.push(mark);
    }

    pub(crate) fn close_mark(&mut self, mark: &'static str) {
        if let Some(at) = self.pending.iter().rposition(|item| *item == mark) {
            self.pending.remove(at);
            return;
        }
        if let Some(at) = self.open.iter().rposition(|item| *item == mark) {
            let trimmed = self.out.len() != self.out.trim_end_matches(' ').len();
            self.trim_end();
            for item in self.open.drain(at..).rev() {
                self.out.push_str(item);
            }
            self.space |= trimmed;
        }
    }

    pub(crate) fn close_marks(&mut self) {
        self.pending.clear();
        while let Some(mark) = self.open.pop() {
            self.trim_end();
            self.out.push_str(mark);
        }
    }

    pub(crate) fn raw_inline(&mut self, text: &str) {
        self.word_start();
        self.out.push_str(text);
    }

    pub(crate) fn open_list(&mut self, ordered: bool) {
        if self.lists.is_empty() {
            self.block();
        }
        self.lists.push(List {
            ordered,
            next: 1,
            width: 0,
        });
    }

    pub(crate) fn close_list(&mut self) {
        if self.lists.pop().is_some() && self.lists.is_empty() {
            self.block();
        }
    }

    pub(crate) fn open_item(&mut self) {
        self.close_marks();
        self.space = false;
        let Some(list) = self.lists.last_mut() else {
            self.block();
            return;
        };
        let marker = if list.ordered {
            let marker = format!("{}. ", list.next);
            list.next += 1;
            marker
        } else {
            "- ".to_string()
        };
        list.width = 0;
        let prefix = self.prefix();
        self.trim_end();
        if !self.out.is_empty() {
            let kept = self.out.trim_end_matches(['\n', ' ', '>']).len();
            self.out.truncate(kept);
            self.out.push('\n');
        }
        self.out.push_str(&prefix);
        self.out.push_str(&marker);
        if let Some(list) = self.lists.last_mut() {
            list.width = marker.len();
        }
        self.fresh = true;
    }
}

pub(crate) fn link_target(href: &str) -> String {
    href.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

pub(crate) fn escape_markdown(text: &str, out: &mut String) {
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
}
