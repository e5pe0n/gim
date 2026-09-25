//! Thin wrappers around the git commands gim needs.

use std::{path::PathBuf, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub hash: String,
    pub subject: String,
}

/// A repository that git commands run in.
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

impl Repo {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Repo { dir: dir.into() }
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .args(args)
            .output()
            .map_err(|e| format!("failed to run git: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if msg.is_empty() {
                format!("git exited with {}", out.status)
            } else {
                msg
            })
        }
    }

    /// Local branches, sorted by name.
    pub fn list_branches(&self) -> Result<Vec<Branch>, String> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(HEAD)%09%(refname:short)%09%(objectname:short)%09%(contents:subject)",
            "refs/heads",
        ])?;
        Ok(out
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let mut f = line.splitn(4, '\t');
                let mut next = || f.next().unwrap_or("").to_string();
                Branch {
                    current: next() == "*",
                    name: next(),
                    hash: next(),
                    subject: next(),
                }
            })
            .collect())
    }

    /// `git branch -d` (or `-D` when `force`).
    pub fn delete_branches(&self, names: &[String], force: bool) -> Result<(), String> {
        let mut args = vec!["branch", if force { "-D" } else { "-d" }, "--"];
        args.extend(names.iter().map(String::as_str));
        self.run(&args).map(|_| ())
    }

    pub fn rename_branch(&self, old: &str, new: &str) -> Result<(), String> {
        self.run(&["branch", "-m", "--", old, new]).map(|_| ())
    }

    /// `git checkout -b <name> <start>`.
    pub fn checkout_new(&self, name: &str, start: &str) -> Result<(), String> {
        self.run(&["checkout", "-b", name, start, "--"]).map(|_| ())
    }

    pub fn checkout(&self, name: &str) -> Result<(), String> {
        self.run(&["checkout", name, "--"]).map(|_| ())
    }
}
