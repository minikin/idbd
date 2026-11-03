# Fuzzing

Three [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets exercise the public API of `idbd` with arbitrary inputs:

- `update`: any configuration, including bounds, and any stream of samples, some with the wrong length. An accepted update must leave a state that `Idbd::from_state` accepts and restores exactly, and `predict` must agree with the prediction the update reports. A rejected update must leave the learner unchanged.
- `from_state`: any checkpoint. `Idbd::from_state` either rejects it or restores it exactly, and an update of the restored learner either succeeds or changes nothing.

- `autostep`: the same properties for `idbd::autostep::Autostep`, with any initial step size, mu and tau, plus an arbitrary checkpoint that must be rejected or restored exactly.

No target may panic.

```sh
cd fuzz
cargo +nightly fuzz run update -- -max_total_time=120
cargo +nightly fuzz run from_state -- -max_total_time=120
```

The `Fuzz` workflow runs each target for one minute on pull requests that touch the core or this directory, and for ten minutes every week. A failing input is uploaded as a workflow artifact. Reproduce it with `cargo +nightly fuzz run <target> <file>`.
