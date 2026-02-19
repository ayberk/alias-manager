use crate::models::Alias;
use crate::parser::{clean_description, extract_tags, strip_quotes};
use regex::Regex;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static NIX_DQ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^([a-zA-Z0-9_.\-]+)\s*=\s*"([^"]*)"[;,]?\s*(?:#.*)?$"#).unwrap()
});
static NIX_SQ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^([a-zA-Z0-9_.\-]+)\s*=\s*'([^']*)'[;,]?\s*(?:#.*)?$"#).unwrap()
});
static NIX_SHELL_ALIAS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^alias\s+([a-zA-Z0-9._-]+)=(.+)$").unwrap());

// Count net brace depth in a line, ignoring braces inside strings and after '#'.
fn net_brace_depth(line: &str) -> i32 {
    let mut depth: i32 = 0;
    let mut in_str = false;
    for ch in line.chars() {
        match ch {
            '"' => in_str = !in_str,
            '#' if !in_str => break,
            '{' if !in_str => depth += 1,
            '}' if !in_str => depth -= 1,
            _ => {}
        }
    }
    depth
}

pub fn parse_nix_file(path: &Path, aliases: &mut Vec<Alias>) -> io::Result<()> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    // name = "value"; or name = "value",  (with optional trailing comment)
    let dq_re = &*NIX_DQ_RE;
    let sq_re = &*NIX_SQ_RE;

    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();
    let mut i = 0;

    while i < n {
        let trimmed = lines[i].trim();

        if trimmed.contains("shellAliases") {
            let mut depth: i32 = 0;
            let mut in_block = false;
            let mut last_comment: Option<String> = None;

            while i < n {
                let line = lines[i].trim();
                let net = net_brace_depth(line);
                depth += net;

                if depth > 0 {
                    if !in_block {
                        // First line where depth goes positive — the opening line; skip alias parsing.
                        in_block = true;
                    } else if depth == 1 {
                        if line.starts_with('#') {
                            let c = line.trim_start_matches('#').trim().to_string();
                            if !c.is_empty() {
                                last_comment = Some(c);
                            }
                        } else if !line.is_empty() {
                            let parsed = dq_re
                                .captures(line)
                                .or_else(|| sq_re.captures(line))
                                .map(|caps| (caps[1].to_string(), caps[2].to_string()));

                            if let Some((name, command)) = parsed {
                                let tags = extract_tags(last_comment.as_deref());
                                let description = clean_description(last_comment.take());
                                aliases.push(Alias {
                                    name,
                                    command,
                                    source_file: path.to_path_buf(),
                                    line_number: i + 1,
                                    description,
                                    tags,
                                    ..Default::default()
                                });
                            }
                            last_comment = None;
                        }
                    }
                } else if in_block {
                    i += 1;
                    break;
                }

                i += 1;
            }
            continue;
        }

        i += 1;
    }

    // Second pass: pick up shell-style `alias name=value` lines inside nix string literals
    // (e.g. interactiveShellInit = '' alias ls=lsd ... '').
    let shell_alias_re = &*NIX_SHELL_ALIAS_RE;
    let mut last_comment: Option<String> = None;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let c = trimmed.trim_start_matches('#').trim().to_string();
            if !c.is_empty() {
                last_comment = Some(c);
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = shell_alias_re.captures(trimmed) {
            let name = caps[1].to_string();
            let value = strip_quotes(caps[2].trim());
            let tags = extract_tags(last_comment.as_deref());
            let description = clean_description(last_comment.take());
            aliases.push(Alias {
                name,
                command: value.to_string(),
                source_file: path.to_path_buf(),
                line_number: idx + 1,
                description,
                tags,
                ..Default::default()
            });
        } else {
            last_comment = None;
        }
    }

    Ok(())
}

pub fn parse_nix_dir(dir: &Path, aliases: &mut Vec<Alias>, max_depth: usize) {
    if max_depth == 0 || !dir.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        let mut sorted: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        sorted.sort();
        for path in sorted {
            if path.is_file() {
                if path.extension().and_then(|e| e.to_str()) == Some("nix") {
                    let _ = parse_nix_file(&path, aliases);
                }
            } else if path.is_dir() {
                parse_nix_dir(&path, aliases, max_depth - 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nix_file() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("system.nix");
        fs::write(
            &file_path,
            r#"{ pkgs, ... }: {
  environment.shellAliases = {
    # list files
    ls = "lsd";
    # @work git shortcut
    gs = "git status";
    nix-rebuild = "sudo darwin-rebuild switch --flake ~/.config/nix";
  };
}"#,
        )?;

        let mut aliases = Vec::new();
        parse_nix_file(&file_path, &mut aliases)?;

        assert_eq!(aliases.len(), 3);

        let ls = aliases.iter().find(|a| a.name == "ls").unwrap();
        assert_eq!(ls.command, "lsd");
        assert_eq!(ls.description, Some("list files".to_string()));

        let gs = aliases.iter().find(|a| a.name == "gs").unwrap();
        assert_eq!(gs.command, "git status");
        assert_eq!(gs.tags, vec!["work".to_string()]);
        assert_eq!(gs.description, Some("git shortcut".to_string()));

        let nr = aliases.iter().find(|a| a.name == "nix-rebuild").unwrap();
        assert_eq!(
            nr.command,
            "sudo darwin-rebuild switch --flake ~/.config/nix"
        );

        Ok(())
    }

    #[test]
    fn test_parse_nix_file_multiline_opener() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("home.nix");
        fs::write(
            &file_path,
            r#"{ ... }: {
  home.shellAliases =
  {
    cat = "bat";
  };
}"#,
        )?;

        let mut aliases = Vec::new();
        parse_nix_file(&file_path, &mut aliases)?;

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "cat");
        assert_eq!(aliases[0].command, "bat");

        Ok(())
    }

    #[test]
    fn test_parse_nix_file_multiple_blocks() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("config.nix");
        fs::write(
            &file_path,
            r#"{ ... }: {
  environment.shellAliases = {
    ls = "lsd";
  };
  programs.zsh.shellAliases = {
    vim = "nvim";
  };
}"#,
        )?;

        let mut aliases = Vec::new();
        parse_nix_file(&file_path, &mut aliases)?;

        assert_eq!(aliases.len(), 2);
        assert!(aliases.iter().any(|a| a.name == "ls" && a.command == "lsd"));
        assert!(
            aliases
                .iter()
                .any(|a| a.name == "vim" && a.command == "nvim")
        );

        Ok(())
    }

    #[test]
    fn test_net_brace_depth() {
        assert_eq!(net_brace_depth("environment.shellAliases = {"), 1);
        assert_eq!(net_brace_depth("  };"), -1);
        assert_eq!(net_brace_depth(r#"  ls = "lsd";"#), 0);
        assert_eq!(net_brace_depth("  # { this brace is in a comment"), 0);
        assert_eq!(net_brace_depth(r#"  foo = "bar { baz }";"#), 0);
    }
}
