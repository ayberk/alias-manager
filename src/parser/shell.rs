use crate::models::Alias;
use crate::parser::{clean_description, extract_tags, resolve_path};
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ALIAS_RE: OnceLock<Regex> = OnceLock::new();
static SOURCE_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_file_recursive(
    path: &Path,
    aliases: &mut Vec<Alias>,
    visited: &mut HashSet<PathBuf>,
    env_vars: &[(String, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical_path = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };

    if !visited.insert(canonical_path) {
        return Ok(());
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let alias_re =
        ALIAS_RE.get_or_init(|| Regex::new(r"^alias\s+([a-zA-Z0-9._-]+)(?:=|\s+)(.*)").unwrap());
    let source_re = SOURCE_RE.get_or_init(|| Regex::new(r"(?:source|\.)\s+([^\s;]+)").unwrap());

    let mut last_comment: Option<String> = None;

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();

        if line.starts_with('#') {
            let comment = line.trim_start_matches('#').trim().to_string();
            if !comment.is_empty() {
                last_comment = Some(comment);
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        if let Some(caps) = alias_re.captures(line) {
            let name = caps.get(1).unwrap().as_str().trim().to_string();
            let mut command_part = caps.get(2).unwrap().as_str().trim();

            if let Some(pos) = command_part.find('#') {
                command_part = command_part[..pos].trim();
            }

            if (command_part.starts_with('\'') && command_part.ends_with('\''))
                || (command_part.starts_with('"') && command_part.ends_with('"'))
            {
                command_part = &command_part[1..command_part.len() - 1];
            }

            let tags = extract_tags(last_comment.as_deref());
            let description = clean_description(last_comment.take());

            aliases.push(Alias {
                name,
                command: command_part.to_string(),
                source_file: path.to_path_buf(),
                line_number: idx + 1,
                is_conflicting: false,
                is_broken: false,
                description,
                usage_count: 0,
                shadows: Vec::new(),
                duplicates: Vec::new(),
                tags,
                last_used: None,
                expanded_command: None,
            });
        } else {
            last_comment = None;
        }

        for caps in source_re.captures_iter(line) {
            let mut source_path_str = caps.get(1).unwrap().as_str().trim();
            if (source_path_str.starts_with('\'') && source_path_str.ends_with('\''))
                || (source_path_str.starts_with('"') && source_path_str.ends_with('"'))
            {
                source_path_str = &source_path_str[1..source_path_str.len() - 1];
            }
            if let Some(p) = resolve_path(source_path_str, parent_dir, env_vars) {
                let _ = parse_file_recursive(&p, aliases, visited, env_vars);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_file_recursive() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join(".zshrc");
        let mut file = fs::File::create(&file_path)?;
        writeln!(
            file,
            "# @work @utils This is a status command\nalias gs='git status' # with comment\nalias l=\"ls -la\""
        )?;

        // Test Fish-style alias
        writeln!(file, "alias fcmd echo fish")?;

        // Test source/include
        let include_path = dir.path().join("included.sh");
        let mut inc_file = fs::File::create(&include_path)?;
        writeln!(inc_file, "alias inc='echo included'")?;

        let include_path_str = include_path.to_str().unwrap();
        writeln!(file, "source {}", include_path_str)?;

        let mut aliases = Vec::new();
        let mut visited = HashSet::new();
        let env_vars = Vec::new();
        parse_file_recursive(&file_path, &mut aliases, &mut visited, &env_vars)?;

        assert_eq!(aliases.len(), 4);
        let gs = aliases.iter().find(|a| a.name == "gs").unwrap();
        assert_eq!(gs.command, "git status");
        assert_eq!(gs.tags, vec!["work".to_string(), "utils".to_string()]);
        assert_eq!(gs.description, Some("This is a status command".to_string()));

        let fcmd = aliases.iter().find(|a| a.name == "fcmd").unwrap();
        assert_eq!(fcmd.command, "echo fish");

        Ok(())
    }
}
