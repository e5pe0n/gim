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
- `gim --cwd-file <path>` writes the worktree gim moved to into `<path>` on exit (see
  [Moving to a worktree](#moving-to-a-worktree)).

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
| `enter`        | checkout branch on the cursor (and quit, see `quit_on_checkout`); on a remote branch, check out a local branch tracking it; on a branch in another worktree (or with `[worktree] enabled`), move to its worktree |
| `b`            | create a branch from the cursor branch and check it out (`git checkout -b`) |
| `y`            | yank the branch on the cursor            |
| `p`            | merge the yanked branch into the cursor branch |
| `P`            | rebase the yanked branch onto the cursor branch |
| `o`            | continue / resolve / abort the merge or rebase in progress |
| `f`            | fetch all remotes (`git fetch --all --prune`) and reload |
| `R`            | reload branch list                       |
| `q`            | quit                                     |

`d` / `D` act on the visual selection when one is active, otherwise on the cursor branch.

Branches checked out in another worktree are marked with `+` (in cyan). Checking one out moves gim
to that worktree (see [Moving to a worktree](#moving-to-a-worktree)). Deleting one removes that
worktree first (`git worktree remove`), which git refuses while it has changes or untracked files;
`D` removes it anyway (`--force`), discarding them.

### Moving to a worktree

A branch checked out in another worktree can't be checked out here too, so `enter` on one moves gim
to that worktree instead: the list is reloaded there and every action runs there.

With `[worktree] enabled = true`, every branch is checked out in a worktree of its own: `enter` on a
branch that has none creates one at the configured `path` (`git worktree add`) and moves there, and
`b` creates the new branch in a new worktree. On a remote branch the new local branch tracks it.
The branch checked out in the current worktree stays here. With the default `path`, worktrees go in
`.<repo>.worktrees/` inside the main worktree, which `git status` there lists as untracked; ignore
it with e.g. `echo '/.*.worktrees/' >> .git/info/exclude`. gim can't change
your shell's directory itself; to `cd` into the worktree when gim quits, use a shell function like:

```sh
gim() {
  local tmp dir
  tmp="$(mktemp)"
  command gim --cwd-file "$tmp" "$@"
  dir="$(cat -- "$tmp")"
  rm -f -- "$tmp"
  [ -n "$dir" ] && [ "$dir" != "$PWD" ] && cd -- "$dir"
}
```

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

[worktree]
# Check out each branch in its own worktree (`enter` / `b`), creating it if needed, and move there.
enabled = false
# Where a branch's worktree is created: `{repo}` is the main worktree's directory name, `{branch}`
# the branch name. A relative path is relative to the main worktree; `~/` is your home directory.
path = ".{repo}.worktrees/{branch}"

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
