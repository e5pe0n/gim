# gim

A TUI for managing git branches by vim keybindings.

![gim demo](assets/demo.gif)

## Install

Download the archive for your platform from [Releases](https://github.com/e5pe0n/gim/releases),
extract it and put `gim` on your `PATH`, or build from source:

```sh
cargo install --path .
```

Run `gim` inside a git repository.

- `gim --version` (`-V`) prints the version.
- `gim self-update` replaces the binary with the latest release.

## Keys (defaults)

| Key            | Action                                   |
| -------------- | ---------------------------------------- |
| `j` / `k`      | move cursor down / up                    |
| `g` / `G`      | jump to top / bottom                     |
| `v`            | start visual selection (press again to end) |
| `esc`          | cancel selection / clear the search filter |
| `/`            | fuzzy-search branches                    |
| `d`            | delete branch(es) (`git branch -d`)      |
| `D`            | force delete branch(es) (`git branch -D`) |
| `r`            | rename branch on the cursor              |
| `enter`        | checkout branch on the cursor (and quit, see `quit_on_checkout`); on a remote branch, check out a local branch tracking it |
| `b`            | create a branch from the cursor branch and check it out (`git checkout -b`) |
| `y`            | yank the branch on the cursor            |
| `p`            | merge the yanked branch into the cursor branch |
| `P`            | rebase the yanked branch onto the cursor branch |
| `o`            | continue / resolve / abort the merge or rebase in progress |
| `f`            | fetch all remotes (`git fetch --all --prune`) and reload |
| `R`            | reload branch list                       |
| `q`            | quit                                     |

`d` / `D` act on the visual selection when one is active, otherwise on the cursor branch.

Branches checked out in another worktree are marked with `+` (in cyan). Deleting one removes that
worktree first (`git worktree remove`), which git refuses while it has changes or untracked files;
`D` removes it anyway (`--force`), discarding them.

### Remote branches

Remote-tracking branches (`origin/feat`, in red) are listed after the local ones. `enter` on one
checks out the local branch of the same name, creating it to track the remote branch
(`git checkout --track`) if it doesn't exist yet. `b` branches off it, and it can be yanked and
merged into a local branch; deleting, renaming, merging into or rebasing a remote branch is refused.
Set `remotes = false` to list local branches only.

### Search

`/` filters the list as you type, fzf-style: the letters must appear in order, and matches at word
starts (after `/`, `-`, `_`, `.`) and consecutive runs rank first. Matching is case-insensitive
unless the query has an upper-case letter. While typing, `up`/`down` (or `ctrl+p`/`ctrl+n`) move
the cursor; `enter` keeps the filter and returns to the list, where every key works on the matching
branches (e.g. `/`, `log`, `enter`, `enter` checks out the best match for "log"); `esc` clears it;
`backspace` on an empty query leaves search.

### Merge and rebase

gim leaves you on the branch you started on:

- A merge into a branch that isn't checked out is done without touching the working tree
  (`git merge-tree`), unless it has conflicts.
- Otherwise gim checks out what it needs and switches back to your branch when the merge / rebase
  finishes or is aborted.

If that needs the working tree and you have uncommitted changes, gim refuses to start, or with
`autostash = true` stashes them and restores them at the end.

When a merge / rebase stops on conflicts (or one is already in progress, however it was started),
the title bar shows it and gim offers `continue` / `resolve` / `abort` (`h`/`l` to choose, `enter`
to select, `c` / `r` / `a` directly, `esc` to go back to the list; `o` reopens it):

- `resolve` opens the configured `editor` on the conflicted files, then returns to the list.
- `continue` stages the resolved files (refusing while conflict markers remain) and runs
  `git commit` / `git rebase --continue`. If a rebase stops again, you're prompted again.
- `abort` runs `git merge --abort` / `git rebase --abort`.

## Config

gim reads `$GIM_CONFIG`, else `$XDG_CONFIG_HOME/gim/config.toml`, else `~/.config/gim/config.toml`
(or pass `-c/--config <path>`). Every setting is optional; omitted actions keep their defaults.
Each binding is a key name or a list of key names (`"k"`, `"up"`, `"ctrl+n"`, `"esc"`, ...).

```toml
# Ask before deleting branches.
confirm_delete = true

# Command run on the conflicted files to resolve merge / rebase conflicts
# (split on whitespace; run from the repository root). Terminal editors work too, e.g. "nvim".
editor = "code"

# Stash uncommitted changes around a merge / rebase instead of refusing to start.
autostash = false

# Quit after checking out a branch (`enter` / `b`).
quit_on_checkout = true

# List remote-tracking branches after the local ones.
remotes = true

[keys]
up           = ["k", "up"]
down         = ["j", "down"]
top          = ["g", "home"]
bottom       = ["G", "end"]
delete       = "d"
force_delete = "D"
visual       = "v"
cancel       = "esc"
rename       = "r"
checkout     = "enter"
checkout_new = "b"
yank         = "y"
merge        = "p"
rebase       = "P"
operation    = "o"
search       = "/"
fetch        = "f"
reload       = "R"
quit         = ["q", "ctrl+c"]
```

## Releasing

1. Actions → **Prepare release** → Run workflow, choosing the part to bump (`patch` / `minor` /
   `major`) and the branch to release from (default `develop`). It bumps the version in
   `Cargo.toml`, pushes `release/vX.Y.Z` and opens a PR to `main` whose description is the
   generated release note.
2. Review the PR and edit its description to adjust the release note.
3. Merge it. **Release** tags `vX.Y.Z` on the merge commit, builds the binaries and publishes the
   release with the PR description as its note.
4. Merge `main` back into `develop`.

Repository setup: Prepare release pushes and opens the PR as a GitHub App (so CI runs on the release
PR). Install an app with *Contents* and *Pull requests* read and write access on this repository, and
add its client ID as the `RELEASE_APP_CLIENT_ID` variable. Create a `release` environment limited to
the `develop` and `main` branches and add a private key of the app as its `RELEASE_APP_PRIVATE_KEY`
secret. Both workflows run their writing jobs in this environment.
