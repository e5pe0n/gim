//! TOML configuration: key bindings and options.

use std::{
    env, fmt, fs, io,
    path::{Component, Path, PathBuf},
};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

/// One key, e.g. `k`, `D`, `up`, `esc`, `ctrl+c`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpec {
    code: KeyCode,
    mods: KeyModifiers,
}

impl KeySpec {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut mods = KeyModifiers::NONE;
        let mut rest = s;
        // A trailing "+" is the key itself (e.g. "ctrl++"), so only split on "+" followed by more.
        while let Some((m, r)) = rest.split_once('+').filter(|(_, r)| !r.is_empty()) {
            mods |= match m.to_ascii_lowercase().as_str() {
                "ctrl" | "c" => KeyModifiers::CONTROL,
                "alt" | "a" | "m" => KeyModifiers::ALT,
                "shift" | "s" => KeyModifiers::SHIFT,
                _ => return Err(format!("unknown modifier {m:?} in key {s:?}")),
            };
            rest = r;
        }
        let code = match rest.to_ascii_lowercase().as_str() {
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "space" => KeyCode::Char(' '),
            _ => {
                let mut chars = rest.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => KeyCode::Char(c),
                    _ => return Err(format!("unknown key {s:?}")),
                }
            }
        };
        Ok(KeySpec { code, mods })
    }

    pub fn matches(&self, ev: &KeyEvent) -> bool {
        match (self.code, ev.code) {
            // Shift is already reflected in the character's case, so ignore it for chars.
            (KeyCode::Char(a), KeyCode::Char(b)) => {
                let strip = |m: KeyModifiers| m - KeyModifiers::SHIFT;
                a == b && strip(self.mods) == strip(ev.modifiers)
            }
            (a, b) => a == b && self.mods == ev.modifiers,
        }
    }
}

impl fmt::Display for KeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.contains(KeyModifiers::CONTROL) {
            f.write_str("ctrl+")?;
        }
        if self.mods.contains(KeyModifiers::ALT) {
            f.write_str("alt+")?;
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            f.write_str("shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Esc => f.write_str("esc"),
            KeyCode::Enter => f.write_str("enter"),
            other => write!(f, "{}", other.to_string().to_lowercase()),
        }
    }
}

/// Bindings for one action. In TOML: a key name or an array of key names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys(pub Vec<KeySpec>);

impl Keys {
    fn of(names: &[&str]) -> Self {
        Keys(
            names
                .iter()
                .map(|n| KeySpec::parse(n).expect("valid default key"))
                .collect(),
        )
    }

    pub fn matches(&self, ev: &KeyEvent) -> bool {
        self.0.iter().any(|k| k.matches(ev))
    }

    /// First binding, for help text.
    pub fn primary(&self) -> Option<&KeySpec> {
        self.0.first()
    }
}

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Many(Vec<String>),
        }
        let names = match Raw::deserialize(d)? {
            Raw::One(s) => vec![s],
            Raw::Many(v) => v,
        };
        names
            .iter()
            .map(|n| KeySpec::parse(n))
            .collect::<Result<_, _>>()
            .map(Keys)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyMap {
    pub up: Keys,
    pub down: Keys,
    pub top: Keys,
    pub bottom: Keys,
    pub delete: Keys,
    pub force_delete: Keys,
    pub visual: Keys,
    pub cancel: Keys,
    pub rename: Keys,
    pub checkout: Keys,
    pub checkout_new: Keys,
    pub yank: Keys,
    pub merge: Keys,
    pub rebase: Keys,
    /// Open the continue / resolve / abort prompt for the merge / rebase in progress.
    pub operation: Keys,
    /// Fuzzy-filter the branch list.
    pub search: Keys,
    /// `git fetch --all --prune`, then reload.
    pub fetch: Keys,
    pub reload: Keys,
    pub quit: Keys,
}

impl Default for KeyMap {
    fn default() -> Self {
        KeyMap {
            up: Keys::of(&["k", "up"]),
            down: Keys::of(&["j", "down"]),
            top: Keys::of(&["g", "home"]),
            bottom: Keys::of(&["G", "end"]),
            delete: Keys::of(&["d"]),
            force_delete: Keys::of(&["D"]),
            visual: Keys::of(&["v"]),
            cancel: Keys::of(&["esc"]),
            rename: Keys::of(&["r"]),
            checkout: Keys::of(&["enter"]),
            checkout_new: Keys::of(&["b"]),
            yank: Keys::of(&["y"]),
            merge: Keys::of(&["p"]),
            rebase: Keys::of(&["P"]),
            operation: Keys::of(&["o"]),
            search: Keys::of(&["/"]),
            fetch: Keys::of(&["f"]),
            reload: Keys::of(&["R"]),
            quit: Keys::of(&["q", "ctrl+c"]),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Ask for confirmation before deleting branches.
    pub confirm_delete: bool,
    /// Command opened on the conflicted files to resolve a merge / rebase conflict,
    /// split on whitespace (e.g. `"code"`, `"nvim -d"`).
    pub editor: String,
    /// Stash uncommitted changes around a merge / rebase that needs the working tree,
    /// instead of refusing to start.
    pub autostash: bool,
    /// Quit after checking out a branch (`checkout` / `checkout_new`).
    pub quit_on_checkout: bool,
    /// List remote-tracking branches after the local ones.
    pub remotes: bool,
    pub worktree: WorktreeConfig,
    pub keys: KeyMap,
}

/// Checking out branches in worktrees of their own.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorktreeConfig {
    /// Check out (`checkout` / `checkout_new`) a branch in its own worktree, creating it if
    /// needed, and move there, instead of switching the current worktree.
    pub enabled: bool,
    /// Where a branch's worktree is created. `{repo}` is the main worktree's directory name,
    /// `{repo_path}` its path relative to the home directory (or its absolute path without the
    /// leading `/` when outside it) and `{branch}` the branch name. A relative path is relative to
    /// the main worktree, and a leading `~/` is the home directory.
    pub path: String,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        WorktreeConfig {
            enabled: false,
            path: "~/.local/share/gim/worktrees/{repo_path}/{branch}".into(),
        }
    }
}

impl WorktreeConfig {
    /// The worktree path for `branch` of the repository whose main worktree is `main`.
    pub fn path_for(&self, main: &Path, branch: &str) -> PathBuf {
        self.path_with_home(main, branch, env::home_dir().as_deref())
    }

    fn path_with_home(&self, main: &Path, branch: &str, home: Option<&Path>) -> PathBuf {
        let repo = main.file_name().unwrap_or_default().to_string_lossy();
        // Unique per repository, so same-named repositories don't share worktree paths.
        let repo_path: PathBuf = home
            .and_then(|h| main.strip_prefix(h).ok())
            .unwrap_or(main)
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect();
        let path = self
            .path
            .replace("{repo_path}", &repo_path.to_string_lossy())
            .replace("{repo}", &repo)
            .replace("{branch}", branch);
        let path = match (path.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => main.join(path),
        };
        // Resolve `..` lexically so the path reads cleanly (e.g. in messages).
        let mut out = PathBuf::new();
        for c in path.components() {
            match c {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                c => out.push(c),
            }
        }
        out
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            confirm_delete: true,
            editor: "code".into(),
            autostash: false,
            quit_on_checkout: true,
            remotes: true,
            worktree: WorktreeConfig::default(),
            keys: KeyMap::default(),
        }
    }
}

/// `$GIM_CONFIG`, else `$XDG_CONFIG_HOME/gim/config.toml`, else `~/.config/gim/config.toml`.
pub fn default_path() -> Option<PathBuf> {
    if let Some(p) = env::var_os("GIM_CONFIG") {
        return Some(p.into());
    }
    let dir = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|h| h.join(".config")))?;
    Some(dir.join("gim").join("config.toml"))
}

pub fn parse(src: &str) -> Result<Config, String> {
    toml::from_str(src).map_err(|e| e.to_string())
}

/// Load the config at `path`. A missing file yields the defaults.
pub fn load(path: &PathBuf) -> Result<Config, String> {
    match fs::read_to_string(path) {
        Ok(src) => parse(&src).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn empty_config_uses_defaults() {
        let cfg = parse("").unwrap();
        assert!(cfg.confirm_delete);
        assert!(cfg.quit_on_checkout);
        assert_eq!(cfg.editor, "code");
        assert_eq!(cfg.keys.down, Keys::of(&["j", "down"]));
    }

    #[test]
    fn overrides_keep_other_defaults() {
        let cfg = parse(
            r#"
confirm_delete = false
editor = "vim"
[keys]
down = "n"
up = ["e", "up"]
"#,
        )
        .unwrap();
        assert!(!cfg.confirm_delete);
        assert_eq!(cfg.editor, "vim");
        assert_eq!(cfg.keys.down, Keys::of(&["n"]));
        assert_eq!(cfg.keys.up, Keys::of(&["e", "up"]));
        assert_eq!(cfg.keys.delete, Keys::of(&["d"]));
    }

    #[test]
    fn worktree_path() {
        let cfg = parse("[worktree]\nenabled = true").unwrap();
        assert!(cfg.worktree.enabled);
        let home = Some(Path::new("/home/me"));
        assert_eq!(
            cfg.worktree
                .path_with_home(Path::new("/home/me/src/gim"), "feat/x", home),
            Path::new("/home/me/.local/share/gim/worktrees/src/gim/feat/x")
        );
        // Outside the home directory, the whole path is used.
        assert_eq!(
            cfg.worktree
                .path_with_home(Path::new("/srv/gim"), "a", home),
            Path::new("/home/me/.local/share/gim/worktrees/srv/gim/a")
        );
        let main = Path::new("/src/gim");
        let cfg = parse("[worktree]\npath = \"/wt/{repo}-{branch}\"").unwrap();
        assert!(!cfg.worktree.enabled);
        assert_eq!(cfg.worktree.path_for(main, "a"), Path::new("/wt/gim-a"));
        let cfg = parse("[worktree]\npath = \".worktrees/{branch}\"").unwrap();
        assert_eq!(
            cfg.worktree.path_for(main, "a"),
            Path::new("/src/gim/.worktrees/a")
        );
        if let Some(home) = env::home_dir() {
            let cfg = parse("[worktree]\npath = \"~/wt/{branch}\"").unwrap();
            assert_eq!(cfg.worktree.path_for(main, "a"), home.join("wt/a"));
        }
        assert!(parse("[worktree]\ndir = \"x\"").is_err());
    }

    #[test]
    fn rejects_unknown_action_and_key() {
        assert!(parse("[keys]\njump = \"x\"").is_err());
        assert!(parse("[keys]\nup = \"hyper+x\"").is_err());
        assert!(parse("[keys]\nup = \"xx\"").is_err());
    }

    #[test]
    fn matching() {
        let upper_d = KeySpec::parse("D").unwrap();
        assert!(upper_d.matches(&ev(KeyCode::Char('D'), KeyModifiers::SHIFT)));
        assert!(upper_d.matches(&ev(KeyCode::Char('D'), KeyModifiers::NONE)));
        assert!(!upper_d.matches(&ev(KeyCode::Char('d'), KeyModifiers::NONE)));
        let ctrl_c = KeySpec::parse("ctrl+c").unwrap();
        assert!(ctrl_c.matches(&ev(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!ctrl_c.matches(&ev(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert_eq!(ctrl_c.to_string(), "ctrl+c");
        assert!(
            KeySpec::parse("esc")
                .unwrap()
                .matches(&ev(KeyCode::Esc, KeyModifiers::NONE))
        );
    }
}
