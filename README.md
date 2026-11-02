# idbd

A Rust implementation of Incremental Delta-Bar-Delta (IDBD, Sutton 1992), an approximate
reproduction of the paper's experiments, and IDBD Explorer, a browser demo that runs the same
Rust code through WebAssembly.

IDBD is the LMS rule with one learning rate per input, `alpha_i = exp(beta_i)`. It adapts each
`beta_i` online by gradient descent on the squared prediction error, so inputs that carry signal
keep large steps and inputs that only add noise get small ones.

| Path | Contents |
| --- | --- |
| [`crates/idbd`](crates/idbd) | The published library. No dependencies, no `unsafe`, MSRV 1.85. |
| [`crates/idbd-lab`](crates/idbd-lab) | Unpublished. Seeded tasks, an LMS baseline, experiment and benchmark binaries. |
| [`crates/idbd-wasm`](crates/idbd-wasm) | Unpublished. The `wasm-bindgen` adapter used by the demo. |
| [`web`](web) | IDBD Explorer, a static browser app with Playwright tests. |
| [`docs`](docs) | Reproduction instructions, generated results and benchmarks. |
| [`features/idbd.feature`](features/idbd.feature) | The behavior specification: 44 scenarios in 12 rules. |

## Using the crate

```rust
use idbd::{Config, Idbd};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut learner = Idbd::new(3, Config::new(0.05, 0.01)?)?;
    let report = learner.update(&[0.5, -1.0, 1.0], 2.0)?;
    println!("error before learning: {}", report.error);
    Ok(())
}
```

`crates/idbd/examples/latency.rs` is a complete streaming application: input and target
scaling, predicting before learning, skipping an invalid sample, and checkpointing the learner
with its stream position. Run it with `cargo run -p idbd --example latency`.

The [crate README](crates/idbd/README.md) covers the update equations, configuration, bounds,
checkpoints, when to use IDBD and its limitations.

## Results

Every number below was produced by the binaries in `crates/idbd-lab` with recorded seeds. The
paper's random draws are unpublished, so these are approximate replications. Each results page
lists the protocol differences.

**Experiment 1 (Figure 3).** 20 inputs, 5 relevant, one sign flip every 20 samples, 20,000
warm-up and 10,000 measured samples, 20 seeds, no bounds. Mean squared error with one standard
error:

| Learner | Best parameter in our grid | MSE | Sutton (1992) |
| --- | --- | --- | --- |
| LMS | alpha 0.03 | 3.402 +/- 0.017 | about 3.5 |
| IDBD, initial alpha 0.05 | theta 0.015 | 1.525 +/- 0.007 | about 1.5 |

At theta 0.02, 5 of 20 IDBD runs failed with a step-size overflow. LMS diverged at alpha 0.09
and above. [Details and plot](docs/results/sutton1992-experiment1/README.md).

**Experiment 2 (Figures 4 and 5).** theta 0.001, 250,000 samples, 5 seeds. The relevant inputs'
step sizes end at 0.129 on average (paper: 0.13 +/- 0.015). The irrelevant inputs end between
0.0075 and 0.0078, slightly above the paper's "all below 0.007". A fixed-rate sweep over the
relevant inputs has its minimum at alpha 0.13 to 0.14 (paper: 0.13 +/- 0.01).
[Details and plots](docs/results/sutton1992-experiment2/README.md).

**Evaluation against LMS (new tasks).** LMS alpha and IDBD theta were each chosen on tuning
seeds 1001 to 1005, then evaluated on seeds 1 to 10:

| Task | LMS MSE | IDBD MSE |
| --- | --- | --- |
| Sutton 1992 task | 3.422 +/- 0.021 | 1.516 +/- 0.010 |
| Drifting coefficients (extension) | 0.219 +/- 0.002 | 0.110 +/- 0.001 |
| Changing relevance (extension) | 3.428 +/- 0.027 | 2.519 +/- 0.024 |
| Stationary | both below 1e-38 | both below 1e-38 |

These are noise-free synthetic tasks where a few inputs matter and the relationship changes.
They do not show that IDBD beats a tuned LMS in general. [Method and per-seed
results](docs/results/evaluation/README.md).

**Performance.** On an Apple M4 Max, one update with 20 features takes about 97 ns and costs
about 5 ns per feature up to 10,000 features. [Method and full table](docs/benchmarks.md).

## Reproducing the results

```sh
cargo run --release -p idbd-lab --bin sutton1992-exp1   # Experiment 1, about 3 s
cargo run --release -p idbd-lab --bin sutton1992-exp2   # Experiment 2 and the Figure 5 sweep
cargo run --release -p idbd-lab --bin evaluate          # IDBD against LMS on four tasks
cargo run --release -p idbd-lab --bin bench             # core timings
```

Each binary writes CSV, SVG and Markdown under `docs/results` (or `--out <dir>`). Reruns on the
same build and platform reproduce the files byte for byte. [docs/reproduction.md](docs/reproduction.md)
lists the generator, seeds, grids and sign-flip timing.

## IDBD Explorer

The demo walks through one IDBD update, then runs IDBD and LMS side by side on a shared stream,
with Sutton's task as the default and clearly labeled extensions. Every learning value it shows
comes from the Rust crate compiled to WebAssembly.

```sh
just web-serve   # build crates/idbd-wasm, then serve http://127.0.0.1:8173
just web-setup   # once: install Playwright and Chromium into web/node_modules
just web-test    # rebuild, then run the Playwright tests
```

The build needs the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` 0.2.113 (the version
pinned in `crates/idbd-wasm`) and Node.js. The app is static: after `just web-build`, any file
server that sends `.wasm` as `application/wasm` can host `web/`.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps
scripts/coverage.sh                      # coverage and CRAP gates
cargo mutants -p idbd                    # mutation testing of the core
```

The lint policy follows [BeautifulRust](https://github.com/morganlinton/BeautifulRust). Core
scenario tests carry their IDs from `features/idbd.feature`, for example
`c07_use_old_traces_and_one_shared_pre_update_error`. Numerical fixtures come from an
independent 50-digit decimal implementation in
[`reference.py`](crates/idbd/tests/scenarios/fixtures/reference.py).
[docs/acceptance.md](docs/acceptance.md) maps every scenario to its test or recorded result and
lists what remains open.

## Reference

Richard S. Sutton. *Adapting Bias by Gradient Descent: An Incremental Version of
Delta-Bar-Delta.* In Proceedings of the Tenth National Conference on Artificial Intelligence
(AAAI-92), pages 171-176, 1992. <https://cdn.aaai.org/AAAI/1992/AAAI92-027.pdf>

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
