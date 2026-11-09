# idbd

<p align="center">
  <a href="https://github.com/minikin/idbd/actions/workflows/ci.yml"><img src="https://github.com/minikin/idbd/raw/badges/ci.svg" alt="CI"></a>
  <a href="https://github.com/minikin/idbd/actions/workflows/ci.yml"><img src="https://github.com/minikin/idbd/raw/badges/coverage.svg" alt="coverage"></a>
  <a href="scripts/coverage.sh"><img src="https://github.com/minikin/idbd/raw/badges/crap.svg" alt="CRAP score"></a>
  <a href="crates/idbd/Cargo.toml"><img src="https://github.com/minikin/idbd/raw/badges/msrv.svg" alt="minimum Rust version"></a>
  <a href="#license"><img src="https://github.com/minikin/idbd/raw/badges/license.svg" alt="license"></a>
</p>

IDBD (Sutton, 1992) is the LMS rule with one learning rate per input. It keeps
`beta_i = ln(alpha_i)` for every input and moves it by gradient descent on the squared error,
so an input whose corrections keep pointing the same way gets a larger step and an input whose
corrections cancel out gets a smaller one. No input is marked as relevant in advance.

<p align="center">
  <img src="docs/assets/demo-loop.webp" width="100%" alt="The story page's guessing game on a 24-hour dial with one ring per sensor: a visitor's marked hours in violet, IDBD's bars forming in amber over three simulated days, then the scores, the visitor 21% and IDBD 85%">
</p>
<p align="center"><sub>One run of the story page's guessing game. A tuned fixed step on rescaled readings scores 89% to IDBD's 85% over ten runs, see <a href="#results">Results</a>.</sub></p>

<p>
  <a href="https://minikin.me/blog/idbd-in-rust/"><picture><source media="(prefers-color-scheme: dark)" srcset="https://github.com/minikin/idbd/raw/HEAD/docs/assets/link-blog-idbd-in-rust-dark.svg?sanitize=true"><img alt="Blog post: IDBD in Rust: A Learning Rate for Every Input" src="docs/assets/link-blog-idbd-in-rust.svg"></picture></a>
  <a href="https://idbd.minikin.me"><picture><source media="(prefers-color-scheme: dark)" srcset="https://github.com/minikin/idbd/raw/HEAD/docs/assets/link-demo-dark.svg?sanitize=true"><img alt="Live demo: idbd.minikin.me" src="docs/assets/link-demo.svg"></picture></a>
</p>

This repository has three parts:

- `idbd`, the crate: IDBD and Autostep (Mahmood et al., 2012) as checked, atomic,
  allocation-free online learners. No dependencies, no `unsafe`, Rust 1.85 or newer.
- `idbd-lab`: an approximate reproduction of the paper's two experiments, plus new tasks, with
  every result below produced from recorded seeds.
- A browser demo that runs the same Rust through WebAssembly: a story page for people who have
  never trained a model, and an Explorer for those who have.

| Path | Contents |
| --- | --- |
| [`crates/idbd`](crates/idbd) | The library. |
| [`crates/idbd-lab`](crates/idbd-lab) | Unpublished. Seeded tasks, an LMS baseline, experiment and benchmark binaries. |
| [`crates/idbd-wasm`](crates/idbd-wasm) | Unpublished. The `wasm-bindgen` adapter used by the demo. |
| [`web`](web) | The story page and the Explorer, static, with Playwright tests. |
| [`docs`](docs) | Reproduction instructions, generated results and benchmarks. |
| [`features/idbd.feature`](features/idbd.feature) | The behavior specification: 62 scenarios in 13 rules. |

## Using the crate

```toml
[dependencies]
idbd = "0.1"
```

```rust
use idbd::{Config, Idbd};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initial step size 0.05, meta step size theta 0.01.
    let mut learner = Idbd::new(3, Config::new(0.05, 0.01)?)?;
    let report = learner.update(&[0.5, -1.0, 1.0], 2.0)?;
    println!("error before learning: {}", report.error);
    Ok(())
}
```

`update` predicts, measures the error, adapts the step sizes and then the weights, in Sutton's
order. A failed update returns a typed error and leaves the learner bit for bit as it was. The
learning path does not allocate.

[`examples/latency.rs`](crates/idbd/examples/latency.rs) is a complete streaming application:
input and target scaling, predicting before learning, skipping an invalid sample, and
checkpointing the learner with its stream position. Run it with
`cargo run -p idbd --example latency`. The [crate README](crates/idbd/README.md) covers the
update equations, configuration, bounds, checkpoints, when to use IDBD and its limitations.

## The demo

The story page explains IDBD with Family Assistant, a home with twelve sensors and a power
meter. Four of the sensors add to the meter, each during its own hours of the day, and the
sensors read on different scales. The page shows the live readings, plots one fixed learning
speed that is late on small readings and nervous on large ones, walks through one update in slow
motion, and then lets you play: you mark the hours in which you think each sensor adds to the
meter, and IDBD and a tuned fixed-speed learner watch three simulated days and draw their own
answer. Every learning value on the page comes from the crate. The reading scales are where
IDBD's edge comes from. The page says so, and that a fixed speed on rescaled readings does as
well.

The Explorer runs IDBD, LMS and untuned Autostep side by side on Sutton's 20-input task, with
replay, export and a guided single update.

```sh
just web-serve   # build, then serve the story at http://127.0.0.1:8173 and /explorer.html
```

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

At theta 0.02, 5 of 20 IDBD runs failed with a step-size overflow. LMS at alpha 0.09 reached an
MSE of 129.7, off the plot, and diverged at alpha 0.1.
[Details and plot](docs/results/sutton1992-experiment1/README.md).

**Experiment 2 (Figures 4 and 5).** theta 0.001, 250,000 samples, 5 seeds. The relevant inputs'
step sizes end at 0.129 on average (paper: 0.13 +/- 0.015). The irrelevant inputs end between
0.00726 and 0.00783, slightly above the paper's "all below 0.007". A fixed-rate sweep over the
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

**Autostep (new tasks, same protocol).** Autostep (Mahmood et al., 2012) at the paper's
default mu = 0.01, with no tuning, against the tuned learners above:

| Task | Autostep, default mu | Autostep, mu tuned |
| --- | --- | --- |
| Sutton 1992 task | 2.267 +/- 0.016 | 2.052 +/- 0.027 (mu 0.1) |
| Drifting coefficients (extension) | 0.185 +/- 0.004 | 0.132 +/- 0.003 (mu 0.1) |
| Changing relevance (extension) | 4.022 +/- 0.028 | 3.448 +/- 0.032 (mu 0.03) |
| Stationary | below 1e-30 | below 1e-32 |

On these tasks untuned Autostep beat tuned LMS on two of three nonstationary tasks and trailed
tuned IDBD on all of them. At mu = 1, 19 of 20 tuning runs stopped when a step size underflowed
to zero. [Method, mu sweep and step sizes](docs/results/autostep/README.md).

**The Family Assistant home (new task).** Twelve sensors on their own reading scales, four of
which add to the power draw during fixed hours of the day, one reading a minute for three days.
This is the task behind the story page's game. The score is the overlap of the hours a
learner's weights mark with the true hours, pooled over all sensors.

| Learner | Mean squared error | Bar score | Overlap on the 4 sensors that matter |
| --- | --- | --- | --- |
| LMS, alpha 0.01 (tuned) | 0.514 +/- 0.004 | 0.38 | 0.43 |
| IDBD, theta 0.07 (tuned) | 0.384 +/- 0.003 | 0.85 | 0.88 |
| LMS on rescaled readings, alpha 0.03 (tuned) | 0.411 +/- 0.003 | 0.89 | 0.89 |

IDBD found all four sensors on every evaluation seed and marked Wi-Fi data use an hour late.
The last row divides every reading by its sensor's scale first, as this README recommends. With
the scales evened out, the tuned fixed step draws bars as good as IDBD's, so on this home IDBD's
edge comes from the different reading scales.
[Method, sensors and every bar](docs/results/family/README.md).

**Performance.** On an Apple M4 Max, one IDBD update with 20 features takes about 95 ns and costs
about 5 ns per feature up to 10,000 features. A standalone loop of one `exp` call per input takes
85 to 97% as long as the update, a reference cost, not a measured split. An Autostep update costs
2.37 to 2.69 times an IDBD update. [Method and full table](docs/benchmarks.md).

## Reproducing the results

```sh
cargo run --release -p idbd-lab --bin sutton1992-exp1   # Experiment 1, about 3 s (see docs/reproduction.md)
cargo run --release -p idbd-lab --bin sutton1992-exp2   # Experiment 2 and the Figure 5 sweep
cargo run --release -p idbd-lab --bin evaluate          # IDBD against LMS on four tasks
cargo run --release -p idbd-lab --bin autostep-eval     # untuned Autostep against both
cargo run --release -p idbd-lab --bin family-eval       # the Family Assistant home
cargo run --release -p idbd-lab --bin bench             # core timings
```

Each binary writes CSV, SVG and Markdown under `docs/results` (or `--out <dir>`). Reruns on the
same build and platform reproduce the files byte for byte, except the timings in
`docs/benchmarks.md`. [docs/reproduction.md](docs/reproduction.md)
lists the generator, seeds, grids and sign-flip timing.

## Building the demo

```sh
just web-setup   # once: install Playwright with Chromium, Firefox and WebKit
just web-test    # rebuild, then run the Playwright tests in all three browsers
just readme-assets   # re-render the README images from the demo
```

The build needs the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` 0.2.113 (the version
pinned in `crates/idbd-wasm`) and Node.js. The house is drawn with three.js 0.170.0, pinned in
`web/package.json`, and in 2D when WebGL is missing or the visitor prefers reduced motion. The
site is static: after `just web-build`, any file server that sends `.wasm` as
`application/wasm` can host `web/`.

`just web-site` copies only `index.html`, `explorer.html`, `styles.css`, `src/`, `story/`,
three.js from `vendor/` and the two files the pages load from `pkg/` into `_site/`.
`just web-deploy` deploys `_site/` to Cloudflare as the assets-only Worker in `wrangler.jsonc`,
after `wrangler login` or with `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` set. The `Demo`
workflow does the same on any manual run, and on every push to `main` once the repository
variable `DEPLOY_DEMO` is `true`. It needs both values as repository secrets.

## Reference

Richard S. Sutton. *Adapting Bias by Gradient Descent: An Incremental Version of
Delta-Bar-Delta.* In Proceedings of the Tenth National Conference on Artificial Intelligence
(AAAI-92), pages 171-176, 1992. <https://cdn.aaai.org/AAAI/1992/AAAI92-027.pdf>

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
