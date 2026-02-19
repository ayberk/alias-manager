pub mod git;
pub mod history;
pub mod shell;

use crate::models::{Alias, ShadowedDefinition};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use which::which;

pub fn get_all_aliases() -> Vec<Alias> {
    let mut raw_aliases = Vec::new();
    let mut history_data = HashMap::new();

    let mut env_vars: Vec<(String, String)> = std::env::vars().collect();
    env_vars.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    if let Some(home) = home::home_dir() {
        history_data = history::parse_history(&home);

        let mut visited_files = HashSet::new();

        // Zsh entry points
        let zsh_files = [".zshrc", ".zshenv", ".zprofile"];
        for file in zsh_files {
            let _ = shell::parse_file_recursive(
                &home.join(file),
                &mut raw_aliases,
                &mut visited_files,
                &env_vars,
            );
        }

        // Bash entry points
        let bash_files = [".bashrc", ".bash_profile", ".profile"];
        for file in bash_files {
            let _ = shell::parse_file_recursive(
                &home.join(file),
                &mut raw_aliases,
                &mut visited_files,
                &env_vars,
            );
        }

        // Fish entry point
        let fish_config = home.join(".config/fish/config.fish");
        let _ = shell::parse_file_recursive(
            &fish_config,
            &mut raw_aliases,
            &mut visited_files,
            &env_vars,
        );

        // Git config
        let gitconfig_path = home.join(".gitconfig");
        let _ = git::parse_git_config(&gitconfig_path, &mut raw_aliases);
    }

    // Group by name to find effective aliases and their shadow chains
    let mut alias_groups: HashMap<String, Vec<Alias>> = HashMap::new();
    for alias in raw_aliases {
        alias_groups
            .entry(alias.name.clone())
            .or_default()
            .push(alias);
    }

    let mut effective_aliases = Vec::new();
    let mut command_to_names: HashMap<String, Vec<String>> = HashMap::new();

    for (name, mut group) in alias_groups {
        if let Some(mut effective) = group.pop() {
            for shadow in group {
                effective.is_conflicting = true;
                effective.shadows.push(ShadowedDefinition {
                    source_file: shadow.source_file,
                    line_number: shadow.line_number,
                    command: shadow.command,
                });
            }

            if let Some(info) = history_data.get(&name) {
                effective.usage_count = info.count;
                if info.last_used > 0 {
                    effective.last_used = Some(info.last_used);
                }
            }

            if effective.name.starts_with("git ") {
                if effective.command.starts_with('!') {
                    let cmd = &effective.command[1..];
                    effective.is_broken = !validate_command(cmd, &env_vars);
                } else {
                    effective.is_broken = false;
                }
            } else {
                effective.is_broken = !validate_command(&effective.command, &env_vars);
            }

            command_to_names
                .entry(effective.command.clone())
                .or_default()
                .push(effective.name.clone());
            effective_aliases.push(effective);
        }
    }

    // Calculate duplicates and expansions
    let mut alias_map: HashMap<String, String> = effective_aliases
        .iter()
        .map(|a| (a.name.clone(), a.command.clone()))
        .collect();

    // Add git subcommands to the map for git-specific expansion
    for alias in &effective_aliases {
        if let Some(stripped) = alias.name.strip_prefix("git ")
            && !alias_map.contains_key(stripped)
        {
            alias_map.insert(stripped.to_string(), alias.command.clone());
        }
    }

    for alias in &mut effective_aliases {
        if let Some(names) = command_to_names.get(&alias.command) {
            alias.duplicates = names
                .iter()
                .filter(|&n| n != &alias.name)
                .cloned()
                .collect();
        }

        alias.expanded_command = resolve_expansion(&alias.command, &alias_map);
    }

    effective_aliases
}

pub fn validate_command(command: &str, env_vars: &[(String, String)]) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if let Some(cmd) = parts.first() {
        let mut expanded_cmd = cmd.to_string();
        for (key, value) in env_vars {
            let key_dollar = format!("${}", key);
            if expanded_cmd.contains(&key_dollar) {
                expanded_cmd = expanded_cmd.replace(&key_dollar, value);
            }
        }

        if expanded_cmd.starts_with("./")
            || expanded_cmd.starts_with("../")
            || expanded_cmd.contains('/')
        {
            return true;
        }

        if which(&expanded_cmd).is_ok() {
            return true;
        }

        let builtins = [
            "cd",
            "echo",
            "export",
            "source",
            ".",
            "alias",
            "unalias",
            "history",
            "exit",
            "pwd",
            "true",
            "false",
            "test",
            "[",
            "[[",
            "local",
            "read",
            "type",
            "command",
            "builtin",
            "eval",
            "exec",
            "set",
            "unset",
            "wait",
            "trap",
            "ulimit",
            "umask",
            "fg",
            "bg",
            "jobs",
            "kill",
            "fc",
            "hash",
            "popd",
            "pushd",
            "dirs",
            "shift",
            "time",
            "times",
            "return",
            "break",
            "continue",
            "printf",
            "getopts",
            "declare",
            "typeset",
            "let",
            "shopt",
            "caller",
            "compgen",
            "complete",
            "compopt",
            "disown",
            "enable",
            "help",
            "logout",
            "mapfile",
            "readarray",
            "suspend",
        ];
        if builtins.contains(&expanded_cmd.as_str()) {
            return true;
        }

        return false;
    }
    true
}

pub fn resolve_path(
    path_str: &str,
    base_dir: &Path,
    env_vars: &[(String, String)],
) -> Option<PathBuf> {
    let mut expanded = path_str.to_string();

    if expanded.contains("$(brew --prefix)") {
        expanded = expanded.replace("$(brew --prefix)", "/opt/homebrew");
    }

    for (key, value) in env_vars {
        let key_dollar = format!("${}", key);
        let key_braces = format!("${{{}}}", key);
        if expanded.contains(&key_dollar) {
            expanded = expanded.replace(&key_dollar, value);
        }
        if expanded.contains(&key_braces) {
            expanded = expanded.replace(&key_braces, value);
        }
    }

    if let Some(home) = home::home_dir() {
        if expanded == "~" {
            return Some(home);
        } else if let Some(stripped) = expanded.strip_prefix("~/") {
            let mut p = home;
            p.push(stripped);
            return Some(p);
        }
    }

    let p = PathBuf::from(&expanded);
    if p.is_absolute() {
        Some(p)
    } else {
        let mut abs_p = base_dir.to_path_buf();
        abs_p.push(p);
        if abs_p.exists() { Some(abs_p) } else { None }
    }
}

pub fn resolve_expansion(command: &str, alias_map: &HashMap<String, String>) -> Option<String> {
    let mut current_command = command.to_string();
    let mut history = HashSet::new();
    let mut expanded = false;

    for _ in 0..10 {
        let parts: Vec<&str> = current_command.split_whitespace().collect();
        if parts.is_empty() {
            break;
        }

        let potential_alias = parts[0];
        if let Some(expansion) = alias_map.get(potential_alias) {
            if history.contains(potential_alias) {
                return Some(format!("{} (Loop detected)", current_command));
            }
            history.insert(potential_alias.to_string());

            let args = if current_command.len() > potential_alias.len() {
                &current_command[potential_alias.len()..]
            } else {
                ""
            };
            current_command = format!("{}{}", expansion, args);
            expanded = true;
        } else {
            break;
        }
    }

    if expanded {
        Some(current_command)
    } else {
        None
    }
}

pub fn extract_tags(comment: Option<&str>) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(c) = comment {
        for word in c.split_whitespace() {
            if word.starts_with('@') && word.len() > 1 {
                tags.push(word[1..].to_string());
            }
        }
    }
    tags
}

pub fn clean_description(comment: Option<String>) -> Option<String> {
    comment
        .map(|c| {
            let cleaned: String = c
                .split_whitespace()
                .filter(|word| !word.starts_with('@'))
                .collect::<Vec<_>>()
                .join(" ");
            cleaned
        })
        .filter(|s| !s.is_empty())
}

pub fn compare_ignore_case(a: &str, b: &str) -> Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_path() {
        let base = Path::new("/home/user");
        let env_vars = vec![("TEST_DIR".to_string(), "/tmp/test".to_string())];

        assert_eq!(
            resolve_path("/abs/path", base, &[]),
            Some(PathBuf::from("/abs/path"))
        );
        assert_eq!(
            resolve_path("$TEST_DIR/file", base, &env_vars),
            Some(PathBuf::from("/tmp/test/file"))
        );
    }

    #[test]
    fn test_resolve_expansion() {
        let mut map = HashMap::new();
        map.insert("g".to_string(), "git".to_string());
        map.insert("gs".to_string(), "g status".to_string());
        map.insert("bar".to_string(), "echo backfoo".to_string());
        map.insert("foo".to_string(), "bar".to_string());

        assert_eq!(resolve_expansion("g", &map), Some("git".to_string()));
        assert_eq!(
            resolve_expansion("gs", &map),
            Some("git status".to_string())
        );
        assert_eq!(
            resolve_expansion("foo", &map),
            Some("echo backfoo".to_string())
        );

        map.insert("a".to_string(), "b".to_string());
        map.insert("b".to_string(), "a".to_string());
        let res = resolve_expansion("a", &map).unwrap();
        assert!(res.contains("Loop detected"));
    }

    #[test]
    fn test_compare_ignore_case() {
        assert_eq!(compare_ignore_case("apple", "Apple"), Ordering::Equal);
        assert_eq!(compare_ignore_case("a", "B"), Ordering::Less);
        assert_eq!(compare_ignore_case("B", "a"), Ordering::Greater);
    }

    #[test]
    fn test_validate_command() {
        assert!(validate_command("ls", &[]));
        assert!(validate_command("cd", &[]));
        assert!(!validate_command("thiscommandcertainlydoesnotexist", &[]));
    }

    #[test]
    fn test_extract_tags() {
        assert_eq!(
            extract_tags(Some("@work @utils info")),
            vec!["work", "utils"]
        );
        assert_eq!(extract_tags(None), Vec::<String>::new());
    }

    #[test]
    fn test_clean_description() {
        assert_eq!(
            clean_description(Some("@work info".into())),
            Some("info".into())
        );
        assert_eq!(clean_description(Some("@work @utils".into())), None);
    }
}
