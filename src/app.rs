//! Application state and key handling.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::fuzzy;
use crate::git::{Branch, Op, Repo};
use crate::input::LineInput;
use crate::ops::{self, Operation, Outcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    Rename,
    /// Prompting for a new branch name to `checkout -b` from the cursor branch.
    Create,
    Confirm,
    /// Typing a fuzzy filter for the branch list.
    Search,
    /// Choosing what to do with the merge / rebase in progress.
    Operation,
}

/// Choices offered for a merge / rebase in progress, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Continue,
    Resolve,
    Abort,
}

impl Choice {
    pub const ALL: [Choice; 3] = [Choice::Continue, Choice::Resolve, Choice::Abort];

    pub fn label(self) -> &'static str {
        match self {
            Choice::Continue => "continue",
            Choice::Resolve => "resolve",
            Choice::Abort => "abort",
        }
    }

    fn step(self, delta: isize) -> Choice {
        let i = Choice::ALL.iter().position(|&c| c == self).unwrap_or(0) as isize;
        Choice::ALL[(i + delta).rem_euclid(Choice::ALL.len() as isize) as usize]
    }
}

/// An editor to run with the terminal released; the event loop takes and runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorRequest {
    pub program: String,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

pub struct App {
    repo: Repo,
    pub cfg: Config,
    /// Every branch, in git's order.
    all_branches: Vec<Branch>,
    /// Branches shown: those matching `filter`, best match first.
    pub branches: Vec<Branch>,
    /// Matched char positions in each shown branch name, for highlighting.
    pub matches: Vec<Vec<usize>>,
    /// Fuzzy filter; empty shows every branch.
    pub filter: LineInput,
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
    /// Branch yanked with `y`, merged / rebased with `p` / `P`.
    pub yanked: Option<String>,
    /// Merge / rebase in progress, refreshed on reload.
    pub operation: Option<Operation>,
    pub choice: Choice,
    pub editor_request: Option<EditorRequest>,
    pub status: Option<Status>,
    pub list_state: ListState,
    pub quit: bool,
    /// Printed after the terminal is restored, when quitting after a checkout.
    pub exit_message: Option<String>,
}

impl App {
    pub fn new(repo: Repo, cfg: Config) -> Result<Self, String> {
        let mut app = App {
            repo,
            cfg,
            all_branches: Vec::new(),
            branches: Vec::new(),
            matches: Vec::new(),
            filter: LineInput::default(),
            cursor: 0,
            anchor: 0,
            mode: Mode::Normal,
            prev_mode: Mode::Normal,
            input: LineInput::default(),
            input_target: String::new(),
            pending_delete: Vec::new(),
            pending_force: false,
            yanked: None,
            operation: None,
            choice: Choice::Resolve,
            editor_request: None,
            status: None,
            list_state: ListState::default(),
            quit: false,
            exit_message: None,
        };
        app.reload()?;
        if let Some(i) = app.branches.iter().position(|b| b.current) {
            app.cursor = i;
        }
        Ok(app)
    }

    fn reload(&mut self) -> Result<(), String> {
        self.all_branches = self.repo.list_branches(self.cfg.remotes)?;
        self.apply_filter();
        let last = self.branches.len().saturating_sub(1);
        self.cursor = self.cursor.min(last);
        self.anchor = self.anchor.min(last);
        self.operation = ops::in_progress(&self.repo)?;
        Ok(())
    }

    /// Recompute the shown branches from `filter`.
    fn apply_filter(&mut self) {
        let query = self.filter.value();
        let mut hits: Vec<(i32, usize, Vec<usize>)> = self
            .all_branches
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                fuzzy::fuzzy_match(&query, &b.name).map(|m| (m.score, i, m.positions))
            })
            .collect();
        // Stable, so equal scores keep git's order.
        hits.sort_by_key(|&(score, _, _)| std::cmp::Reverse(score));
        self.branches = hits
            .iter()
            .map(|(_, i, _)| self.all_branches[*i].clone())
            .collect();
        self.matches = hits.into_iter().map(|(_, _, p)| p).collect();
    }

    pub fn filtering(&self) -> bool {
        !self.filter.value().is_empty()
    }

    fn clear_filter(&mut self) {
        // Keep the cursor on the same branch in the full list.
        let name = self.branches.get(self.cursor).map(|b| b.name.clone());
        self.filter.set("");
        self.apply_filter();
        if let Some(i) = name.and_then(|n| self.branches.iter().position(|b| b.name == n)) {
            self.cursor = i;
        }
    }

    /// Report a successful checkout, quitting if configured to.
    fn checked_out(&mut self, text: String) {
        if self.cfg.quit_on_checkout {
            self.quit = true;
            self.exit_message = Some(text.clone());
        }
        self.set_status(text, false);
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

    fn selected(&self) -> &[Branch] {
        if self.branches.is_empty() {
            return &[];
        }
        let (lo, hi) = self.selection();
        &self.branches[lo..=hi]
    }

    fn is_remote(&self, name: &str) -> bool {
        self.all_branches
            .iter()
            .any(|b| b.name == name && b.remote.is_some())
    }

    pub fn handle_key(&mut self, ev: KeyEvent) {
        match self.mode {
            Mode::Rename | Mode::Create => self.handle_input(ev),
            Mode::Confirm => self.handle_confirm(ev),
            Mode::Search => self.handle_search(ev),
            Mode::Operation => self.handle_operation(ev),
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
            if self.mode == Mode::Normal && self.filtering() {
                self.clear_filter();
            }
            self.mode = Mode::Normal;
        } else if k.search.matches(&ev) {
            self.mode = Mode::Search;
        } else if k.fetch.matches(&ev) {
            match self.repo.fetch().and_then(|_| self.reload()) {
                Ok(()) => self.set_status("fetched", false),
                Err(e) => self.set_status(e, true),
            }
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
        } else if k.yank.matches(&ev) {
            self.yank();
        } else if k.merge.matches(&ev) {
            self.integrate(Op::Merge);
        } else if k.rebase.matches(&ev) {
            self.integrate(Op::Rebase);
        } else if k.operation.matches(&ev) {
            if self.operation.is_some() {
                self.open_operation();
            } else {
                self.set_status("no merge or rebase in progress", false);
            }
        }
    }

    fn yank(&mut self) {
        let Some(name) = self.branches.get(self.cursor).map(|b| b.name.clone()) else {
            return;
        };
        self.mode = Mode::Normal;
        self.set_status(format!("yanked {name}"), false);
        self.yanked = Some(name);
    }

    /// Merge the yanked branch into, or rebase it onto, the cursor branch.
    fn integrate(&mut self, op: Op) {
        let Some(target) = self.branches.get(self.cursor).map(|b| b.name.clone()) else {
            return;
        };
        self.mode = Mode::Normal;
        if self.operation.is_some() {
            // Finish or abort the one in progress first.
            self.open_operation();
            return;
        }
        let Some(branch) = self.yanked.clone() else {
            self.set_status("nothing yanked", true);
            return;
        };
        if branch == target {
            self.set_status(format!("{branch} is the yanked branch itself"), true);
            return;
        }
        match op {
            Op::Merge if self.is_remote(&target) => {
                self.set_status(format!("can't merge into remote branch {target}"), true);
                return;
            }
            Op::Rebase if self.is_remote(&branch) => {
                self.set_status(format!("can't rebase remote branch {branch}"), true);
                return;
            }
            _ => {}
        }
        let result = ops::start(&self.repo, op, &branch, &target, self.cfg.autostash);
        self.after_step(result);
    }

    /// Show the outcome of a merge / rebase step, prompting again if it stopped.
    fn after_step(&mut self, result: Result<Outcome, String>) {
        let reloaded = self.reload();
        match result {
            Ok(Outcome::Done(msg)) => self.set_status(msg, false),
            Ok(Outcome::Stopped) => self.open_operation(),
            Err(e) => self.set_status(e, true),
        }
        if let Err(e) = reloaded {
            self.set_status(e, true);
        }
    }

    fn open_operation(&mut self) {
        let Some(operation) = &self.operation else {
            return;
        };
        self.choice = if operation.conflicts.is_empty() {
            Choice::Continue
        } else {
            Choice::Resolve
        };
        self.mode = Mode::Operation;
    }

    fn handle_operation(&mut self, ev: KeyEvent) {
        let k = &self.cfg.keys;
        let choice = match ev.code {
            KeyCode::Char('c') if ev.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
                return;
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return;
            }
            KeyCode::Char('c') => Choice::Continue,
            KeyCode::Char('r') => Choice::Resolve,
            KeyCode::Char('a') => Choice::Abort,
            KeyCode::Enter => self.choice,
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => {
                self.choice = self.choice.step(-1);
                return;
            }
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
                self.choice = self.choice.step(1);
                return;
            }
            _ if k.up.matches(&ev) => {
                self.choice = self.choice.step(-1);
                return;
            }
            _ if k.down.matches(&ev) => {
                self.choice = self.choice.step(1);
                return;
            }
            _ => return,
        };
        self.mode = Mode::Normal;
        match choice {
            Choice::Continue => {
                let result = ops::resume(&self.repo);
                self.after_step(result);
            }
            Choice::Resolve => self.resolve(),
            Choice::Abort => {
                let result = ops::abort(&self.repo).map(Outcome::Done);
                self.after_step(result);
            }
        }
    }

    fn resolve(&mut self) {
        let files = match &self.operation {
            Some(o) if !o.conflicts.is_empty() => o.conflicts.clone(),
            _ => {
                self.set_status("no conflicts left: continue to finish", false);
                return;
            }
        };
        let mut words = self.cfg.editor.split_whitespace().map(String::from);
        let Some(program) = words.next() else {
            self.set_status("no editor configured", true);
            return;
        };
        let dir = match self.repo.toplevel() {
            Ok(dir) => dir,
            Err(e) => {
                self.set_status(e, true);
                return;
            }
        };
        let mut args: Vec<String> = words.collect();
        args.extend(files);
        self.editor_request = Some(EditorRequest { program, args, dir });
        let key = self
            .cfg
            .keys
            .operation
            .primary()
            .map(|k| k.to_string())
            .unwrap_or_default();
        self.set_status(
            format!("resolve the conflicts, then press {key} and continue"),
            false,
        );
    }

    /// Called by the event loop once the requested editor has been run.
    pub fn editor_done(&mut self, result: std::io::Result<std::process::ExitStatus>) {
        let err = match result {
            Ok(status) if status.success() => None,
            Ok(status) => Some(format!("editor exited with {status}")),
            Err(e) => Some(format!("failed to run editor {:?}: {e}", self.cfg.editor)),
        };
        if let Some(e) = err {
            self.set_status(e, true);
        }
        if let Err(e) = self.reload() {
            self.set_status(e, true);
        }
    }

    fn handle_search(&mut self, ev: KeyEvent) {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        let last = self.branches.len().saturating_sub(1);
        match ev.code {
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Esc => {
                self.clear_filter();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => self.mode = Mode::Normal,
            // Backspace on an empty query leaves search, like vim's `/`.
            KeyCode::Backspace if !self.filtering() => self.mode = Mode::Normal,
            KeyCode::Up | KeyCode::BackTab => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('p' | 'k') if ctrl => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('n' | 'j') if ctrl => self.cursor = (self.cursor + 1).min(last),
            _ => {
                let before = self.filter.value();
                self.filter.handle(&ev);
                if self.filter.value() != before {
                    self.apply_filter();
                    self.cursor = 0;
                }
            }
        }
    }

    fn start_input(&mut self, mode: Mode) {
        let Some(b) = self.branches.get(self.cursor) else {
            return;
        };
        let name = b.name.clone();
        if mode == Mode::Rename && b.remote.is_some() {
            self.set_status(format!("can't rename remote branch {name}"), true);
            return;
        }
        self.input
            .set(if mode == Mode::Rename { &name } else { "" });
        self.input_target = name;
        self.prev_mode = self.mode;
        self.mode = mode;
    }

    fn checkout(&mut self) {
        let Some(b) = self.branches.get(self.cursor) else {
            return;
        };
        self.mode = Mode::Normal;
        let (name, local) = (b.name.clone(), b.local_name().to_string());
        let (result, done) = if b.remote.is_none() {
            (self.repo.checkout(&name), format!("switched to {name}"))
        } else if self
            .all_branches
            .iter()
            .any(|b| b.remote.is_none() && b.name == local)
        {
            // Like `git switch`: a local branch of that name wins.
            (self.repo.checkout(&local), format!("switched to {local}"))
        } else {
            (
                self.repo.checkout_track(&local, &name),
                format!("switched to new branch {local} tracking {name}"),
            )
        };
        match result.and_then(|_| self.reload()) {
            Ok(()) => {
                if let Some(i) = self.branches.iter().position(|b| b.current) {
                    self.cursor = i;
                }
                self.checked_out(done);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn start_delete(&mut self, force: bool) {
        if let Some(b) = self.selected().iter().find(|b| b.remote.is_some()) {
            self.set_status(format!("can't delete remote branch {}", b.name), true);
            return;
        }
        let names: Vec<String> = self.selected().iter().map(|b| b.name.clone()).collect();
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
                if mode == Mode::Rename {
                    self.set_status(done, false);
                } else {
                    self.checked_out(done);
                }
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
        // The app's own merges / rebases commit too.
        git(dir.path(), &["config", "user.name", "t"]);
        git(dir.path(), &["config", "user.email", "t@t"]);
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
                "backspace" => KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
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

    /// Commit `content` to `file` on `branch`, then go back to main.
    fn commit_file(dir: &std::path::Path, branch: &str, file: &str, content: &str) {
        git(dir, &["checkout", "-q", branch]);
        std::fs::write(dir.join(file), content).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-q", "-m", &format!("{file} on {branch}")]);
        git(dir, &["checkout", "-q", "main"]);
    }

    fn rev(dir: &std::path::Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn cursor_to(app: &mut App, name: &str) {
        app.cursor = app.branches.iter().position(|b| b.name == name).unwrap();
    }

    fn names(app: &App) -> Vec<String> {
        app.repo
            .list_branches(false)
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

    fn shown(app: &App) -> Vec<&str> {
        app.branches.iter().map(|b| b.name.as_str()).collect()
    }

    #[test]
    fn search_filters_and_ranks() {
        let (_d, mut app) = setup(&["feature/login", "fix-login-bug", "flag"]);
        press(&mut app, &["/", "f", "l"]);
        assert_eq!(app.mode, Mode::Search);
        // Runs rank first, then shorter gaps; "main" doesn't match.
        assert_eq!(shown(&app), ["flag", "fix-login-bug", "feature/login"]);
        assert_eq!(app.matches[2], [0, 8]);
        press(&mut app, &["o"]);
        assert_eq!(shown(&app), ["fix-login-bug", "feature/login"]);
        assert_eq!(app.cursor, 0);
        // Enter keeps the filter; j / k navigate it and actions use it.
        press(&mut app, &["enter", "j"]);
        assert_eq!((app.mode, app.cursor), (Mode::Normal, 1));
        press(&mut app, &["y"]);
        assert_eq!(app.yanked.as_deref(), Some("feature/login"));
        // Esc clears it, keeping the cursor on the same branch.
        press(&mut app, &["esc"]);
        assert!(!app.filtering());
        assert_eq!(shown(&app).len(), 4);
        assert_eq!(app.branches[app.cursor].name, "feature/login");
    }

    #[test]
    fn search_then_checkout() {
        let (_d, mut app) = setup(&["a", "zeta"]);
        press(&mut app, &["/", "z", "t", "enter", "enter"]);
        assert!(app.quit, "{:?}", app.status);
        assert_eq!(app.exit_message.as_deref(), Some("switched to zeta"));
    }

    #[test]
    fn search_esc_and_no_matches() {
        let (_d, mut app) = setup(&["a"]);
        press(&mut app, &["/", "x", "q", "d"]);
        // Typed keys don't trigger actions, and nothing matches.
        assert!(!app.quit && app.branches.is_empty());
        press(&mut app, &["enter", "d", "enter"]);
        assert_eq!((app.mode, names(&app).len()), (Mode::Normal, 2));
        press(&mut app, &["/", "esc"]);
        assert_eq!((app.mode, shown(&app)), (Mode::Normal, vec!["a", "main"]));
    }

    #[test]
    fn search_backspace_on_empty_query_exits() {
        let (_d, mut app) = setup(&["a", "zeta"]);
        press(&mut app, &["/", "backspace"]);
        assert_eq!(app.mode, Mode::Normal);
        // Deleting the last char stays in search, showing everything.
        press(&mut app, &["/", "z", "backspace"]);
        assert_eq!((app.mode, shown(&app).len()), (Mode::Search, 3));
        press(&mut app, &["backspace"]);
        assert_eq!(app.mode, Mode::Normal);
    }

    /// A clone of a repo with branches main and `branches`; only main is local.
    fn setup_clone(branches: &[&str]) -> (TempDir, TempDir, App) {
        let (up, _) = setup(branches);
        let dir = tempfile::tempdir().unwrap();
        git(
            dir.path(),
            &["clone", "-q", up.path().to_str().unwrap(), "."],
        );
        let app = App::new(Repo::new(dir.path()), Config::default()).unwrap();
        (up, dir, app)
    }

    #[test]
    fn lists_remote_branches() {
        let (_u, _d, mut app) = setup_clone(&["feat"]);
        // origin/HEAD is skipped.
        assert_eq!(shown(&app), ["main", "origin/feat", "origin/main"]);
        assert_eq!(app.branches[1].remote.as_deref(), Some("origin"));
        assert_eq!(app.branches[1].local_name(), "feat");
        app.cfg.remotes = false;
        press(&mut app, &["R"]);
        assert_eq!(shown(&app), ["main"]);
    }

    #[test]
    fn checkout_remote_branch_tracks_it() {
        let (_u, d, mut app) = setup_clone(&["feat"]);
        cursor_to(&mut app, "origin/feat");
        press(&mut app, &["enter"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert_eq!(
            app.exit_message.as_deref(),
            Some("switched to new branch feat tracking origin/feat")
        );
        assert_eq!(head(d.path()), "feat");
        assert_eq!(
            rev(d.path(), &["rev-parse", "--abbrev-ref", "feat@{upstream}"]),
            "origin/feat"
        );
        assert_eq!(app.branches[app.cursor].name, "feat");

        // A local branch of the same name is checked out instead.
        app.cfg.quit_on_checkout = false;
        cursor_to(&mut app, "origin/main");
        press(&mut app, &["enter"]);
        assert_eq!(app.status.as_ref().unwrap().text, "switched to main");
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn fetch_lists_new_and_pruned_remote_branches() {
        let (u, _d, mut app) = setup_clone(&["feat"]);
        git(u.path(), &["branch", "new"]);
        git(u.path(), &["branch", "-D", "feat"]);
        press(&mut app, &["f"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert_eq!(shown(&app), ["main", "origin/main", "origin/new"]);
    }

    #[test]
    fn remote_branches_refuse_local_only_actions() {
        let (_u, _d, mut app) = setup_clone(&["feat"]);
        cursor_to(&mut app, "origin/feat");
        press(&mut app, &["d"]);
        assert!(is_error(&app) && app.mode == Mode::Normal);
        press(&mut app, &["g", "v", "j", "D"]);
        assert!(is_error(&app) && app.mode == Mode::Visual);
        press(&mut app, &["esc", "r"]);
        assert!(is_error(&app) && app.mode == Mode::Normal);
        // Yanked remote branches can be merged, but not rebased; nothing merges into one.
        press(&mut app, &["y", "k", "P"]);
        assert!(is_error(&app), "{:?}", app.status);
        press(&mut app, &["p"]);
        assert!(!is_error(&app), "{:?}", app.status);
        press(&mut app, &["y", "G", "p"]);
        assert!(is_error(&app), "{:?}", app.status);
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
        assert!(!app.quit);
        press(&mut app, &["enter"]);
        assert!(app.branches[app.cursor].current, "{:?}", app.status);
        assert!(app.quit);
        assert_eq!(app.exit_message.as_deref(), Some("switched to z"));
    }

    #[test]
    fn checkout_stays_open_when_configured() {
        let (_d, mut app) = setup(&["a"]);
        app.cfg.quit_on_checkout = false;
        press(&mut app, &["g", "enter"]);
        assert!(app.branches[app.cursor].current, "{:?}", app.status);
        assert!(!app.quit);
        assert_eq!(app.exit_message, None);
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
        assert!(app.quit);
    }

    fn head(dir: &std::path::Path) -> String {
        rev(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    /// Reload, yank `branch` and put the cursor on `target`.
    fn yank_to(app: &mut App, branch: &str, target: &str) {
        press(app, &["R"]);
        cursor_to(app, branch);
        press(app, &["y"]);
        cursor_to(app, target);
    }

    fn is_error(app: &App) -> bool {
        app.status.as_ref().is_some_and(|s| s.error)
    }

    #[test]
    fn merge_and_rebase_need_a_yank() {
        let (_d, mut app) = setup(&["a"]);
        press(&mut app, &["p"]);
        assert!(is_error(&app));
        press(&mut app, &["g", "y", "P"]);
        assert_eq!(app.yanked.as_deref(), Some("a"));
        assert!(is_error(&app), "self rebase");
    }

    #[test]
    fn merge_into_checked_out_branch() {
        let (d, mut app) = setup(&["feat"]);
        commit_file(d.path(), "feat", "f", "feat");
        commit_file(d.path(), "main", "m", "main");
        yank_to(&mut app, "feat", "main");
        press(&mut app, &["p"]);
        assert_eq!(app.mode, Mode::Normal, "{:?}", app.status);
        let feat = rev(d.path(), &["rev-parse", "feat"]);
        assert_eq!(rev(d.path(), &["merge-base", "feat", "main"]), feat);
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn merge_into_other_branch_keeps_head_and_worktree() {
        let (d, mut app) = setup(&["dev", "feat"]);
        commit_file(d.path(), "feat", "f", "feat");
        commit_file(d.path(), "dev", "g", "dev");
        commit_file(d.path(), "main", "m", "main");
        // Uncommitted changes don't matter when the working tree isn't needed.
        std::fs::write(d.path().join("m"), "dirty").unwrap();
        yank_to(&mut app, "feat", "dev");
        press(&mut app, &["p"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert_eq!(
            rev(d.path(), &["rev-parse", "dev^2"]),
            rev(d.path(), &["rev-parse", "feat"])
        );
        assert_eq!(head(d.path()), "main");
        assert!(!d.path().join("f").exists());
        assert_eq!(
            std::fs::read_to_string(d.path().join("m")).unwrap(),
            "dirty"
        );
    }

    #[test]
    fn fast_forward_other_branch() {
        let (d, mut app) = setup(&["dev", "feat"]);
        commit_file(d.path(), "feat", "f", "feat");
        yank_to(&mut app, "feat", "dev");
        press(&mut app, &["p"]);
        assert!(app.status.as_ref().unwrap().text.contains("fast-forwarded"));
        assert_eq!(
            rev(d.path(), &["rev-parse", "dev"]),
            rev(d.path(), &["rev-parse", "feat"])
        );
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn rebase_returns_to_original_branch() {
        let (d, mut app) = setup(&["feat"]);
        commit_file(d.path(), "feat", "f", "feat");
        commit_file(d.path(), "main", "m", "main");
        yank_to(&mut app, "feat", "main");
        press(&mut app, &["P"]);
        assert_eq!(app.mode, Mode::Normal, "{:?}", app.status);
        let main = rev(d.path(), &["rev-parse", "main"]);
        assert_eq!(rev(d.path(), &["merge-base", "feat", "main"]), main);
        assert_eq!(rev(d.path(), &["rev-list", "--count", "main..feat"]), "1");
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn dirty_tree_refused_unless_autostash() {
        let (d, mut app) = setup(&["feat"]);
        commit_file(d.path(), "feat", "f", "feat");
        commit_file(d.path(), "main", "m", "main");
        std::fs::write(d.path().join("m"), "dirty").unwrap();
        let before = rev(d.path(), &["rev-parse", "feat"]);
        yank_to(&mut app, "feat", "main");
        press(&mut app, &["P"]);
        assert!(is_error(&app));
        assert!(app.status.as_ref().unwrap().text.contains("uncommitted"));
        assert_eq!(rev(d.path(), &["rev-parse", "feat"]), before);

        app.cfg.autostash = true;
        press(&mut app, &["P"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert_ne!(rev(d.path(), &["rev-parse", "feat"]), before);
        assert_eq!(head(d.path()), "main");
        assert_eq!(
            std::fs::read_to_string(d.path().join("m")).unwrap(),
            "dirty"
        );
        assert!(rev(d.path(), &["stash", "list"]).is_empty());
    }

    /// feat and dev both add `x`; main has uncommitted changes to `m`; autostash on.
    fn conflicting() -> (TempDir, App) {
        let (d, mut app) = setup(&["dev", "feat"]);
        commit_file(d.path(), "feat", "x", "feat");
        commit_file(d.path(), "dev", "x", "dev");
        commit_file(d.path(), "main", "m", "main");
        std::fs::write(d.path().join("m"), "dirty").unwrap();
        app.cfg.autostash = true;
        yank_to(&mut app, "feat", "dev");
        (d, app)
    }

    #[test]
    fn merge_conflict_abort_restores_everything() {
        let (d, mut app) = conflicting();
        press(&mut app, &["p"]);
        assert_eq!(app.mode, Mode::Operation, "{:?}", app.status);
        let op = app.operation.as_ref().unwrap();
        assert_eq!(
            (op.op, op.conflicts.as_slice()),
            (Op::Merge, ["x".to_string()].as_slice())
        );
        assert_eq!(head(d.path()), "dev");
        assert_eq!(app.choice, Choice::Resolve);
        // Move to "abort" and select it.
        press(&mut app, &["l", "enter"]);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.operation.is_none() && app.editor_request.is_none());
        assert!(!d.path().join(".git/MERGE_HEAD").exists());
        assert!(!d.path().join(".git/gim-pending").exists());
        assert_eq!(head(d.path()), "main");
        assert_eq!(
            std::fs::read_to_string(d.path().join("m")).unwrap(),
            "dirty"
        );
    }

    #[test]
    fn merge_conflict_resolve_then_continue() {
        let (d, mut app) = conflicting();
        app.cfg.editor = "myeditor --wait".into();
        press(&mut app, &["p", "enter"]);
        assert_eq!(app.mode, Mode::Normal);
        let req = app.editor_request.take().unwrap();
        assert_eq!(req.program, "myeditor");
        assert_eq!(req.args, ["--wait", "x"]);
        assert_eq!(
            req.dir.canonicalize().unwrap(),
            d.path().canonicalize().unwrap()
        );
        // Still in progress; continuing is refused while markers remain.
        assert!(app.operation.is_some());
        press(&mut app, &["o", "c"]);
        assert!(is_error(&app));
        assert!(
            app.status
                .as_ref()
                .unwrap()
                .text
                .contains("conflict markers")
        );

        std::fs::write(d.path().join("x"), "resolved").unwrap();
        press(&mut app, &["o", "c"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert!(app.operation.is_none());
        assert_eq!(
            rev(d.path(), &["rev-parse", "dev^2"]),
            rev(d.path(), &["rev-parse", "feat"])
        );
        assert_eq!(rev(d.path(), &["show", "dev:x"]), "resolved");
        assert_eq!(head(d.path()), "main");
        assert_eq!(
            std::fs::read_to_string(d.path().join("m")).unwrap(),
            "dirty"
        );
    }

    #[test]
    fn rebase_stopping_twice() {
        let (d, mut app) = setup(&["feat"]);
        commit_file(d.path(), "feat", "x", "feat x");
        commit_file(d.path(), "feat", "y", "feat y");
        commit_file(d.path(), "main", "x", "main x");
        commit_file(d.path(), "main", "y", "main y");
        yank_to(&mut app, "feat", "main");
        press(&mut app, &["P"]);
        assert_eq!(app.mode, Mode::Operation, "{:?}", app.status);
        assert_eq!(app.operation.as_ref().unwrap().conflicts, ["x"]);

        std::fs::write(d.path().join("x"), "x resolved").unwrap();
        press(&mut app, &["c"]);
        // Stopped again on the second commit: prompted again.
        assert_eq!(app.mode, Mode::Operation, "{:?}", app.status);
        assert_eq!(app.operation.as_ref().unwrap().conflicts, ["y"]);

        std::fs::write(d.path().join("y"), "y resolved").unwrap();
        press(&mut app, &["c"]);
        assert_eq!(app.mode, Mode::Normal);
        assert!(!is_error(&app), "{:?}", app.status);
        assert!(app.operation.is_none());
        let main = rev(d.path(), &["rev-parse", "main"]);
        assert_eq!(rev(d.path(), &["merge-base", "feat", "main"]), main);
        assert_eq!(rev(d.path(), &["rev-list", "--count", "main..feat"]), "2");
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn detects_operation_started_elsewhere() {
        let (d, _) = setup(&["feat"]);
        commit_file(d.path(), "feat", "x", "feat");
        commit_file(d.path(), "main", "x", "main");
        let merged = Command::new("git")
            .current_dir(d.path())
            .args(["merge", "feat"])
            .output()
            .unwrap();
        assert!(!merged.status.success());
        let mut app = App::new(Repo::new(d.path()), Config::default()).unwrap();
        let op = app.operation.as_ref().unwrap();
        assert_eq!((op.op, op.desc.as_str()), (Op::Merge, "merge in progress"));
        // A new rebase reopens the prompt for the merge instead of starting.
        press(&mut app, &["P"]);
        assert_eq!(app.mode, Mode::Operation);
        press(&mut app, &["esc"]);
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, &["o", "a"]);
        assert!(!is_error(&app), "{:?}", app.status);
        assert_eq!(app.status.as_ref().unwrap().text, "merge aborted");
        assert!(app.operation.is_none());
        assert_eq!(head(d.path()), "main");
    }

    #[test]
    fn renders_operation() {
        use ratatui::{Terminal, backend::TestBackend};
        let (_d, mut app) = conflicting();
        press(&mut app, &["p"]);
        let mut term = Terminal::new(TestBackend::new(80, 6)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let row = |y| (0..80).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(
            row(0).contains(" MERGING  merging feat into dev (1 conflicted file)"),
            "{}",
            row(0)
        );
        assert!(row(0).contains("yanked: feat"), "{}", row(0));
        assert_eq!(
            row(4).trim_end(),
            "merging feat into dev, 1 conflicted file:  continue   resolve   abort"
        );
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

    #[test]
    fn renders_search() {
        use ratatui::layout::Position;
        use ratatui::style::Modifier;
        use ratatui::{Terminal, backend::TestBackend};
        let (_d, mut app) = setup(&["feature/login", "fix-bug"]);
        press(&mut app, &["/", "f", "b"]);
        let mut term = Terminal::new(TestBackend::new(60, 5)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let row = |y| (0..60).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(row(1).starts_with("  fix-bug "), "{}", row(1));
        assert_eq!(row(2).trim_end(), "");
        assert_eq!(row(3).trim_end(), "/fb");
        assert_eq!(term.backend().cursor_position(), Position::new(3, 3));
        // Matched chars are styled.
        assert!(buf[(6, 1)].modifier.contains(Modifier::UNDERLINED));
        assert!(!buf[(3, 1)].modifier.contains(Modifier::UNDERLINED));

        press(&mut app, &["enter"]);
        term.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let row = |y| (0..60).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(row(0).contains("  /fb"), "{}", row(0));
    }
}
