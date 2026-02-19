mod app;
mod models;
mod parser;
mod ui;

use crate::models::{App, Config, Focus};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use std::io;
use std::sync::mpsc;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dump_mode = args.contains(&"--dump".to_string());

    if dump_mode {
        let mut aliases = parser::get_all_aliases();
        aliases.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        for alias in &aliases {
            let source_name = alias
                .source_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let cmd = alias.expanded_command.as_ref().unwrap_or(&alias.command);
            println!("{:<15} | {:<12} | {}", alias.name, source_name, cmd);
        }
        return Ok(());
    }

    let cfg = Config::load();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let aliases = parser::get_all_aliases();
        let _ = tx.send(aliases);
    });

    let app = App::new(&cfg);
    let res = run_app(&mut terminal, app, &rx);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend + io::Write>(
    terminal: &mut Terminal<B>,
    mut app: App,
    rx: &mpsc::Receiver<Vec<crate::models::Alias>>,
) -> io::Result<()> {
    loop {
        if let Some((_, time)) = &app.last_action
            && time.elapsed().as_secs() >= 2
        {
            app.last_action = None;
        }

        terminal.draw(|f| ui::ui(f, &mut app))?;

        if app.is_loading
            && let Ok(aliases) = rx.try_recv()
        {
            app.loaded(aliases);
        }

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Tab => {
                    app.focus = match app.focus {
                        Focus::Filter => Focus::Aliases,
                        Focus::Aliases => Focus::Filter,
                    };
                    app.input_mode = app.focus == Focus::Filter;
                }
                _ => {
                    if app.focus == Focus::Filter && app.input_mode {
                        match key.code {
                            KeyCode::Enter | KeyCode::Esc => {
                                app.input_mode = false;
                                app.focus = Focus::Aliases;
                                app.save_config();
                            }
                            KeyCode::Char(c) => {
                                app.filter_query.push(c);
                                app.apply_filter();
                            }
                            KeyCode::Backspace => {
                                app.filter_query.pop();
                                app.apply_filter();
                            }
                            _ => {}
                        }
                    } else if app.focus == Focus::Aliases {
                        match key.code {
                            KeyCode::Char('q') => {
                                app.save_config();
                                return Ok(());
                            }
                            KeyCode::Char('/') => {
                                app.focus = Focus::Filter;
                                app.input_mode = true;
                            }
                            KeyCode::Char('c') => {
                                app.filter_query.clear();
                                app.apply_filter();
                                app.save_config();
                            }
                            KeyCode::Char('h') => {
                                app.show_source = !app.show_source;
                                app.save_config();
                            }
                            KeyCode::Char('d') => {
                                app.toggle_details();
                            }
                            KeyCode::Char('s') => {
                                app.toggle_sort();
                            }
                            KeyCode::Char('?') => {
                                app.show_help = !app.show_help;
                                app.save_config();
                            }
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let is_extended = matches!(key.code, KeyCode::Char('Y'));
                                if let Some(selected) = app.list_state.selected()
                                    && let Some(alias) = app.filtered_aliases.get(selected)
                                {
                                    let text_to_copy = if is_extended {
                                        alias.yank_command(true)
                                    } else {
                                        alias.name.clone()
                                    };

                                    match arboard::Clipboard::new() {
                                        Ok(mut cb) => {
                                            if let Err(e) = cb.set_text(text_to_copy.clone()) {
                                                app.set_action(&format!("Err: {}", e));
                                            } else {
                                                let truncated = if text_to_copy.len() > 15 {
                                                    format!("{}...", &text_to_copy[..12])
                                                } else {
                                                    text_to_copy.clone()
                                                };
                                                app.set_action(&format!("Yanked: {}", truncated));
                                            }
                                        }
                                        Err(_) => app.set_action("Clipboard init failed"),
                                    }
                                }
                            }
                            KeyCode::Char('e') => {
                                if let Some(selected) = app.list_state.selected()
                                    && selected < app.filtered_aliases.len()
                                {
                                    let alias = &app.filtered_aliases[selected];
                                    let editor = std::env::var("EDITOR")
                                        .unwrap_or_else(|_| "vi".to_string());

                                    disable_raw_mode()?;
                                    execute!(
                                        terminal.backend_mut(),
                                        LeaveAlternateScreen,
                                        DisableMouseCapture
                                    )?;

                                    let mut cmd = std::process::Command::new(&editor);
                                    if editor.contains("vi")
                                        || editor.contains("nano")
                                        || editor.contains("emacs")
                                    {
                                        cmd.arg(format!("+{}", alias.line_number));
                                    } else if editor.contains("code") {
                                        cmd.arg("--goto").arg(format!(
                                            "{}:{}",
                                            alias.source_file.display(),
                                            alias.line_number
                                        ));
                                    }
                                    if !editor.contains("code") {
                                        cmd.arg(&alias.source_file);
                                    }
                                    let _ = cmd.status();

                                    let updated_aliases = parser::get_all_aliases();
                                    app.loaded(updated_aliases);

                                    enable_raw_mode()?;
                                    execute!(
                                        terminal.backend_mut(),
                                        EnterAlternateScreen,
                                        EnableMouseCapture
                                    )?;
                                    terminal.clear()?;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => app.next(),
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
