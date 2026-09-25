//! Merging and rebasing branches, including stopping on conflicts and resuming.
//!
//! Whenever an operation needs the working tree, the branch that was checked out (and any
//! autostash) is recorded in the git dir, and restored once the operation completes or is
//! aborted, so the user ends up where they started.

use std::fs;

use crate::git::{Op, Pending, Repo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Finished; the status message to show.
    Done(String),
    /// Stopped on conflicts and still in progress.
    Stopped,
}

/// A merge / rebase in progress, whoever started it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub op: Op,
    pub desc: String,
    /// Unresolved paths, relative to the top level.
    pub conflicts: Vec<String>,
}

pub fn in_progress(repo: &Repo) -> Result<Option<Operation>, String> {
    let Some(op) = repo.in_progress()? else {
        return Ok(None);
    };
    let desc = match repo.load_pending()? {
        Some(p) => p.desc,
        None => match op {
            Op::Merge => "merge in progress".into(),
            Op::Rebase => "rebase in progress".into(),
        },
    };
    Ok(Some(Operation {
        op,
        desc,
        conflicts: repo.conflicted_files()?,
    }))
}

/// Merge `branch` into `target`, or rebase `branch` onto `target`.
pub fn start(
    repo: &Repo,
    op: Op,
    branch: &str,
    target: &str,
    autostash: bool,
) -> Result<Outcome, String> {
    let (b, t) = (repo.rev_parse(branch)?, repo.rev_parse(target)?);
    let (desc, done) = match op {
        Op::Merge => (
            format!("merging {branch} into {target}"),
            format!("merged {branch} into {target}"),
        ),
        Op::Rebase => (
            format!("rebasing {branch} onto {target}"),
            format!("rebased {branch} onto {target}"),
        ),
    };
    match op {
        Op::Merge if repo.is_ancestor(&b, &t)? => {
            return Ok(Outcome::Done(format!("{target} already contains {branch}")));
        }
        Op::Rebase if repo.is_ancestor(&t, &b)? => {
            return Ok(Outcome::Done(format!(
                "{branch} is already based on {target}"
            )));
        }
        // Merging into a branch that isn't checked out: try without touching the working tree.
        Op::Merge if repo.current_branch()?.as_deref() != Some(target) => {
            let msg = format!("gim: {done}");
            if repo.is_ancestor(&t, &b)? {
                repo.update_branch(target, &b, &t, &msg)?;
                return Ok(Outcome::Done(format!(
                    "fast-forwarded {target} to {branch}"
                )));
            }
            if let Some(tree) = repo.merge_tree(&t, &b)? {
                let subject = format!("Merge branch '{branch}' into {target}");
                let commit = repo.commit_tree(&tree, &[&t, &b], &subject)?;
                repo.update_branch(target, &commit, &t, &msg)?;
                return Ok(Outcome::Done(done));
            }
            // Conflicts: they have to be resolved in the working tree.
        }
        _ => {}
    }

    let orig = repo.head_ref()?;
    let stash = if !repo.is_dirty()? {
        None
    } else if autostash {
        Some(repo.stash_push()?)
    } else {
        return Err(
            "uncommitted changes: commit or stash them first (or set autostash = true)".into(),
        );
    };
    repo.save_pending(&Pending {
        orig,
        stash,
        desc,
        done: done.clone(),
    })?;
    let result = match op {
        Op::Merge => repo.checkout(target).and_then(|_| repo.merge(branch)),
        Op::Rebase => repo.rebase(branch, target),
    };
    settle(repo, result, done)
}

/// Stage the resolved conflicts and carry on with the operation in progress.
pub fn resume(repo: &Repo) -> Result<Outcome, String> {
    let op = repo
        .in_progress()?
        .ok_or("no merge or rebase in progress")?;
    let files = repo.conflicted_files()?;
    let top = repo.toplevel()?;
    if let Some(f) = files.iter().find(|f| has_markers(&top.join(f))) {
        return Err(format!("{f} still has conflict markers"));
    }
    if !files.is_empty() {
        repo.add(&files)?;
    }
    let done = match repo.load_pending()? {
        Some(p) => p.done,
        None => match op {
            Op::Merge => "merge completed".into(),
            Op::Rebase => "rebase completed".into(),
        },
    };
    let result = match op {
        Op::Merge => repo.merge_continue(),
        Op::Rebase => repo.rebase_continue(),
    };
    settle(repo, result, done)
}

pub fn abort(repo: &Repo) -> Result<String, String> {
    let op = repo
        .in_progress()?
        .ok_or("no merge or rebase in progress")?;
    let what = match op {
        Op::Merge => {
            repo.merge_abort()?;
            "merge"
        }
        Op::Rebase => {
            repo.rebase_abort()?;
            "rebase"
        }
    };
    restore(repo)?;
    Ok(format!("{what} aborted"))
}

/// Turn the result of a step into an outcome, restoring the starting point unless the
/// operation is still in progress.
fn settle(repo: &Repo, result: Result<(), String>, done: String) -> Result<Outcome, String> {
    if result.is_err() && repo.in_progress()?.is_some() {
        return Ok(Outcome::Stopped);
    }
    let restored = restore(repo);
    result?;
    restored?;
    Ok(Outcome::Done(done))
}

/// Check the original branch out again and pop the autostash, if gim recorded them.
fn restore(repo: &Repo) -> Result<(), String> {
    let Some(p) = repo.load_pending()? else {
        return Ok(());
    };
    repo.clear_pending()?;
    if repo.head_ref()? != p.orig {
        repo.checkout(&p.orig)
            .map_err(|e| format!("could not check {} out again: {e}", p.orig))?;
    }
    if let Some(stash) = p.stash {
        repo.stash_pop(&stash)
            .map_err(|e| format!("could not restore stashed changes ({stash}): {e}"))?;
    }
    Ok(())
}

fn has_markers(path: &std::path::Path) -> bool {
    fs::read(path).is_ok_and(|bytes| {
        String::from_utf8_lossy(&bytes)
            .lines()
            .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "))
    })
}
