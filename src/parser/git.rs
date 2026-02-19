use crate::models::Alias;
use crate::parser::{clean_description, extract_tags};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

static GIT_SECTION_RE: OnceLock<Regex> = OnceLock::new();
static GIT_ALIAS_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_git_config(
    path: &Path,
    aliases: &mut Vec<Alias>,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let mut in_alias_section = false;
    let section_re = GIT_SECTION_RE.get_or_init(|| Regex::new(r"^\[([^\]]+)\]").unwrap());
    let alias_re = GIT_ALIAS_RE.get_or_init(|| Regex::new(r"^([^=\s]+)\s*=\s*(.*)").unwrap());

    let mut last_comment: Option<String> = None;

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();

        if line.starts_with('#') || line.starts_with(';') {
            let comment = line.trim_start_matches(['#', ';']).trim().to_string();
            if !comment.is_empty() {
                last_comment = Some(comment);
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        if let Some(caps) = section_re.captures(line) {
            let section = caps.get(1).unwrap().as_str();
            in_alias_section = section == "alias";
            last_comment = None;
            continue;
        }

        if in_alias_section {
            if let Some(caps) = alias_re.captures(line) {
                let name = caps.get(1).unwrap().as_str().to_string();
                let command = caps.get(2).unwrap().as_str().to_string();
                let tags = extract_tags(last_comment.as_deref());
                let description = clean_description(last_comment.take());

                aliases.push(Alias {
                    name: format!("git {}", name),
                    command,
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
        } else {
            last_comment = None;
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
    fn test_parse_git_config() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let file_path = dir.path().join(".gitconfig");
        let mut file = fs::File::create(&file_path)?;
        writeln!(
            file,
            "[alias]\n    # @git This is a status shortcut\n    st = status\n    co = checkout"
        )?;

        let mut aliases = Vec::new();
        parse_git_config(&file_path, &mut aliases)?;

        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "git st");
        assert_eq!(aliases[0].tags, vec!["git".to_string()]);
        assert_eq!(
            aliases[0].description,
            Some("This is a status shortcut".to_string())
        );
        Ok(())
    }
}
