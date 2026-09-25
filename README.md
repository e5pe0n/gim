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
| `esc`          | cancel selection                         |
| `d`            | delete branch(es) (`git branch -d`)      |
| `D`            | force delete branch(es) (`git branch -D`) |
| `r`            | rename branch on the cursor              |
| `enter`        | checkout branch on the cursor            |
| `b`            | create a branch from the cursor branch and check it out (`git checkout -b`) |
| `y`            | yank the branch on the cursor            |
| `p`            | merge the yanked branch into the cursor branch (checks the cursor branch out first) |
| `P`            | rebase the yanked branch onto the cursor branch (`git rebase <cursor> <yanked>`) |
| `R`            | reload branch list                       |
| `q`            | quit                                     |

`d` / `D` act on the visual selection when one is active, otherwise on the cursor branch.

If `p` / `P` stop on conflicts, gim offers `resolve` / `abort` (`h`/`l` to choose, `enter` to select,
or `r` / `a` directly):

- `resolve` opens the configured `editor` on the conflicted files and leaves the merge / rebase in
  progress; finish it with `git add` and `git commit` / `git rebase --continue`.
- `abort` runs `git merge --abort` / `git rebase --abort`.

Either way you return to the branch list.

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
reload       = "R"
quit         = ["q", "ctrl+c"]
```
