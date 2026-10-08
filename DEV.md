# Development

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
