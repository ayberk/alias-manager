use crate::models::{Alias, App, Config, Focus, KeyAction, SortField};
use crate::parser::compare_ignore_case;
use crossterm::event::{KeyCode, KeyEvent};
use std::io;
use std::sync::Arc;

impl App {
    pub fn new(config: &Config) -> Self {
        Self {
            all_aliases: Vec::new(),
            filtered_aliases: Vec::new(),
            list_state: Default::default(),
            filter_query: config.last_filter.clone(),
            focus: Focus::Aliases,
            sort_field: SortField::Name,
            show_source: config.show_source,
            show_help: config.show_help,
            show_details: true,
            is_loading: true,
            last_action: None,
            nix_paths: config.nix_paths.clone(),
            ignore_sources: config.ignore_sources.clone(),
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
        self.set_action(&format!("Sorted by {}", self.sort_field));
    }

    fn sort_aliases(&mut self) {
        let sort_field = self.sort_field;
        for list in [&mut self.all_aliases, &mut self.filtered_aliases] {
            match sort_field {
                SortField::Name => {
                    list.sort_by(|a, b| compare_ignore_case(&a.name, &b.name));
                }
                SortField::Usage => {
                    list.sort_by(|a, b| {
                        b.usage_count
                            .cmp(&a.usage_count)
                            .then_with(|| compare_ignore_case(&a.name, &b.name))
                    });
                }
                SortField::Broken => {
                    list.sort_by(|a, b| {
                        b.is_broken
                            .cmp(&a.is_broken)
                            .then_with(|| compare_ignore_case(&a.name, &b.name))
                    });
                }
            }
        }
    }

    fn selected_alias(&self) -> Option<&Alias> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_aliases.get(i))
            .map(|a| a.as_ref())
    }

    pub fn loaded(&mut self, aliases: Vec<Alias>) {
        let selected_name = self
            .list_state
            .selected()
            .and_then(|i| self.filtered_aliases.get(i))
            .map(|a| a.name.clone());

        self.all_aliases = aliases
            .into_iter()
            .filter(|a| !self.ignore_sources.contains(&a.source_file))
            .map(Arc::new)
            .collect();
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

        self.filtered_aliases = self
            .all_aliases
            .iter()
            .filter(|a| {
                if let Some(tag_query) = query.strip_prefix('@') {
                    // @-prefixed query: match only tags
                    a.tags.iter().any(|t| t.to_lowercase().contains(tag_query))
                } else {
                    a.name.to_lowercase().contains(&query)
                        || a.command.to_lowercase().contains(&query)
                        || a.source_file
                            .to_string_lossy()
                            .to_lowercase()
                            .contains(&query)
                        || a.tags.iter().any(|t| t.to_lowercase().contains(&query))
                        || a.expanded_command
                            .as_ref()
                            .map(|e| e.to_lowercase().contains(&query))
                            .unwrap_or(false)
                }
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

    pub fn save_config(&mut self) -> io::Result<()> {
        let cfg = Config {
            show_source: self.show_source,
            show_help: self.show_help,
            last_filter: self.filter_query.clone(),
            nix_paths: self.nix_paths.clone(),
            ignore_sources: self.ignore_sources.clone(),
        };
        if let Err(e) = cfg.store() {
            self.set_action(&format!("Config save failed: {e}"));
            return Err(e);
        }
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> KeyAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Filter => Focus::Aliases,
                    Focus::Aliases => Focus::Filter,
                };
            }
            _ => {
                if self.focus == Focus::Filter {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            self.focus = Focus::Aliases;
                            let _ = self.save_config();
                        }
                        KeyCode::Char(c) => {
                            self.filter_query.push(c);
                            self.apply_filter();
                        }
                        KeyCode::Backspace => {
                            self.filter_query.pop();
                            self.apply_filter();
                        }
                        _ => {}
                    }
                } else if self.focus == Focus::Aliases {
                    match key.code {
                        KeyCode::Char('q') => {
                            let _ = self.save_config();
                            return KeyAction::Quit;
                        }
                        KeyCode::Char('/') => {
                            self.focus = Focus::Filter;
                        }
                        KeyCode::Char('c') => {
                            self.filter_query.clear();
                            self.apply_filter();
                            let _ = self.save_config();
                        }
                        KeyCode::Char('h') => {
                            self.show_source = !self.show_source;
                            let _ = self.save_config();
                        }
                        KeyCode::Char('d') => self.toggle_details(),
                        KeyCode::Char('s') => self.toggle_sort(),
                        KeyCode::Char('?') => {
                            self.show_help = !self.show_help;
                            let _ = self.save_config();
                        }
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            let is_extended = matches!(key.code, KeyCode::Char('Y'));
                            if let Some(alias) = self.selected_alias() {
                                let text_to_copy = if is_extended {
                                    alias.yank_command(true)
                                } else {
                                    alias.name.clone()
                                };
                                match arboard::Clipboard::new() {
                                    Ok(mut cb) => {
                                        if let Err(e) = cb.set_text(text_to_copy.clone()) {
                                            self.set_action(&format!("Err: {e}"));
                                        } else {
                                            let truncated = if text_to_copy.chars().count() > 15 {
                                                let s: String =
                                                    text_to_copy.chars().take(12).collect();
                                                format!("{}...", s)
                                            } else {
                                                text_to_copy.clone()
                                            };
                                            self.set_action(&format!("Yanked: {truncated}"));
                                        }
                                    }
                                    Err(_) => self.set_action("Clipboard init failed"),
                                }
                            }
                        }
                        KeyCode::Char('e') => {
                            if let Some(alias) = self.selected_alias() {
                                let editor =
                                    std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                                return KeyAction::OpenEditor {
                                    source_file: alias.source_file.clone(),
                                    line_number: alias.line_number,
                                    editor,
                                };
                            }
                        }
                        KeyCode::Char('x') => {
                            if let Some(alias) = self.selected_alias() {
                                return KeyAction::ExcludeSource(alias.source_file.clone());
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => self.next(),
                        KeyCode::Up | KeyCode::Char('k') => self.previous(),
                        _ => {}
                    }
                }
            }
        }
        KeyAction::Continue
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
            usage_count: usage,
            ..Default::default()
        }
    }

    fn mock_broken_alias(name: &str) -> Alias {
        Alias {
            is_broken: true,
            ..mock_alias(name, 0)
        }
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

        // @-prefixed query must NOT match names/commands that happen to contain @
        let mut at_alias = mock_alias("@fruit", 0); // name has literal @fruit
        at_alias.tags = vec![];
        app.all_aliases.push(Arc::new(at_alias));

        app.filter_query = "@fruit".to_string();
        app.apply_filter();
        // apple and banana are tagged "fruit"; the "@fruit" alias (no tags) must NOT appear
        assert_eq!(app.filtered_aliases.len(), 2);
        assert!(!app.filtered_aliases.iter().any(|a| a.name == "@fruit"));

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
