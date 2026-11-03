# Security policy

## Supported versions

Only the latest published release of `idbd` receives fixes. Before 1.0, a fix ships as a new patch or minor release, not as a backport.

## Reporting a vulnerability

Report it privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue.

Include the crate version, a minimal input that triggers the problem and what you observed.

## Scope

`idbd` contains no `unsafe` code, has no runtime dependencies and performs no I/O. Relevant reports include:

- a panic reachable through the public API with any input, including non-finite values and untrusted `State` checkpoints
- a public operation that reports success but leaves the learner in a state that `Idbd::from_state` would reject
- memory or time use that grows without bound for a fixed number of features

A numerical result that differs from Sutton (1992) is a bug, not a vulnerability. Open a normal issue for it.

The demo under `web/` and the crates `idbd-lab` and `idbd-wasm` are not published to crates.io. Report problems in them the same way.
