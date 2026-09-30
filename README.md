# gim

A TUI for managing git branches.

## Install

```sh
cargo install --path .
```

Run `gim` inside a git repository.

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
| `enter`        | checkout branch on the cursor (and quit, see `quit_on_checkout`) |
| `b`            | create a branch from the cursor branch and check it out (`git checkout -b`) |
| `y`            | yank the branch on the cursor            |
| `p`            | merge the yanked branch into the cursor branch |
| `P`            | rebase the yanked branch onto the cursor branch |
| `o`            | continue / resolve / abort the merge or rebase in progress |
| `R`            | reload branch list                       |
| `q`            | quit                                     |

`d` / `D` act on the visual selection when one is active, otherwise on the cursor branch.

### Search

`/` filters the list as you type, fzf-style: the letters must appear in order, and matches at word
starts (after `/`, `-`, `_`, `.`) and consecutive runs rank first. Matching is case-insensitive
unless the query has an upper-case letter. While typing, `up`/`down` (or `ctrl+p`/`ctrl+n`) move
the cursor; `enter` keeps the filter and returns to the list, where every key works on the matching
branches (e.g. `/`, `log`, `enter`, `enter` checks out the best match for "log"); `esc` clears it, as does
backspacing the query away.

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
reload       = "R"
quit         = ["q", "ctrl+c"]
```
