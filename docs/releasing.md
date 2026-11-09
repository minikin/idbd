# Releasing `idbd`

Only `crates/idbd` is published. `idbd-lab` and `idbd-wasm` have `publish = false`.

## How a release happens

1. Conventional Commits land on `main`. The `Release` workflow runs release-plz, which opens or updates a pull request labeled `release` that bumps the version in `crates/idbd/Cargo.toml`. release-plz runs `cargo-semver-checks` to choose the bump. It does not edit `CHANGELOG.md` at the repository root (`changelog_update = false`), because the changelog is written for crate users, not generated from commit messages.
2. A maintainer moves the `[Unreleased]` notes in `CHANGELOG.md` under the new version and its date in that pull request, then merges it.
3. A maintainer starts the `Release` workflow on `main` by hand (Actions, Release, Run workflow). The `publish` job checks the package contents, publishes any version that is not on crates.io yet, tags it `v<version>` and creates a GitHub release.

Nothing is published without step 3.

## One-time setup

- Add a crates.io API token with the `publish-new` and `publish-update` scopes as the `CARGO_REGISTRY_TOKEN` secret of the `crates-io` environment. Add required reviewers to that environment if a second person should approve each publish.
- In the repository settings, allow GitHub Actions to create pull requests. This is on since 2026-11-08.
- Pull requests that release-plz opens with `GITHUB_TOKEN` do not trigger CI. Close and reopen the pull request, or push an empty commit to it, to run the checks before merging.
- Make the repository public before the first publish, so that the `repository` link in the crate metadata works.

## The first release

0.1.0 is already the version in `crates/idbd/Cargo.toml`, and its notes sit under `## [0.1.0] - 2026-11-08` in `CHANGELOG.md`. If the first publish happens on a later day, change that date in a normal pull request before starting the workflow.

The CI `semver` job skips its check until a version is on crates.io, so the API can still change before 0.1.0 ships. From then on it compares every pull request with the newest published version.

## Before every release

The `publish` job runs only after `ci-passed` finds a successful CI run for the exact commit on `main`. Merge, wait for CI to pass, then start the workflow.

`cargo package -p idbd --list` must show only the crate sources, `README.md`, `CHANGELOG.md`, the two license files and the manifest files Cargo adds. The `publish` job and the `Package the core crate` CI job both check this.
