use crate::models::Alias;
use crate::parser::{clean_description, extract_tags, resolve_path, strip_quotes};
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static ALIAS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^alias\s+([a-zA-Z0-9._-]+)(?:=|\s+)(.*)").unwrap());
static SOURCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:source|\.)\s+([^\s;]+)").unwrap());

/// Strip a trailing `# comment` from a shell value, but only outside quoted regions.
fn strip_inline_comment(s: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return s[..i].trim_end(),
            _ => {}
        }
    }
    s
}

pub fn parse_file_recursive(
    path: &Path,
    aliases: &mut Vec<Alias>,
    visited: &mut HashSet<PathBuf>,
    env_vars: &[(String, String)],
) -> io::Result<()> {
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
    let alias_re = &*ALIAS_RE;
    let source_re = &*SOURCE_RE;

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

            command_part = strip_inline_comment(command_part);
            command_part = strip_quotes(command_part);

            let tags = extract_tags(last_comment.as_deref());
            let description = clean_description(last_comment.take());

            aliases.push(Alias {
                name,
                command: command_part.to_string(),
                source_file: path.to_path_buf(),
                line_number: idx + 1,
                description,
                tags,
                ..Default::default()
            });
        } else {
            last_comment = None;
        }

        for caps in source_re.captures_iter(line) {
            let source_path_str = strip_quotes(caps.get(1).unwrap().as_str().trim());
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
    fn test_parse_file_recursive() -> io::Result<()> {
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

    #[test]
    fn test_hash_inside_quotes_not_stripped() -> io::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join(".zshrc");
        fs::write(
            &file_path,
            "alias h='git log --oneline # pretty' # actual comment\nalias h2=\"git log --format='%h'\"\n",
        )?;
        let mut aliases = Vec::new();
        let mut visited = HashSet::new();
        parse_file_recursive(&file_path, &mut aliases, &mut visited, &[])?;
        let h = aliases.iter().find(|a| a.name == "h").unwrap();
        assert_eq!(h.command, "git log --oneline # pretty");
        let h2 = aliases.iter().find(|a| a.name == "h2").unwrap();
        assert_eq!(h2.command, "git log --format='%h'");
        Ok(())
    }

    #[test]
    fn test_strip_inline_comment() {
        assert_eq!(strip_inline_comment("git status # desc"), "git status");
        assert_eq!(
            strip_inline_comment("'git status # no'"),
            "'git status # no'"
        );
        assert_eq!(strip_inline_comment("\"git log\" # end"), "\"git log\"");
        assert_eq!(strip_inline_comment("no comment"), "no comment");
    }
}
