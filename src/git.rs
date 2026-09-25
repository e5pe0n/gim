//! Thin wrappers around the git commands gim needs.

use std::{fs, io, path::PathBuf, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub hash: String,
    pub subject: String,
}

/// A merge or rebase that can stop on conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Merge,
    Rebase,
}

/// What gim needs to restore once a merge / rebase it started is finished or aborted.
/// Kept in the git dir so it survives restarting gim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    /// Branch (or commit, if HEAD was detached) to check out again.
    pub orig: String,
    /// Stash commit made by autostash, to pop afterwards.
    pub stash: Option<String>,
    /// e.g. "merging feat into main".
    pub desc: String,
    /// Status message on success, e.g. "merged feat into main".
    pub done: String,
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
        self.run_env(args, &[])
    }

    fn run_env(&self, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .args(args)
            .envs(env.iter().copied())
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

    /// Run git for a yes / no answer: exit 0 is true, 1 is false, anything else an error.
    fn test(&self, args: &[&str]) -> Result<(bool, String), String> {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .args(args)
            .output()
            .map_err(|e| format!("failed to run git: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        match out.status.code() {
            Some(0) => Ok((true, stdout)),
            Some(1) => Ok((false, stdout)),
            _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        }
    }

    /// The checked-out branch, or `None` when HEAD is detached.
    pub fn current_branch(&self) -> Result<Option<String>, String> {
        let (on_branch, out) = self.test(&["symbolic-ref", "--short", "-q", "HEAD"])?;
        Ok(on_branch.then(|| out.trim().to_string()))
    }

    /// The checked-out branch, or the commit when HEAD is detached.
    pub fn head_ref(&self) -> Result<String, String> {
        match self.current_branch()? {
            Some(b) => Ok(b),
            None => self.rev_parse("HEAD"),
        }
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String, String> {
        let arg = format!("{rev}^{{commit}}");
        Ok(self
            .run(&["rev-parse", "--verify", "-q", &arg])?
            .trim()
            .to_string())
    }

    /// Whether tracked files have uncommitted changes.
    pub fn is_dirty(&self) -> Result<bool, String> {
        Ok(!self
            .run(&["status", "--porcelain", "--untracked-files=no"])?
            .trim()
            .is_empty())
    }

    pub fn is_ancestor(&self, ancestor: &str, of: &str) -> Result<bool, String> {
        Ok(self.test(&["merge-base", "--is-ancestor", ancestor, of])?.0)
    }

    /// Merge two commits without touching the working tree. `Some(tree)` when clean,
    /// `None` on conflicts.
    pub fn merge_tree(&self, ours: &str, theirs: &str) -> Result<Option<String>, String> {
        let (clean, out) = self.test(&["merge-tree", "--write-tree", ours, theirs])?;
        Ok(clean.then(|| out.lines().next().unwrap_or("").to_string()))
    }

    pub fn commit_tree(&self, tree: &str, parents: &[&str], msg: &str) -> Result<String, String> {
        let mut args = vec!["commit-tree", tree, "-m", msg];
        for p in parents {
            args.extend(["-p", p]);
        }
        Ok(self.run(&args)?.trim().to_string())
    }

    /// Point `branch` at `new`, provided it is still at `old`.
    pub fn update_branch(
        &self,
        branch: &str,
        new: &str,
        old: &str,
        msg: &str,
    ) -> Result<(), String> {
        let refname = format!("refs/heads/{branch}");
        self.run(&["update-ref", "-m", msg, &refname, new, old])
            .map(|_| ())
    }

    /// `git merge` `branch` into the checked-out branch.
    pub fn merge(&self, branch: &str) -> Result<(), String> {
        self.run(&["merge", "--no-edit", branch]).map(|_| ())
    }

    /// `git rebase <onto> <branch>`: replay `branch` on top of `onto` (leaves `branch` checked out).
    pub fn rebase(&self, branch: &str, onto: &str) -> Result<(), String> {
        self.run(&["rebase", onto, branch]).map(|_| ())
    }

    /// Conclude a merge whose conflicts are resolved and staged.
    pub fn merge_continue(&self) -> Result<(), String> {
        self.run(&["commit", "--no-edit"]).map(|_| ())
    }

    pub fn rebase_continue(&self) -> Result<(), String> {
        // Keep the commit message instead of opening an editor.
        self.run_env(&["rebase", "--continue"], &[("GIT_EDITOR", "true")])
            .map(|_| ())
    }

    pub fn merge_abort(&self) -> Result<(), String> {
        self.run(&["merge", "--abort"]).map(|_| ())
    }

    pub fn rebase_abort(&self) -> Result<(), String> {
        self.run(&["rebase", "--abort"]).map(|_| ())
    }

    /// Paths with unresolved conflicts, relative to the top level.
    pub fn conflicted_files(&self) -> Result<Vec<String>, String> {
        let out = self.run(&["diff", "--name-only", "--diff-filter=U"])?;
        Ok(out
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    /// Stage `paths` (relative to the top level), including deletions.
    pub fn add(&self, paths: &[String]) -> Result<(), String> {
        let specs: Vec<String> = paths.iter().map(|p| format!(":(top){p}")).collect();
        let mut args = vec!["add", "-A", "--"];
        args.extend(specs.iter().map(String::as_str));
        self.run(&args).map(|_| ())
    }

    /// Stash tracked changes and return the stash commit.
    pub fn stash_push(&self) -> Result<String, String> {
        self.run(&["stash", "push", "-q", "-m", "gim autostash"])?;
        self.rev_parse("refs/stash")
    }

    /// Pop the stash entry whose commit is `oid`.
    pub fn stash_pop(&self, oid: &str) -> Result<(), String> {
        let list = self.run(&["stash", "list", "--format=%H"])?;
        let i = list
            .lines()
            .position(|h| h == oid)
            .ok_or_else(|| format!("stash {oid} not found"))?;
        self.run(&["stash", "pop", "-q", &format!("stash@{{{i}}}")])
            .map(|_| ())
    }

    fn git_path(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self
            .dir
            .join(self.run(&["rev-parse", "--git-path", name])?.trim()))
    }

    /// The merge / rebase in progress, if any.
    pub fn in_progress(&self) -> Result<Option<Op>, String> {
        if self.git_path("MERGE_HEAD")?.exists() {
            return Ok(Some(Op::Merge));
        }
        for dir in ["rebase-merge", "rebase-apply"] {
            if self.git_path(dir)?.exists() {
                return Ok(Some(Op::Rebase));
            }
        }
        Ok(None)
    }

    pub fn save_pending(&self, p: &Pending) -> Result<(), String> {
        let text = format!(
            "orig={}\nstash={}\ndesc={}\ndone={}\n",
            p.orig,
            p.stash.as_deref().unwrap_or(""),
            p.desc,
            p.done
        );
        fs::write(self.git_path(PENDING)?, text).map_err(|e| e.to_string())
    }

    pub fn load_pending(&self) -> Result<Option<Pending>, String> {
        let text = match fs::read_to_string(self.git_path(PENDING)?) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let mut p = Pending::default();
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            match k {
                "orig" => p.orig = v.into(),
                "stash" if !v.is_empty() => p.stash = Some(v.into()),
                "desc" => p.desc = v.into(),
                "done" => p.done = v.into(),
                _ => {}
            }
        }
        Ok(Some(p))
    }

    pub fn clear_pending(&self) -> Result<(), String> {
        match fs::remove_file(self.git_path(PENDING)?) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }

    /// Absolute path of the working tree's top level.
    pub fn toplevel(&self) -> Result<PathBuf, String> {
        Ok(PathBuf::from(
            self.run(&["rev-parse", "--show-toplevel"])?.trim(),
        ))
    }
}

/// File in the git dir holding [`Pending`].
const PENDING: &str = "gim-pending";
