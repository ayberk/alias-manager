pub mod git;
pub mod history;
pub mod nix;
pub mod shell;

use crate::models::{Alias, ShadowedDefinition};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use which::which;

/// Return the Homebrew prefix, caching the result after the first call.
///
/// Tries `brew --prefix` first; falls back to the two well-known install
/// locations so that both Apple Silicon (/opt/homebrew) and Intel (/usr/local)
/// Macs are handled without hardcoding.
fn brew_prefix() -> Option<&'static str> {
    static PREFIX: OnceLock<Option<String>> = OnceLock::new();
    PREFIX
        .get_or_init(|| {
            // Try the real brew first.
            if let Ok(out) = std::process::Command::new("brew").arg("--prefix").output()
                && out.status.success()
                && let Ok(s) = String::from_utf8(out.stdout)
            {
                let trimmed = s.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
            // Fall back: pick whichever standard path exists.
            for candidate in ["/opt/homebrew", "/usr/local"] {
                if Path::new(candidate).exists() {
                    return Some(candidate.to_string());
                }
            }
            None
        })
        .as_deref()
}

pub fn get_all_aliases(extra_nix_paths: &[PathBuf]) -> Vec<Alias> {
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

        // Nix config (~/.config/nix/ and common alternate locations)
        nix::parse_nix_dir(&home.join(".config/nix"), &mut raw_aliases, 3);
        let _ = nix::parse_nix_file(
            &home.join(".nixpkgs/darwin-configuration.nix"),
            &mut raw_aliases,
        );
        let _ = nix::parse_nix_file(&home.join(".config/nixpkgs/home.nix"), &mut raw_aliases);

        // User-configured nix paths; deduplicate against built-in paths already scanned above.
        let mut scanned_nix: HashSet<PathBuf> = [
            home.join(".config/nix"),
            home.join(".nixpkgs/darwin-configuration.nix"),
            home.join(".config/nixpkgs/home.nix"),
        ]
        .into_iter()
        .collect();
        for path in extra_nix_paths {
            if !scanned_nix.insert(path.clone()) {
                continue;
            }
            if path.is_dir() {
                nix::parse_nix_dir(path, &mut raw_aliases, 3);
            } else {
                let _ = nix::parse_nix_file(path, &mut raw_aliases);
            }
        }
    }

    // Cache for validate_command: keyed by expanded first-word, value is validity.
    let mut cmd_cache: HashMap<String, bool> = HashMap::new();

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
                    effective.is_broken = !validate_command(cmd, &env_vars, &mut cmd_cache);
                } else {
                    effective.is_broken = false;
                }
            } else {
                effective.is_broken =
                    !validate_command(&effective.command, &env_vars, &mut cmd_cache);
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

pub fn validate_command(
    command: &str,
    env_vars: &[(String, String)],
    cache: &mut HashMap<String, bool>,
) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if let Some(cmd) = parts.first() {
        let mut expanded_cmd = cmd.to_string();
        for (key, value) in env_vars {
            let key_dollar = format!("${}", key);
            if expanded_cmd.contains(&key_dollar) {
                expanded_cmd = expanded_cmd.replace(&key_dollar, value);
            }
        }

        if let Some(&cached) = cache.get(&expanded_cmd) {
            return cached;
        }

        if expanded_cmd.starts_with("./")
            || expanded_cmd.starts_with("../")
            || expanded_cmd.contains('/')
        {
            cache.insert(expanded_cmd, true);
            return true;
        }

        if which(&expanded_cmd).is_ok() {
            cache.insert(expanded_cmd, true);
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
        let valid = builtins.contains(&expanded_cmd.as_str());
        cache.insert(expanded_cmd, valid);
        valid
    } else {
        true
    }
}

/// Expand a leading `~` or `~/` to the user's home directory.
pub(crate) fn expand_home(s: &str) -> PathBuf {
    if s == "~" {
        home::home_dir().unwrap_or_else(|| PathBuf::from("~"))
    } else if let Some(rest) = s.strip_prefix("~/") {
        home::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(s))
    } else {
        PathBuf::from(s)
    }
}

pub fn resolve_path(
    path_str: &str,
    base_dir: &Path,
    env_vars: &[(String, String)],
) -> Option<PathBuf> {
    let mut expanded = path_str.to_string();

    if expanded.contains("$(brew --prefix)")
        && let Some(prefix) = brew_prefix()
    {
        expanded = expanded.replace("$(brew --prefix)", prefix);
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

    if expanded.starts_with('~') {
        let p = expand_home(&expanded);
        return if p.as_os_str() == expanded.as_str() {
            // expand_home returned the input unchanged — home dir unavailable
            None
        } else {
            Some(p)
        };
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
            // Self-referential: `alias fd='fd -H'` — expansion starts with the same
            // name, meaning the shell resolves it to the real binary. No expansion needed.
            if expansion.split_whitespace().next() == Some(potential_alias) {
                break;
            }
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

/// Strip a single layer of matching `'...'` or `"..."` quotes.
pub(crate) fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('"') && s.ends_with('"')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
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
        let mut cache = HashMap::new();
        assert!(validate_command("ls", &[], &mut cache));
        assert!(validate_command("cd", &[], &mut cache));
        assert!(!validate_command(
            "thiscommandcertainlydoesnotexist",
            &[],
            &mut cache
        ));
        // Second calls must hit the cache (no regression in result).
        assert!(validate_command("ls", &[], &mut cache));
        assert!(!validate_command(
            "thiscommandcertainlydoesnotexist",
            &[],
            &mut cache
        ));
    }

    #[test]
    fn test_validate_command_path_forms() {
        let mut cache = HashMap::new();
        // Relative and absolute paths are considered valid without PATH lookup.
        assert!(validate_command("./run.sh", &[], &mut cache));
        assert!(validate_command("../scripts/build.sh", &[], &mut cache));
        assert!(validate_command("/usr/bin/env", &[], &mut cache));
    }

    #[test]
    fn test_validate_command_var_expansion() {
        let mut cache = HashMap::new();
        let env_vars = vec![("MYBIN".to_string(), "ls".to_string())];
        // $MYBIN should expand to `ls` which exists.
        assert!(validate_command("$MYBIN -la", &env_vars, &mut cache));
        // $GHOST expands to a non-existent command.
        let env_vars2 = vec![(
            "GHOST".to_string(),
            "thiscommandcertainlydoesnotexist".to_string(),
        )];
        assert!(!validate_command("$GHOST", &env_vars2, &mut cache));
    }

    #[test]
    fn test_validate_command_empty() {
        let mut cache = HashMap::new();
        // Empty command string → first token absent → treated as valid.
        assert!(validate_command("", &[], &mut cache));
        assert!(validate_command("   ", &[], &mut cache));
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
        assert_eq!(strip_quotes("'hello\""), "'hello\""); // mismatched quotes
        assert_eq!(strip_quotes("'"), "'"); // single char, no panic
        assert_eq!(strip_quotes(""), "");
    }

    #[test]
    fn test_expand_home() {
        let home = home::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/foo"), home.join("foo"));
        assert_eq!(expand_home("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_home("rel/path"), PathBuf::from("rel/path"));
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
