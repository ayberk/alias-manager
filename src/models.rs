use ratatui::widgets::ListState;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub show_source: bool,
    pub show_help: bool,
    pub last_filter: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_source: true,
            show_help: true,
            last_filter: String::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if let Some(home) = home::home_dir() {
            let path = home.join(".alias-manager.conf");
            if let Ok(content) = fs::read_to_string(path) {
                let mut config = Self::default();
                for line in content.lines() {
                    let parts: Vec<&str> = line.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        match parts[0] {
                            "show_source" => config.show_source = parts[1] == "true",
                            "show_help" => config.show_help = parts[1] == "true",
                            "last_filter" => config.last_filter = parts[1].to_string(),
                            _ => {}
                        }
                    }
                }
                return config;
            }
        }
        Self::default()
    }

    pub fn store(&self) {
        if let Some(home) = home::home_dir() {
            let path = home.join(".alias-manager.conf");
            let content = format!(
                "show_source={}\nshow_help={}\nlast_filter={}\n",
                self.show_source, self.show_help, self.last_filter
            );
            let _ = fs::write(path, content);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowedDefinition {
    pub source_file: PathBuf,
    pub line_number: usize,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub command: String,
    pub source_file: PathBuf,
    pub line_number: usize,
    pub is_conflicting: bool,
    pub is_broken: bool,
    pub description: Option<String>,
    pub usage_count: usize,
    pub shadows: Vec<ShadowedDefinition>,
    pub duplicates: Vec<String>,
    pub tags: Vec<String>,
    pub last_used: Option<u64>, // Unix timestamp
    pub expanded_command: Option<String>,
}

impl Alias {
    pub fn yank_command(&self, recursive: bool) -> String {
        let cmd = if recursive && let Some(expanded) = &self.expanded_command {
            expanded
        } else {
            &self.command
        };

        if self.name.starts_with("git ") {
            if let Some(stripped) = cmd.strip_prefix('!') {
                stripped.to_string()
            } else {
                format!("git {}", cmd)
            }
        } else {
            cmd.clone()
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Focus {
    Filter,
    Aliases,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SortField {
    Name,
    Usage,
    Broken,
}

pub struct App {
    pub all_aliases: Vec<Alias>,
    pub filtered_aliases: Vec<Alias>,
    pub list_state: ListState,
    pub filter_query: String,
    pub input_mode: bool,
    pub focus: Focus,
    pub sort_field: SortField,
    pub show_source: bool,
    pub show_help: bool,
    pub show_details: bool,
    pub is_loading: bool,
    pub last_action: Option<(String, std::time::Instant)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yank_command_shell() {
        let alias = Alias {
            name: "l".into(),
            command: "ls -la".into(),
            expanded_command: Some("ls -la".into()),
            source_file: PathBuf::from("test"),
            line_number: 1,
            is_conflicting: false,
            is_broken: false,
            description: None,
            usage_count: 0,
            shadows: vec![],
            duplicates: vec![],
            tags: vec![],
            last_used: None,
        };
        assert_eq!(alias.yank_command(false), "ls -la");
        assert_eq!(alias.yank_command(true), "ls -la");
    }

    #[test]
    fn test_yank_command_git() {
        let alias = Alias {
            name: "git st".into(),
            command: "status".into(),
            expanded_command: Some("status".into()),
            source_file: PathBuf::from("test"),
            line_number: 1,
            is_conflicting: false,
            is_broken: false,
            description: None,
            usage_count: 0,
            shadows: vec![],
            duplicates: vec![],
            tags: vec![],
            last_used: None,
        };
        assert_eq!(alias.yank_command(false), "git status");
        assert_eq!(alias.yank_command(true), "git status");
    }

    #[test]
    fn test_yank_command_git_shell() {
        let alias = Alias {
            name: "git l".into(),
            command: "!ls -la".into(),
            expanded_command: Some("!ls -la".into()),
            source_file: PathBuf::from("test"),
            line_number: 1,
            is_conflicting: false,
            is_broken: false,
            description: None,
            usage_count: 0,
            shadows: vec![],
            duplicates: vec![],
            tags: vec![],
            last_used: None,
        };
        assert_eq!(alias.yank_command(false), "ls -la");
    }

    #[test]
    fn test_yank_command_recursive() {
        let alias = Alias {
            name: "gs".into(),
            command: "g status".into(),
            expanded_command: Some("git status".into()),
            source_file: PathBuf::from("test"),
            line_number: 1,
            is_conflicting: false,
            is_broken: false,
            description: None,
            usage_count: 0,
            shadows: vec![],
            duplicates: vec![],
            tags: vec![],
            last_used: None,
        };
        assert_eq!(alias.yank_command(false), "g status");
        assert_eq!(alias.yank_command(true), "git status");
    }
}
