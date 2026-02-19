mod app;
mod models;
mod parser;
mod ui;

use crate::models::{App, Config, KeyAction};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
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

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dump_arg = std::env::args().find(|a| a == "--dump" || a.starts_with("--dump="));

    if let Some(arg) = dump_arg {
        let format = arg.strip_prefix("--dump=").unwrap_or("table");

        let cfg = Config::load();
        let mut aliases = parser::get_all_aliases(&cfg.nix_paths);
        aliases.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        match format {
            "json" => {
                println!("[");
                for (i, alias) in aliases.iter().enumerate() {
                    let cmd = alias.expanded_command.as_ref().unwrap_or(&alias.command);
                    let comma = if i + 1 < aliases.len() { "," } else { "" };
                    println!(
                        "  {{\"name\":{},\"source\":{},\"command\":{}}}{}",
                        json_str(&alias.name),
                        json_str(alias.source_name()),
                        json_str(cmd),
                        comma
                    );
                }
                println!("]");
            }
            "csv" => {
                println!("name,source,command");
                for alias in &aliases {
                    let cmd = alias.expanded_command.as_ref().unwrap_or(&alias.command);
                    println!(
                        "{},{},{}",
                        csv_field(&alias.name),
                        csv_field(alias.source_name()),
                        csv_field(cmd)
                    );
                }
            }
            _ => {
                for alias in &aliases {
                    let cmd = alias.expanded_command.as_ref().unwrap_or(&alias.command);
                    println!("{:<15} | {:<12} | {}", alias.name, alias.source_name(), cmd);
                }
            }
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

    let extra_nix = cfg.nix_paths.clone();
    thread::spawn(move || {
        let aliases = parser::get_all_aliases(&extra_nix);
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

        if app.is_loading {
            match rx.try_recv() {
                Ok(aliases) => app.loaded(aliases),
                Err(mpsc::TryRecvError::Disconnected) => {
                    app.is_loading = false;
                    app.set_action("Failed to load aliases");
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match app.handle_key(key) {
                KeyAction::Quit => return Ok(()),
                KeyAction::OpenEditor {
                    source_file,
                    line_number,
                    editor,
                } => {
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;

                    let mut cmd = std::process::Command::new(&editor);
                    if editor.contains("vi") || editor.contains("nano") || editor.contains("emacs")
                    {
                        cmd.arg(format!("+{line_number}"));
                    } else if editor.contains("code") {
                        cmd.arg("--goto")
                            .arg(format!("{}:{line_number}", source_file.display()));
                    }
                    if !editor.contains("code") {
                        cmd.arg(&source_file);
                    }
                    let _ = cmd.status();

                    let updated_aliases = parser::get_all_aliases(&app.nix_paths);
                    app.loaded(updated_aliases);

                    enable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        EnterAlternateScreen,
                        EnableMouseCapture
                    )?;
                    terminal.clear()?;
                }
                KeyAction::ExcludeSource(source_file) => {
                    if !app.ignore_sources.contains(&source_file) {
                        app.ignore_sources.push(source_file);
                    }
                    let _ = app.save_config();
                    let updated_aliases = parser::get_all_aliases(&app.nix_paths);
                    app.loaded(updated_aliases);
                    app.set_action("Source excluded");
                }
                KeyAction::Continue => {}
            }
        }
    }
}
