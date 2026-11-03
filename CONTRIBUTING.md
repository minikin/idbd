# Contributing

## Commit messages

Commits follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>(<scope>): <summary>

<body>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.

Scopes are optional, lowercase, and name the part of the repository that changes:

| Scope | Path |
| --- | --- |
| `core` | `crates/idbd` |
| `lab` | `crates/idbd-lab` |
| `wasm` | `crates/idbd-wasm` |
| `web` | `web/` |
| `results` | `docs/results`, `docs/benchmarks.md` |

A `!` after the type or scope marks a breaking change to the public API of `idbd`, for example `feat(core)!: return Update by value`. Explain the break in the body.

Write the summary in the imperative mood, in lowercase, without a trailing period. The body says why the change is needed and records anything a reviewer must know, such as a CRAP regression and its reason.

CI checks every non-merge commit in a pull request. Run the same check locally:

```sh
scripts/check-commits.sh origin/main..HEAD
```

Commits merged before the convention was adopted (up to pull request #1) do not follow it.

## Checks

`just check` runs formatting, Clippy, tests and rustdoc. `scripts/coverage.sh` runs the coverage and CRAP gate, and `just web-test` runs the browser tests.
