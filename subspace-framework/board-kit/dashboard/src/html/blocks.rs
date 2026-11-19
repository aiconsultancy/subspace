use super::writer::{Table, Writer};

impl Writer {
    fn begin_cell(&mut self) {
        if let Some(table) = &mut self.table
            && table.saved.is_none()
        {
            table.saved = Some(std::mem::take(&mut self.out));
            self.fresh = true;
            self.space = false;
        }
    }

    fn end_cell(&mut self) {
        self.close_marks();
        let Some(table) = &mut self.table else {
            return;
        };
        let Some(saved) = table.saved.take() else {
            return;
        };
        let cell = std::mem::replace(&mut self.out, saved)
            .trim()
            .replace('\n', " ");
        if let Some(row) = table.rows.last_mut() {
            row.push(cell);
        }
        self.fresh = true;
    }

    pub(crate) fn end_table(&mut self) {
        self.end_cell();
        let Some(table) = self.table.take() else {
            return;
        };
        let rows: Vec<Vec<String>> = table
            .rows
            .into_iter()
            .filter(|row| !row.is_empty())
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        self.block();
        let prefix = self.prefix();
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
                self.out.push_str(&prefix);
            }
            self.out.push('|');
            for column in 0..columns {
                self.out.push_str(&format!(
                    " {} |",
                    row.get(column).map_or("", String::as_str)
                ));
            }
            if index == 0 {
                self.out.push('\n');
                self.out.push_str(&prefix);
                self.out.push('|');
                self.out.push_str(&" --- |".repeat(columns));
            }
        }
        self.fresh = false;
        self.block();
    }

    pub(crate) fn start_table(&mut self) {
        if self.table.is_none() {
            self.block();
            self.table = Some(Table {
                rows: Vec::new(),
                saved: None,
            });
        }
    }
    pub(crate) fn finish_table(&mut self) {
        self.end_table();
    }
    pub(crate) fn start_row(&mut self) {
        self.end_cell();
        if let Some(table) = &mut self.table {
            table.rows.push(Vec::new());
        }
    }
    pub(crate) fn start_cell(&mut self) {
        self.end_cell();
        if self
            .table
            .as_ref()
            .is_some_and(|table| table.rows.is_empty())
            && let Some(table) = &mut self.table
        {
            table.rows.push(Vec::new());
        }
        self.begin_cell();
    }
    pub(crate) fn finish_cell(&mut self) {
        self.end_cell();
    }
    pub(crate) fn set_code(&mut self) {
        self.code = Some(String::new());
    }
    pub(crate) fn finish_code(&mut self) {
        let code = self.code.take().unwrap_or_default();
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        if !code.is_empty() {
            let ticks = if code.contains('`') { "``" } else { "`" };
            let pad = if code.starts_with('`') || code.ends_with('`') {
                " "
            } else {
                ""
            };
            self.raw_inline(&format!("{ticks}{pad}{code}{pad}{ticks}"));
        }
    }
    pub(crate) fn start_pre(&mut self) {
        self.block();
        self.pre = Some(String::new());
    }
    pub(crate) fn finish_pre(&mut self) {
        let code = self
            .pre
            .take()
            .unwrap_or_default()
            .trim_matches('\n')
            .to_string();
        let fence = if code.contains("```") { "~~~" } else { "```" };
        let prefix = self.prefix();
        self.out.push_str(fence);
        for line in code.lines() {
            self.out.push('\n');
            self.out.push_str(&prefix);
            self.out.push_str(line);
        }
        self.out.push('\n');
        self.out.push_str(&prefix);
        self.out.push_str(fence);
        self.fresh = false;
        self.block();
    }
}
