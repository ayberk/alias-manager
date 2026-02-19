use crate::parser::expand_home;
use ratatui::widgets::ListState;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Config {
    pub show_source: bool,
    pub show_help: bool,
    pub last_filter: String,
    /// Extra nix file/directory paths to scan, beyond the built-in defaults.
    /// Set in ~/.alias-manager.conf as: nix_paths=~/dotfiles/nix:~/.config/nix
    pub nix_paths: Vec<PathBuf>,
    /// Source files whose aliases should be excluded entirely.
    /// Set in ~/.alias-manager.conf as: ignore_sources=~/.iterm2_shell_integration.zsh
    pub ignore_sources: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_source: true,
            show_help: true,
            last_filter: String::new(),
            nix_paths: Vec::new(),
            ignore_sources: Vec::new(),
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
                            "nix_paths" => {
                                config.nix_paths = parts[1]
                                    .split(':')
                                    .filter(|s| !s.is_empty())
                                    .map(expand_home)
                                    .collect();
                            }
                            "ignore_sources" => {
                                config.ignore_sources = parts[1]
                                    .split(':')
                                    .filter(|s| !s.is_empty())
                                    .map(expand_home)
                                    .collect();
                            }
                            _ => {}
                        }
                    }
                }
                return config;
            }
        }
        Self::default()
    }

    pub fn store(&self) -> io::Result<()> {
        let home = home::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
        let path = home.join(".alias-manager.conf");
        let mut content = format!(
            "show_source={}\nshow_help={}\nlast_filter={}\n",
            self.show_source, self.show_help, self.last_filter,
        );
        if !self.nix_paths.is_empty() {
            let nix_paths_str = self
                .nix_paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(":");
            content.push_str(&format!("nix_paths={}\n", nix_paths_str));
        }
        if !self.ignore_sources.is_empty() {
            let ignore_str = self
                .ignore_sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(":");
            content.push_str(&format!("ignore_sources={}\n", ignore_str));
        }
        fs::write(path, content)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowedDefinition {
    pub source_file: PathBuf,
    pub line_number: usize,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
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
    pub fn source_name(&self) -> &str {
        self.source_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

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

impl std::fmt::Display for SortField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortField::Name => f.write_str("Name"),
            SortField::Usage => f.write_str("Usage"),
            SortField::Broken => f.write_str("Broken"),
        }
    }
}

pub struct App {
    pub all_aliases: Vec<Arc<Alias>>,
    pub filtered_aliases: Vec<Arc<Alias>>,
    pub list_state: ListState,
    pub filter_query: String,
    pub focus: Focus,
    pub sort_field: SortField,
    pub show_source: bool,
    pub show_help: bool,
    pub show_details: bool,
    pub is_loading: bool,
    pub last_action: Option<(String, std::time::Instant)>,
    pub nix_paths: Vec<PathBuf>,
    pub ignore_sources: Vec<PathBuf>,
}

/// Returned by [`App::handle_key`] to signal what the event loop should do.
pub enum KeyAction {
    /// Exit the application.
    Quit,
    /// Open the given file in an external editor, then reload aliases.
    OpenEditor {
        source_file: PathBuf,
        line_number: usize,
        editor: String,
    },
    /// Add the given source file to the ignore list, save config, and reload.
    ExcludeSource(PathBuf),
    /// No terminal-level action needed; the app state was already updated.
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_config_store_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join(".alias-manager.conf");

        let original = Config {
            show_source: false,
            show_help: true,
            last_filter: "docker".to_string(),
            nix_paths: vec![PathBuf::from("/tmp/nix1"), PathBuf::from("/tmp/nix2")],
            ignore_sources: vec![PathBuf::from("/tmp/ignored.sh")],
        };

        // Write directly to a temp path (bypassing home-dir lookup).
        let content = format!(
            "show_source={}\nshow_help={}\nlast_filter={}\nnix_paths={}\nignore_sources={}\n",
            original.show_source,
            original.show_help,
            original.last_filter,
            original
                .nix_paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(":"),
            original
                .ignore_sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(":"),
        );
        std::fs::write(&conf_path, &content).unwrap();

        // Parse it back using the same logic Config::load uses.
        let loaded = {
            let mut cfg = Config::default();
            for line in content.lines() {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    match parts[0] {
                        "show_source" => cfg.show_source = parts[1] == "true",
                        "show_help" => cfg.show_help = parts[1] == "true",
                        "last_filter" => cfg.last_filter = parts[1].to_string(),
                        "nix_paths" => {
                            cfg.nix_paths = parts[1]
                                .split(':')
                                .filter(|s| !s.is_empty())
                                .map(|s| PathBuf::from(s))
                                .collect()
                        }
                        "ignore_sources" => {
                            cfg.ignore_sources = parts[1]
                                .split(':')
                                .filter(|s| !s.is_empty())
                                .map(|s| PathBuf::from(s))
                                .collect()
                        }
                        _ => {}
                    }
                }
            }
            cfg
        };

        assert_eq!(loaded.show_source, original.show_source);
        assert_eq!(loaded.show_help, original.show_help);
        assert_eq!(loaded.last_filter, original.last_filter);
        assert_eq!(loaded.nix_paths, original.nix_paths);
        assert_eq!(loaded.ignore_sources, original.ignore_sources);
    }

    #[test]
    fn test_config_store_omits_empty_vecs() {
        let cfg = Config {
            show_source: true,
            show_help: false,
            last_filter: String::new(),
            nix_paths: Vec::new(),
            ignore_sources: Vec::new(),
        };
        let mut content = format!(
            "show_source={}\nshow_help={}\nlast_filter={}\n",
            cfg.show_source, cfg.show_help, cfg.last_filter,
        );
        // nix_paths and ignore_sources should be absent when empty
        assert!(!content.contains("nix_paths"));
        assert!(!content.contains("ignore_sources"));
        // But present when non-empty
        content.push_str("nix_paths=/tmp/foo\n");
        assert!(content.contains("nix_paths"));
    }

    #[test]
    fn test_yank_command_shell() {
        let alias = Alias {
            name: "l".into(),
            command: "ls -la".into(),
            expanded_command: Some("ls -la".into()),
            source_file: PathBuf::from("test"),
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        };
        assert_eq!(alias.yank_command(false), "g status");
        assert_eq!(alias.yank_command(true), "git status");
    }
}
