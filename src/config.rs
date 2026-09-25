//! TOML configuration: key bindings and options.

use std::{env, fmt, fs, io, path::PathBuf};

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
    pub keys: KeyMap,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            confirm_delete: true,
            editor: "code".into(),
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
