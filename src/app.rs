//! Application state and key handling.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::git::{Branch, Repo};
use crate::input::LineInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    Rename,
    /// Prompting for a new branch name to `checkout -b` from the cursor branch.
    Create,
    Confirm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

pub struct App {
    repo: Repo,
    pub cfg: Config,
    pub branches: Vec<Branch>,
    pub cursor: usize,
    /// Visual-mode anchor.
    pub anchor: usize,
    pub mode: Mode,
    /// Mode to return to when a prompt is cancelled.
    pub prev_mode: Mode,
    pub input: LineInput,
    /// Branch the input prompt acts on: the one being renamed, or the start point.
    pub input_target: String,
    pub pending_delete: Vec<String>,
    pub pending_force: bool,
    pub status: Option<Status>,
    pub list_state: ListState,
    pub quit: bool,
}

impl App {
    pub fn new(repo: Repo, cfg: Config) -> Result<Self, String> {
        let mut app = App {
            repo,
            cfg,
            branches: Vec::new(),
            cursor: 0,
            anchor: 0,
            mode: Mode::Normal,
            prev_mode: Mode::Normal,
            input: LineInput::default(),
            input_target: String::new(),
            pending_delete: Vec::new(),
            pending_force: false,
            status: None,
            list_state: ListState::default(),
            quit: false,
        };
        app.reload()?;
        if let Some(i) = app.branches.iter().position(|b| b.current) {
            app.cursor = i;
        }
        Ok(app)
    }

    fn reload(&mut self) -> Result<(), String> {
        self.branches = self.repo.list_branches()?;
        let last = self.branches.len().saturating_sub(1);
        self.cursor = self.cursor.min(last);
        self.anchor = self.anchor.min(last);
        Ok(())
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(Status {
            text: text.into(),
            error,
        });
    }

    /// Whether a visual selection is active (including while confirming its deletion).
    pub fn in_visual(&self) -> bool {
        self.mode == Mode::Visual || (self.mode == Mode::Confirm && self.prev_mode == Mode::Visual)
    }

    /// Inclusive index range the next action applies to.
    pub fn selection(&self) -> (usize, usize) {
        if self.in_visual() {
            (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
        } else {
            (self.cursor, self.cursor)
        }
    }

    fn selected_names(&self) -> Vec<String> {
        if self.branches.is_empty() {
            return Vec::new();
        }
        let (lo, hi) = self.selection();
        self.branches[lo..=hi]
            .iter()
            .map(|b| b.name.clone())
            .collect()
    }

    pub fn handle_key(&mut self, ev: KeyEvent) {
        match self.mode {
            Mode::Rename | Mode::Create => self.handle_input(ev),
            Mode::Confirm => self.handle_confirm(ev),
            Mode::Normal | Mode::Visual => self.handle_list(ev),
        }
    }

    fn handle_list(&mut self, ev: KeyEvent) {
        let k = &self.cfg.keys;
        let last = self.branches.len().saturating_sub(1);
        self.status = None;
        if k.quit.matches(&ev) {
            self.quit = true;
        } else if k.up.matches(&ev) {
            self.cursor = self.cursor.saturating_sub(1);
        } else if k.down.matches(&ev) {
            self.cursor = (self.cursor + 1).min(last);
        } else if k.top.matches(&ev) {
            self.cursor = 0;
        } else if k.bottom.matches(&ev) {
            self.cursor = last;
        } else if k.visual.matches(&ev) {
            if self.mode == Mode::Visual {
                self.mode = Mode::Normal;
            } else if !self.branches.is_empty() {
                self.mode = Mode::Visual;
                self.anchor = self.cursor;
            }
        } else if k.cancel.matches(&ev) {
            self.mode = Mode::Normal;
        } else if k.reload.matches(&ev) {
            if let Err(e) = self.reload() {
                self.set_status(e, true);
            }
        } else if k.delete.matches(&ev) {
            self.start_delete(false);
        } else if k.force_delete.matches(&ev) {
            self.start_delete(true);
        } else if k.rename.matches(&ev) {
            self.start_input(Mode::Rename);
        } else if k.checkout_new.matches(&ev) {
            self.start_input(Mode::Create);
        } else if k.checkout.matches(&ev) {
            self.checkout();
        }
    }

    fn start_input(&mut self, mode: Mode) {
        let Some(name) = self.branches.get(self.cursor).map(|b| b.name.clone()) else {
            return;
        };
        self.input
            .set(if mode == Mode::Rename { &name } else { "" });
        self.input_target = name;
        self.prev_mode = self.mode;
        self.mode = mode;
    }

    fn checkout(&mut self) {
        let Some(name) = self.branches.get(self.cursor).map(|b| b.name.clone()) else {
            return;
        };
        self.mode = Mode::Normal;
        match self.repo.checkout(&name).and_then(|_| self.reload()) {
            Ok(()) => self.set_status(format!("switched to {name}"), false),
            Err(e) => self.set_status(e, true),
        }
    }

    fn start_delete(&mut self, force: bool) {
        let names = self.selected_names();
        if names.is_empty() {
            return;
        }
        self.prev_mode = self.mode;
        if self.cfg.confirm_delete {
            self.pending_delete = names;
            self.pending_force = force;
            self.mode = Mode::Confirm;
        } else {
            self.delete(names, force);
        }
    }

    fn delete(&mut self, names: Vec<String>, force: bool) {
        // Keep the cursor where the deleted block started.
        let (lo, _) = self.selection();
        self.mode = Mode::Normal;
        let result = self.repo.delete_branches(&names, force);
        let reloaded = self.reload();
        self.cursor = lo.min(self.branches.len().saturating_sub(1));
        match result.and(reloaded) {
            Ok(()) => self.set_status(format!("deleted {}", names.join(", ")), false),
            Err(e) => self.set_status(e, true),
        }
    }

    fn handle_confirm(&mut self, ev: KeyEvent) {
        let names = std::mem::take(&mut self.pending_delete);
        match ev.code {
            KeyCode::Char('y' | 'Y') => self.delete(names, self.pending_force),
            KeyCode::Char('c') if ev.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            _ => {
                self.mode = self.prev_mode;
                self.set_status("cancelled", false);
            }
        }
    }

    fn handle_input(&mut self, ev: KeyEvent) {
        match ev.code {
            KeyCode::Esc => self.mode = self.prev_mode,
            KeyCode::Char('c') if ev.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Enter => {
                let mode = std::mem::replace(&mut self.mode, Mode::Normal);
                let target = self.input_target.clone();
                let new = self.input.value().trim().to_string();
                if new.is_empty() || (mode == Mode::Rename && new == target) {
                    return;
                }
                let (result, done) = if mode == Mode::Rename {
                    (
                        self.repo.rename_branch(&target, &new),
                        format!("renamed {target} -> {new}"),
                    )
                } else {
                    (
                        self.repo.checkout_new(&new, &target),
                        format!("switched to new branch {new}"),
                    )
                };
                if let Err(e) = result.and_then(|_| self.reload()) {
                    self.set_status(e, true);
                    return;
                }
                if let Some(i) = self.branches.iter().position(|b| b.name == new) {
                    self.cursor = i;
                }
                self.set_status(done, false);
            }
            _ => self.input.handle(&ev),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn setup(branches: &[&str]) -> (TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "init"]);
        for b in branches {
            git(dir.path(), &["branch", b]);
        }
        let app = App::new(Repo::new(dir.path()), Config::default()).unwrap();
        (dir, app)
    }

    fn press(app: &mut App, keys: &[&str]) {
        for k in keys {
            let ev = match *k {
                "esc" => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                "enter" => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                "ctrl+u" => KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                s => {
                    let c = s.chars().next().unwrap();
                    let mods = if c.is_uppercase() {
                        KeyModifiers::SHIFT
                    } else {
                        KeyModifiers::NONE
                    };
                    KeyEvent::new(KeyCode::Char(c), mods)
                }
            };
            app.handle_key(ev);
        }
    }

    fn names(app: &App) -> Vec<String> {
        app.repo
            .list_branches()
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect()
    }

    #[test]
    fn cursor_starts_on_current_and_moves() {
        let (_d, mut app) = setup(&["a", "b"]); // a, b, main
        assert_eq!(app.cursor, 2);
        press(&mut app, &["k", "k", "k"]);
        assert_eq!(app.cursor, 0);
        press(&mut app, &["j"]);
        assert_eq!(app.cursor, 1);
        press(&mut app, &["j", "j", "j"]);
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn delete_with_confirm() {
        let (_d, mut app) = setup(&["a", "b"]);
        press(&mut app, &["g", "d", "n"]);
        assert_eq!(names(&app), ["a", "b", "main"]);
        press(&mut app, &["d", "y"]);
        assert_eq!(names(&app), ["b", "main"]);
    }

    #[test]
    fn visual_delete_and_cancel() {
        let (_d, mut app) = setup(&["a", "b", "c"]);
        press(&mut app, &["g", "v", "j", "esc"]);
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, &["g", "v", "j", "j", "D", "y"]);
        assert_eq!(names(&app), ["main"]);
        assert_eq!((app.mode, app.cursor), (Mode::Normal, 0));
    }

    #[test]
    fn unmerged_needs_force() {
        let (d, mut app) = setup(&[]);
        git(d.path(), &["checkout", "-q", "-b", "feat"]);
        git(d.path(), &["commit", "-q", "--allow-empty", "-m", "x"]);
        git(d.path(), &["checkout", "-q", "main"]);
        press(&mut app, &["R", "g", "d", "y"]);
        assert!(app.status.as_ref().is_some_and(|s| s.error));
        assert_eq!(names(&app), ["feat", "main"]);
        press(&mut app, &["D", "y"]);
        assert_eq!(names(&app), ["main"]);
    }

    #[test]
    fn rename_and_checkout() {
        let (_d, mut app) = setup(&["a"]);
        press(&mut app, &["g", "r", "ctrl+u", "z", "enter"]);
        assert_eq!(names(&app), ["main", "z"]);
        assert_eq!(app.branches[app.cursor].name, "z");
        press(&mut app, &["enter"]);
        assert!(app.branches[app.cursor].current, "{:?}", app.status);
    }

    #[test]
    fn checkout_new_from_cursor_branch() {
        let (d, mut app) = setup(&[]);
        git(d.path(), &["checkout", "-q", "-b", "base"]);
        git(
            d.path(),
            &["commit", "-q", "--allow-empty", "-m", "on base"],
        );
        git(d.path(), &["checkout", "-q", "main"]);
        press(&mut app, &["R", "g", "b", "esc"]);
        assert_eq!((app.mode, names(&app).len()), (Mode::Normal, 2));
        press(&mut app, &["b", "f", "e", "a", "t", "enter"]);
        assert_eq!(names(&app), ["base", "feat", "main"]);
        let cur = &app.branches[app.cursor];
        assert!(cur.current && cur.name == "feat", "{:?}", app.status);
        assert_eq!(cur.subject, "on base");
    }

    #[test]
    fn renders() {
        use ratatui::{Terminal, backend::TestBackend};
        let (_d, mut app) = setup(&["feature/login", "fix-bug"]);
        press(&mut app, &["g", "v", "j", "d"]);
        let mut term = Terminal::new(TestBackend::new(60, 7)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let text: Vec<String> = (0..7)
            .map(|y| {
                (0..60)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        println!("{}", text.join("\n"));
        assert!(text[0].contains("VISUAL"));
        assert!(text[1].starts_with("  feature/login"));
        assert!(text[3].starts_with("* main"));
        assert_eq!(text[5], "Delete feature/login, fix-bug? [y/N]");
    }
}
