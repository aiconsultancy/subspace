use crate::ProgramName;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabOrder {
    ids: Vec<u64>,
    selected: usize,
}

impl TabOrder {
    pub fn new() -> Self {
        Self {
            ids: Vec::new(),
            selected: 0,
        }
    }

    pub fn selected(&self) -> Option<u64> {
        self.ids.get(self.selected).copied()
    }

    pub fn insert(&mut self, id: u64) {
        self.ids.push(id);
        self.selected = self.ids.len() - 1;
    }

    pub fn select(&mut self, index: usize) {
        if index < self.ids.len() {
            self.selected = index;
        }
    }

    pub fn next(&mut self) {
        if !self.ids.is_empty() {
            self.selected = (self.selected + 1) % self.ids.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.ids.is_empty() {
            self.selected = (self.selected + self.ids.len() - 1) % self.ids.len();
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let Some(index) = self.ids.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.ids.remove(index);
        if self.ids.is_empty() {
            self.selected = 0;
        } else if index < self.selected {
            self.selected -= 1;
        } else if self.selected >= self.ids.len() {
            self.selected = self.ids.len() - 1;
        }
        true
    }

    pub fn ids(&self) -> &[u64] {
        &self.ids
    }
}

pub fn title(osc_title: Option<&str>, program: &ProgramName) -> String {
    osc_title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| program.as_str())
        .to_owned()
}

pub fn truncate(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_owned();
    }
    if max_chars <= 1 {
        return "…".to_owned();
    }
    format!("{}…", chars[..max_chars - 1].iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::{TabOrder, title, truncate};
    use crate::ProgramName;

    #[test]
    fn tab_order_inserts_selects_and_removes() {
        let mut tabs = TabOrder::new();
        tabs.insert(1);
        tabs.insert(2);
        tabs.previous();
        assert_eq!(tabs.selected(), Some(1));
        assert!(tabs.remove(1));
        assert_eq!(tabs.selected(), Some(2));
    }

    #[test]
    fn removing_an_earlier_tab_keeps_the_selected_tab() {
        let mut tabs = TabOrder::new();
        tabs.insert(1);
        tabs.insert(2);
        tabs.insert(3);
        tabs.select(2);
        assert!(tabs.remove(1));
        assert_eq!(tabs.selected(), Some(3));
    }

    #[test]
    fn tab_order_wraps_next_previous_and_ignores_invalid_selection() {
        let mut tabs = TabOrder::new();
        tabs.insert(1);
        tabs.insert(2);
        tabs.select(0);
        tabs.select(99);
        assert_eq!(tabs.selected(), Some(1));
        tabs.next();
        assert_eq!(tabs.selected(), Some(2));
        tabs.next();
        assert_eq!(tabs.selected(), Some(1));
        tabs.previous();
        assert_eq!(tabs.selected(), Some(2));
    }

    #[test]
    fn title_prefers_osc_and_truncates_by_characters() {
        let program = ProgramName::from_path("/bin/zsh");
        assert_eq!(title(Some("build"), &program), "build");
        assert_eq!(title(None, &program), "zsh");
        assert_eq!(truncate("terminal", 5), "term…");
        assert_eq!(truncate("éclair", 3), "éc…");
    }
}
