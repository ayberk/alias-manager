use crate::models::{Alias, App, Config, Focus, SortField};
use crate::parser::compare_ignore_case;

impl App {
    pub fn new(config: &Config) -> Self {
        Self {
            all_aliases: Vec::new(),
            filtered_aliases: Vec::new(),
            list_state: Default::default(),
            filter_query: config.last_filter.clone(),
            input_mode: false,
            focus: Focus::Aliases,
            sort_field: SortField::Name,
            show_source: config.show_source,
            show_help: config.show_help,
            show_details: true,
            is_loading: true,
            last_action: None,
        }
    }

    pub fn set_action(&mut self, message: &str) {
        self.last_action = Some((message.to_string(), std::time::Instant::now()));
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    pub fn toggle_sort(&mut self) {
        self.sort_field = match self.sort_field {
            SortField::Name => SortField::Usage,
            SortField::Usage => SortField::Broken,
            SortField::Broken => SortField::Name,
        };
        self.sort_aliases();
        self.set_action(&format!("Sorted by {:?}", self.sort_field));
    }

    fn sort_aliases(&mut self) {
        match self.sort_field {
            SortField::Name => {
                self.all_aliases
                    .sort_by(|a, b| compare_ignore_case(&a.name, &b.name));
                self.filtered_aliases
                    .sort_by(|a, b| compare_ignore_case(&a.name, &b.name));
            }
            SortField::Usage => {
                self.all_aliases.sort_by(|a, b| {
                    b.usage_count
                        .cmp(&a.usage_count)
                        .then_with(|| compare_ignore_case(&a.name, &b.name))
                });
                self.filtered_aliases.sort_by(|a, b| {
                    b.usage_count
                        .cmp(&a.usage_count)
                        .then_with(|| compare_ignore_case(&a.name, &b.name))
                });
            }
            SortField::Broken => {
                self.all_aliases.sort_by(|a, b| {
                    b.is_broken
                        .cmp(&a.is_broken)
                        .then_with(|| compare_ignore_case(&a.name, &b.name))
                });
                self.filtered_aliases.sort_by(|a, b| {
                    b.is_broken
                        .cmp(&a.is_broken)
                        .then_with(|| compare_ignore_case(&a.name, &b.name))
                });
            }
        }
    }

    pub fn loaded(&mut self, aliases: Vec<Alias>) {
        let selected_name = self
            .list_state
            .selected()
            .and_then(|i| self.filtered_aliases.get(i))
            .map(|a| a.name.clone());

        self.all_aliases = aliases;
        self.sort_aliases();
        self.apply_filter();
        self.is_loading = false;

        if let Some(name) = selected_name
            && let Some(new_idx) = self.filtered_aliases.iter().position(|a| a.name == name)
        {
            self.list_state.select(Some(new_idx));
            return;
        }

        if !self.filtered_aliases.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn apply_filter(&mut self) {
        let query = self.filter_query.to_lowercase();
        let tag_query = query.strip_prefix('@').unwrap_or(&query);

        self.filtered_aliases = self
            .all_aliases
            .iter()
            .filter(|a| {
                a.name.to_lowercase().contains(&query)
                    || a.command.to_lowercase().contains(&query)
                    || a.source_file
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
                    || a.tags.iter().any(|t| t.to_lowercase().contains(tag_query))
                    || a.expanded_command
                        .as_ref()
                        .map(|e| e.to_lowercase().contains(&query))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();

        if self.filtered_aliases.is_empty() {
            self.list_state.select(None);
        } else {
            let current = self.list_state.selected().unwrap_or(0);
            if current >= self.filtered_aliases.len() {
                self.list_state.select(Some(0));
            } else {
                self.list_state.select(Some(current));
            }
        }
    }

    pub fn next(&mut self) {
        if self.filtered_aliases.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.filtered_aliases.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.filtered_aliases.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_aliases.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn save_config(&self) {
        let cfg = Config {
            show_source: self.show_source,
            show_help: self.show_help,
            last_filter: self.filter_query.clone(),
        };
        cfg.store();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mock_alias(name: &str, usage: usize) -> Alias {
        Alias {
            name: name.to_string(),
            command: "test".to_string(),
            source_file: PathBuf::from("test.sh"),
            line_number: 1,
            is_conflicting: false,
            is_broken: false,
            description: None,
            usage_count: usage,
            shadows: Vec::new(),
            duplicates: Vec::new(),
            tags: Vec::new(),
            last_used: None,
            expanded_command: None,
        }
    }

    fn mock_broken_alias(name: &str) -> Alias {
        let mut a = mock_alias(name, 0);
        a.is_broken = true;
        a
    }

    #[test]
    fn test_app_navigation() {
        let mut app = App::new(&Config::default());
        app.loaded(vec![
            mock_alias("a", 0),
            mock_alias("b", 0),
            mock_alias("c", 0),
        ]);

        assert_eq!(app.list_state.selected(), Some(0));
        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
        app.next();
        assert_eq!(app.list_state.selected(), Some(2));
        app.next();
        assert_eq!(app.list_state.selected(), Some(0));

        app.previous();
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn test_app_filtering() {
        let mut app = App::new(&Config::default());
        let mut apple = mock_alias("apple", 0);
        apple.tags = vec!["fruit".to_string()];
        let mut banana = mock_alias("banana", 0);
        banana.tags = vec!["fruit".to_string(), "yellow".to_string()];

        app.loaded(vec![apple, banana, mock_alias("cherry", 0)]);

        app.filter_query = "a".to_string();
        app.apply_filter();
        assert_eq!(app.filtered_aliases.len(), 2);

        app.filter_query = "fruit".to_string();
        app.apply_filter();
        assert_eq!(app.filtered_aliases.len(), 2);

        app.filter_query = "@yellow".to_string();
        app.apply_filter();
        assert_eq!(app.filtered_aliases.len(), 1);
        assert_eq!(app.filtered_aliases[0].name, "banana");

        app.filter_query = "zzz".to_string();
        app.apply_filter();
        assert_eq!(app.filtered_aliases.len(), 0);
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_app_sorting() {
        let mut app = App::new(&Config::default());
        let aliases = vec![
            mock_alias("apple", 10),
            mock_alias("banana", 50),
            mock_alias("cherry", 20),
            mock_broken_alias("broken"),
        ];
        app.loaded(aliases);

        assert_eq!(app.filtered_aliases[0].name, "apple");
        assert_eq!(app.filtered_aliases[1].name, "banana");

        app.toggle_sort();
        assert_eq!(app.filtered_aliases[0].name, "banana");

        app.toggle_sort();
        assert_eq!(app.filtered_aliases[0].name, "broken");
        assert_eq!(app.filtered_aliases[1].name, "apple");

        app.toggle_sort();
        assert_eq!(app.filtered_aliases[0].name, "apple");
    }
}
