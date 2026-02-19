use crate::models::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use regex::Regex;
use std::sync::OnceLock;

static BASH_TOKEN_RE: OnceLock<Regex> = OnceLock::new();

pub fn ui(f: &mut Frame, app: &mut App) {
    // 1. Prepare details if visible
    let mut details_lines = Vec::new();
    if app.show_details
        && let Some(selected) = app.list_state.selected()
        && !app.filtered_aliases.is_empty()
        && let Some(alias) = app.filtered_aliases.get(selected)
    {
        let source_name = alias
            .source_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let icon = get_file_icon(source_name);

        details_lines.push(Line::from(vec![
            Span::styled("Source:  ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "{} {}:{}",
                    icon,
                    alias.source_file.display(),
                    alias.line_number
                ),
                Style::default().fg(Color::Blue),
            ),
        ]));

        // Usage (Always shown)
        let last_used_str = alias
            .last_used
            .map(format_last_used)
            .unwrap_or_else(|| "Never".to_string());
        let usage_suffix = if alias.usage_count == 1 {
            "use"
        } else {
            "uses"
        };
        details_lines.push(Line::from(vec![
            Span::styled("Usage:   ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "{} ({} total {})",
                    last_used_str, alias.usage_count, usage_suffix
                ),
                Style::default().fg(Color::Green),
            ),
        ]));
        // Info (Only shown if exists)
        if let Some(desc) = &alias.description {
            details_lines.push(Line::from(vec![
                Span::styled("Info:    ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(desc, Style::default().fg(Color::Yellow)),
            ]));
        }

        if !alias.tags.is_empty() {
            let tag_spans: Vec<Span> = alias
                .tags
                .iter()
                .map(|t| {
                    Span::styled(
                        format!("@{}", t),
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect();

            let mut line_spans = vec![Span::styled(
                "Tags:    ",
                Style::default().add_modifier(Modifier::BOLD),
            )];
            for (i, span) in tag_spans.into_iter().enumerate() {
                if i > 0 {
                    line_spans.push(Span::raw(" "));
                }
                line_spans.push(span);
            }
            details_lines.push(Line::from(line_spans));
        }

        if !alias.duplicates.is_empty() {
            details_lines.push(Line::from(vec![
                Span::styled(
                    "Duplicates: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    alias.duplicates.join(", "),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }

        if let Some(expanded) = &alias.expanded_command {
            let mut line_spans = vec![Span::styled(
                "Command: ",
                Style::default().add_modifier(Modifier::BOLD),
            )];
            line_spans.extend(highlight_bash(expanded));
            details_lines.push(Line::from(line_spans));
        }

        if alias.is_broken {
            details_lines.push(Line::from(vec![
                Span::styled(
                    "STATUS:  ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "[X] Broken - Command not found in PATH",
                    Style::default().fg(Color::Red),
                ),
            ]));
        } else if alias.is_conflicting {
            details_lines.push(Line::from(vec![
                Span::styled(
                    "STATUS:  ",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "[!] Conflict - This alias is overridden",
                    Style::default().fg(Color::LightRed),
                ),
            ]));

            for shadow in &alias.shadows {
                details_lines.push(Line::from(vec![
                    Span::raw("         Shadowed: "),
                    Span::styled(
                        format!("{}:{}", shadow.source_file.display(), shadow.line_number),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" ("),
                    Span::styled(
                        &shadow.command,
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(")"),
                ]));
            }
        }
    }

    // 2. Setup dynamic constraints
    let mut constraints = vec![
        Constraint::Length(3), // Header
        Constraint::Min(0),    // List
    ];

    if app.show_details {
        // Height is number of lines + 2 for borders (maxed out at 10 or similar if needed, but here simple)
        let height = (details_lines.len() as u16 + 2).max(3);
        constraints.push(Constraint::Length(height));
    }

    if app.show_help {
        constraints.push(Constraint::Length(3)); // Help
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.size());

    // 3. Render Header
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let title_text = if app.is_loading {
        "Alias Manager (Loading...)".to_string()
    } else {
        format!(
            "Alias Manager ({} aliases) [Sort: {:?}]",
            app.filtered_aliases.len(),
            app.sort_field
        )
    };

    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, header_chunks[0]);

    let filter_text = if app.focus == Focus::Filter && app.input_mode {
        format!("{}|", app.filter_query)
    } else {
        app.filter_query.clone()
    };
    let filter_para = Paragraph::new(filter_text)
        .style(if app.focus == Focus::Filter {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Filter")
                .border_style(if app.focus == Focus::Filter {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                }),
        );
    f.render_widget(filter_para, header_chunks[1]);

    // 4. Render List
    if app.is_loading {
        let loading = Paragraph::new("Loading aliases...")
            .block(Block::default().borders(Borders::ALL).title("Aliases"));
        f.render_widget(loading, chunks[1]);
    } else {
        let items: Vec<ListItem> = app
            .filtered_aliases
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let mut line_spans = Vec::new();
                let mut name_style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                let mut name_prefix = String::new();

                if a.is_broken {
                    name_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
                    name_prefix.push_str("[X] ");
                } else if a.is_conflicting {
                    name_style = Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD);
                    name_prefix.push_str("[!] ");
                } else {
                    name_prefix.push_str("    ");
                }

                line_spans.push(Span::styled(
                    format!("{}{:<15} ", name_prefix, a.name),
                    name_style,
                ));
                line_spans.push(Span::raw("| "));

                if app.show_source {
                    let source_name = a
                        .source_file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    let icon = get_file_icon(source_name);
                    let source_style =
                        if app.list_state.selected() == Some(i) && app.focus == Focus::Aliases {
                            Style::default().fg(Color::Gray)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        };
                    line_spans.push(Span::styled(
                        format!("{} {:<12} ", icon, source_name),
                        source_style,
                    ));
                    line_spans.push(Span::raw("| "));
                }
                line_spans.extend(highlight_bash(&a.command));

                if a.usage_count > 0 {
                    let usage_suffix = if a.usage_count == 1 { "use" } else { "uses" };
                    line_spans.push(Span::raw(" | "));
                    line_spans.push(Span::styled(
                        format!("{} {}", a.usage_count, usage_suffix),
                        Style::default().fg(Color::Green),
                    ));
                }

                // Show feedback on the selected line if there is a last_action
                if let Some(selected) = app.list_state.selected()
                    && selected == i
                    && let Some((msg, _)) = &app.last_action
                {
                    line_spans.push(Span::raw("   "));
                    line_spans.push(Span::styled(
                        format!("[{}]", msg),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                ListItem::new(Line::from(line_spans))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Aliases")
                    .border_style(if app.focus == Focus::Aliases {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    }),
            )
            .highlight_style(if app.focus == Focus::Aliases {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            })
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, chunks[1], &mut app.list_state);
    }

    let mut next_chunk = 2;

    // 5. Render Details
    if app.show_details {
        let details_widget = if details_lines.is_empty() {
            Paragraph::new("Select an alias.")
                .block(Block::default().borders(Borders::ALL).title("Details"))
        } else {
            Paragraph::new(details_lines)
                .block(Block::default().borders(Borders::ALL).title("Details"))
        };
        f.render_widget(details_widget, chunks[next_chunk]);
        next_chunk += 1;
    }

    // 6. Render Help
    if app.show_help {
        let help_text = match app.focus {
            Focus::Filter => " <ENTER> finish | <TAB> switch focus ",
            Focus::Aliases => {
                " <q> quit | <TAB> focus | <j/k> nav | <s> sort | <d> details | <h> toggle src | <e> edit | <y/Y> yank/ext | <?> toggle help "
            }
        };
        let legend = " [X] Broken | [!] Conflict ";
        let full_help = format!("{} | {}", help_text, legend);

        f.render_widget(
            Paragraph::new(full_help)
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title("Help")),
            chunks[next_chunk],
        );
    }
}

pub fn highlight_bash(command: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let re = BASH_TOKEN_RE.get_or_init(|| Regex::new(r#"("[^"]*"|'[^']*'|&&|\|\||[|;]|\s+|-{1,2}[a-zA-Z0-9-]+|\$[a-zA-Z0-9_]+|\$\{[^}]+\}|[^\s;|"'$]+)"#).unwrap());

    let mut is_first = true;
    for caps in re.captures_iter(command) {
        let token = caps.get(0).unwrap().as_str();
        let trimmed = token.trim();

        if trimmed.is_empty() {
            spans.push(Span::raw(token.to_string()));
            continue;
        }

        let style = if trimmed == "&&" || trimmed == "||" || trimmed == "|" || trimmed == ";" {
            is_first = true;
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else if trimmed.starts_with('-') {
            Style::default().fg(Color::Yellow)
        } else if trimmed.starts_with('$') {
            Style::default().fg(Color::Cyan)
        } else if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            Style::default().fg(Color::LightRed)
        } else if is_first {
            is_first = false;
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        spans.push(Span::styled(token.to_string(), style));
    }

    if spans.is_empty() && !command.is_empty() {
        spans.push(Span::raw(command.to_string()));
    }

    spans
}

fn format_last_used(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if timestamp == 0 || timestamp > now {
        return "Unknown".to_string();
    }

    let diff = now - timestamp;
    let minutes = diff / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    let months = days / 30;
    let years = days / 365;

    if years > 0 {
        format!("{} year{} ago", years, if years > 1 { "s" } else { "" })
    } else if months > 0 {
        format!("{} month{} ago", months, if months > 1 { "s" } else { "" })
    } else if days > 0 {
        format!("{} day{} ago", days, if days > 1 { "s" } else { "" })
    } else if hours > 0 {
        format!("{} hour{} ago", hours, if hours > 1 { "s" } else { "" })
    } else if minutes > 0 {
        format!(
            "{} minute{} ago",
            minutes,
            if minutes > 1 { "s" } else { "" }
        )
    } else {
        "Just now".to_string()
    }
}

fn get_file_icon(filename: &str) -> &'static str {
    if filename == ".gitconfig" || filename.contains("git") {
        "󰊢"
    } else if filename == ".zshrc" || filename.ends_with(".zsh") {
        "󱆃"
    } else if filename == ".bashrc" || filename == ".bash_profile" || filename.ends_with(".sh") {
        "󱆅"
    } else if filename.contains("alias") {
        "󰈺"
    } else {
        "󰈮"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_bash() {
        let spans = highlight_bash("ls -la && echo $HOME");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::Green));
    }
}
