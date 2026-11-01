# enforce-us

Automation for maintaining this `openai/codex` fork's enforce-us patch,
continuing the convention from `aphoh/codex`.

## What the patch does

`enforce-us.patch` (original commit `526b3e0f68d82ee8bdee894946e2fb5f5f6c1d60`
on aphoh/codex) replaces whatever requirements-TOML content is loaded with the
literal `enforce_residency = "us"` in two places:

- `codex-rs/config/src/cloud_config_bundle.rs` — `CloudRequirementsTomlBundle`
- `codex-rs/config/src/loader/mod.rs` — `load_requirements_toml` (also renames
  `Ok(contents)` to `Ok(_contents)`)

Net effect: every requirements source resolves to a hardcoded US data-residency
requirement.

## Layout

| Ref | Purpose |
|---|---|
| `main` | pristine mirror of `upstream/main`; never modified |
| `enforce-us-tooling` | **default branch**; this file, `enforce-us.patch`, `scripts/cut-release.sh`, and both workflows |
| `enforce-us-X.Y.Z` | `rust-vX.Y.Z` + the am'd patch + a commit adding the release workflow |
| `enforce-us-vX.Y.Z` | tag on the above; GitHub Release `Codex X.Y.Z enforce-us` |
| `enforce-us-X.Y.Z-conflict` | pushed when the patch stops applying cleanly |

`enforce-us-tooling` is the default branch because GitHub only runs scheduled
workflows (and `workflow_dispatch` for non-tag refs) from the default branch.

## Workflows

- **enforce-us-sync.yml** — every 6h + manual. Diffs upstream stable
  `rust-vX.Y.Z` tags against local `enforce-us-vX.Y.Z` tags, and cuts each
  missing version **newer than the latest existing enforce-us tag** (no
  backfill; floor defaults to `0.156.1` if history is empty). On success it
  pushes branch+tag and dispatches `enforce-us-release.yml`. On a `git am`
  conflict it pushes `enforce-us-X.Y.Z-conflict` with `.rej` rejects committed
  and opens issue `enforce-us: conflict on X.Y.Z` (deduped by title).
- **enforce-us-release.yml** — on `enforce-us-v*` tag push, dispatch, or
  workflow_call. Builds `codex` + `codex-code-mode-host` release binaries for
  `x86_64-unknown-linux-gnu` (ubuntu-24.04) and `aarch64-apple-darwin`
  (macos-15), reusing the Rust toolchain version pinned in upstream's
  `rust-release.yml` at the built tag and upstream's prebuilt `rusty_v8`
  artifacts. Verifies `codex --version` == `codex-cli X.Y.Z`, then publishes
  release `Codex X.Y.Z enforce-us` with assets
  `codex-X.Y.Z-enforce-us-linux-x86-64.tar.gz` /
  `-macos-arm64.tar.gz`, the source commit, verified version output, and
  archive SHA-256s.

## Secrets

None required. `GITHUB_TOKEN` (with `contents`, `issues`, `pull-requests`,
`actions: write`) is enough because the sync workflow dispatches release
builds explicitly — pushes made with `GITHUB_TOKEN` do not fire tag-push
triggers. To make raw tag pushes trigger builds directly, add a PAT as
`ENFORCE_US_PAT` and uncomment the `token:` line on the sync checkout step.

## Manual usage

```sh
git remote add upstream https://github.com/openai/codex.git
git fetch upstream --tags
git checkout enforce-us-tooling
./scripts/cut-release.sh 0.167.0     # after the tag exists upstream
```

Then dispatch the build (or push the tag with a PAT-authenticated remote):

```sh
gh workflow run enforce-us-release.yml --ref enforce-us-tooling -f tag=enforce-us-v0.167.0
```

## Resolving a conflict

1. Check out the conflict branch: `git checkout enforce-us-X.Y.Z-conflict`.
   The committed `.rej` files show which hunks failed.
2. Fix the source by hand so the intended change is present, drop the `.rej`
   files, commit.
3. Tag `enforce-us-vX.Y.Z` on that branch and push; dispatch the release
   workflow.
4. Close the `enforce-us: conflict on X.Y.Z` issue.

If upstream restructures `cloud_config_bundle.rs` / `loader/mod.rs` so the
patch semantics no longer map, regenerate `enforce-us.patch` on
`enforce-us-tooling` rather than hand-editing each release.
