//! gim: a TUI for managing git branches.

mod app;
mod config;
mod git;
mod input;
mod ops;
mod ui;

use std::{
    env,
    path::PathBuf,
    process::{Command, ExitCode},
};

use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;

const USAGE: &str = "usage: gim [-c|--config <path>]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gim: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut config_path = config::default_path();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => {
                config_path = Some(PathBuf::from(args.next().ok_or(USAGE)?));
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unexpected argument {arg:?}\n{USAGE}")),
        }
    }

    let cfg = match &config_path {
        Some(p) => config::load(p)?,
        None => config::Config::default(),
    };
    let mut app = App::new(git::Repo::new("."), cfg)?;

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result.map_err(|e| e.to_string())
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
        if let Some(req) = app.editor_request.take() {
            // Hand the terminal to the editor (terminal editors need it; GUI ones return at once).
            ratatui::restore();
            let result = Command::new(&req.program)
                .args(&req.args)
                .current_dir(&req.dir)
                .status();
            *terminal = ratatui::try_init()?;
            app.editor_done(result);
        }
    }
    Ok(())
}
