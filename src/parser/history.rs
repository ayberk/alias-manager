use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub struct HistoryInfo {
    pub count: usize,
    pub last_used: u64,
}

pub fn parse_history(home: &Path) -> HashMap<String, HistoryInfo> {
    let mut history = HashMap::new();

    // Try Zsh history
    let zsh_hist = home.join(".zsh_history");
    if let Ok(file) = File::open(&zsh_hist) {
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            // Zsh extended history format: : 1678900000:0;command args...
            if line.starts_with(": ") {
                if let Some(cmd_start) = line.find(';') {
                    let metadata = &line[2..cmd_start];
                    let timestamp = metadata
                        .split(':')
                        .next()
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let cmd_line = &line[cmd_start + 1..];
                    if let Some(cmd) = cmd_line.split_whitespace().next() {
                        let info = history.entry(cmd.to_string()).or_insert(HistoryInfo {
                            count: 0,
                            last_used: 0,
                        });
                        info.count += 1;
                        if timestamp > info.last_used {
                            info.last_used = timestamp;
                        }
                    }
                }
            } else if let Some(cmd) = line.split_whitespace().next() {
                let info = history.entry(cmd.to_string()).or_insert(HistoryInfo {
                    count: 0,
                    last_used: 0,
                });
                info.count += 1;
            }
        }
    }

    // Try Bash history (usually no timestamps unless configured)
    let bash_hist = home.join(".bash_history");
    if let Ok(file) = File::open(&bash_hist) {
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some(cmd) = trimmed.split_whitespace().next() {
                let info = history.entry(cmd.to_string()).or_insert(HistoryInfo {
                    count: 0,
                    last_used: 0,
                });
                info.count += 1;
            }
        }
    }

    // Try Fish history (~/.local/share/fish/fish_history)
    // Format is YAML-like:
    // - cmd: ls -la
    //   when: 1678900000
    let fish_hist = home.join(".local/share/fish/fish_history");
    if let Ok(file) = File::open(&fish_hist) {
        let reader = BufReader::new(file);
        let mut current_cmd: Option<String> = None;
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if let Some(cmd_part) = line.strip_prefix("- cmd: ") {
                if let Some(cmd) = cmd_part.split_whitespace().next() {
                    current_cmd = Some(cmd.to_string());
                    let info = history.entry(cmd.to_string()).or_insert(HistoryInfo {
                        count: 0,
                        last_used: 0,
                    });
                    info.count += 1;
                }
            } else if let Some(when_part) = line.strip_prefix("when: ")
                && let Some(cmd) = &current_cmd
                && let Ok(timestamp) = when_part.parse::<u64>()
                && let Some(info) = history.get_mut(cmd)
                && timestamp > info.last_used
            {
                info.last_used = timestamp;
            }
        }
    }

    history
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_history() -> std::io::Result<()> {
        let dir = tempdir()?;
        let home = dir.path();

        // Mock Zsh history
        let zsh_file = home.join(".zsh_history");
        let mut f = File::create(zsh_file)?;
        writeln!(f, ": 1000000000:0;ls -la")?;
        writeln!(f, ": 2000000000:0;ls")?;

        let history = parse_history(home);
        let ls_info = history.get("ls").unwrap();
        assert_eq!(ls_info.count, 2);
        assert_eq!(ls_info.last_used, 2000000000);

        // Mock Fish history
        let fish_dir = home.join(".local/share/fish");
        std::fs::create_dir_all(&fish_dir)?;
        let fish_file = fish_dir.join("fish_history");
        let mut f = File::create(fish_file)?;
        writeln!(f, "- cmd: grep pattern\n  when: 1600000000")?;
        writeln!(f, "- cmd: ls -la\n  when: 3000000000")?;

        let history = parse_history(home);
        assert_eq!(history.get("grep").unwrap().count, 1);
        assert_eq!(history.get("ls").unwrap().last_used, 3000000000);
        assert_eq!(history.get("ls").unwrap().count, 3); // 2 from Zsh + 1 from Fish

        Ok(())
    }
}
