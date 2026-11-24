# Development

## Testing worktree mode manually

`cargo test` covers the logic, but the TUI, `--cwd-file` and the shell `cd` need a real terminal.
Run everything in a throwaway sandbox so no worktree lands in your real data dir.

Set up a sandbox with an upstream repo (for remote branches), a clone to work in, a config that
enables worktree mode, and a `gim` shell function running the debug build:

```sh
cargo build
GIM_BIN="$PWD/target/debug/gim"
GIM_SANDBOX="$(mktemp -d)"
export XDG_DATA_HOME="$GIM_SANDBOX/data"

git init -q "$GIM_SANDBOX/upstream"
git -C "$GIM_SANDBOX/upstream" commit -q --allow-empty -m init
git -C "$GIM_SANDBOX/upstream" branch feat/remote
git clone -q "$GIM_SANDBOX/upstream" "$GIM_SANDBOX/repo"
git -C "$GIM_SANDBOX/repo" branch feat/local
printf '[worktree]\nenabled = true\n' > "$GIM_SANDBOX/config.toml"

gim() {
  local tmp dir
  tmp="$(mktemp)"
  "$GIM_BIN" -c "$GIM_SANDBOX/config.toml" --cwd-file "$tmp" "$@"
  dir="$(cat -- "$tmp")"
  rm -f -- "$tmp"
  [ -n "$dir" ] && [ "$dir" != "$PWD" ] && cd -- "$dir"
}
cd "$GIM_SANDBOX/repo"
```

Worktrees are created under `$XDG_DATA_HOME/gim/worktrees/<repo_path>/`. The sandbox is outside your
home directory, so `<repo_path>` is the repo's absolute path without the leading `/` (on macOS
`private/var/folders/…/repo`). Run `gim` and check each case:

1. `enter` on `feat/local`: gim quits with `created worktree …/feat/local (feat/local)`, the shell
   is now in that directory and `git branch --show-current` prints `feat/local`.
2. `gim` again: `feat/local` is current and the default branch is marked `+`. `enter` on the
   default branch prints `moved to worktree …/repo (…)` and the shell is back in
   `$GIM_SANDBOX/repo`, with no new worktree.
3. `enter` on `origin/feat/remote`: a worktree `…/feat/remote` is created, and in it
   `git rev-parse --abbrev-ref @{upstream}` prints `origin/feat/remote`.
4. `b` on any branch, type `feat/new`, `enter`: the new branch is created in a new worktree
   `…/feat/new`.
5. `enter` on the current branch: `switched to …`, no worktree is created and the shell stays put.
6. `d` / `D` on a branch with a worktree: the worktree directory is removed with the branch.
7. With `quit_on_checkout = false` as the first line of the config (above `[worktree]`),
   `enter` on a `+` branch reloads the list in that worktree without quitting, and `q` then `cd`s
   there.
8. With `enabled = false`, `enter` on a branch without a worktree checks it out in place as before;
   on a `+` branch it still moves to that worktree.

`git worktree list` (in any of the worktrees) shows what exists at each step. Clean up with:

```sh
cd ~ && rm -rf "$GIM_SANDBOX" && unset -f gim && unset XDG_DATA_HOME GIM_BIN GIM_SANDBOX
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
secret. Both workflows run their writing jobs in this environment. Publishing the release creates
the `vX.Y.Z` tag as the app, so a tag ruleset can limit `v*` tag creation to the app and admins.
