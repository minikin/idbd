# Reproducing the recorded results

Every number under `docs/results/` and in `docs/benchmarks.md` was produced by a binary in
`crates/idbd-lab`. Nothing in those files is typed in by hand. Each binary rewrites its whole
output directory. A rerun of the experiments on the same build replaces the files with
identical content. The timings in `docs/benchmarks.md` change from run to run.

## Commands

Run all seven from the repository root. They print progress to standard error and write their
results to the directory shown.

```sh
cargo run --release -p idbd-lab --bin sutton1992-exp1   # docs/results/sutton1992-experiment1/
cargo run --release -p idbd-lab --bin evaluate          # docs/results/evaluation/
cargo run --release -p idbd-lab --bin sutton1992-exp2   # docs/results/sutton1992-experiment2/
cargo run --release -p idbd-lab --bin autostep-eval     # docs/results/autostep/
cargo run --release -p idbd-lab --bin house-eval        # docs/results/house/
cargo run --release -p idbd-lab --bin family-eval       # docs/results/family/
cargo run --release -p idbd-lab --bin bench             # docs/benchmarks.md
```

Each accepts `--out <dir>` to write somewhere else, and `--help` to print its default. `bench`
refuses to run in a debug build.

Measured with `/usr/bin/time -p` on the release binaries, an Apple M4 Max runs
`sutton1992-exp1` in about 3 seconds, `evaluate` in 3, `sutton1992-exp2` in 2,
`autostep-eval` in 6, `house-eval` in 1, `family-eval` in under 1 and `bench` in 14.

## Software

| item | value |
| --- | --- |
| Crates | `idbd` 0.1.0, `idbd-lab` 0.1.0 (unpublished) |
| Edition | 2024, minimum supported Rust 1.85 |
| Toolchain used for the recorded numbers | rustc 1.97.1 (8bab26f4f 2026-07-14), aarch64-apple-darwin |
| Profile | `release`, with the workspace setting `debug = "line-tables-only"` |
| `RUSTFLAGS` | unset |

Results are deterministic for a given build: the same binary on the same target produces the
same bytes, apart from the timings in `docs/benchmarks.md`. Across targets, floating-point
results can differ in the last bits, so treat cross-platform agreement as numerical, not
bitwise.

## Random generator

```text
xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)
```

This string is `idbd_lab::rng::GENERATOR` and appears in every report and plot. `Rng::new(seed)`
expands the seed through SplitMix64 into the four words of `xoshiro256**` state. Standard normals
come from a Box-Muller pair, the second value cached for the next call. Changing any of that
changes every recorded result and requires a new generator version string.

The first three `next_u64` outputs for seed 0 are pinned by a unit test in `rng.rs`, checked
against a Python transcription of the reference C code.

## Task

The four `world.rs` presets use 20 inputs drawn independently from a standard normal, with no
target noise. The target is the dot product of the inputs with the current true coefficients.

The Sutton 1992 preset gives the first five coefficients +1 or -1, drawn from the seed, and
leaves the other fifteen at zero. Before sample `t`, if `t > 1` and `(t - 1) % 20 == 0`, one of
the five relevant coefficients is chosen uniformly at random and its sign is flipped. The flips
therefore apply before samples 21, 41, 61 and so on, never before sample 1. `World::next_sample`
applies pending changes first and then draws the inputs, so a flip and the sample it precedes
are never reordered.

A seed fixes three things at once: the input stream, the five initial signs, and which
coefficient flips at each boundary. World events and inputs are drawn from two separate streams
(`seed` and `seed ^ 0x5745_4F52_4C44_0001`), so a user-triggered event cannot shift the inputs.

The other presets are `stationary` (no changes at all), `drifting` (relevant coefficients follow
independent AR(1) processes, autocorrelation 0.999, unit stationary variance) and
`changing-relevance` (Sutton's flips plus a move of the relevant group every 5000 samples). The
last two are extensions, not from the paper.

The thermostat house in `house.rs` is a different task and does not follow that description. It
has 6 sensors with independent standard normal readings, of which the first 3 move the room
temperature, and the target carries Gaussian noise with standard deviation 0.5, so the noise
alone gives an MSE of 0.25. Each relevant sensor switches between a strong and a weak influence
with probability 1/60 before every reading after the first. Task `still-house` is the same house
with switching turned off. `house-eval` runs both.

The Family Assistant home in `family.rs` is another task. It has 12 sensors whose readings are
independent standard normals times each sensor's own scale (0.5, 1 or 2). Four of them add to the
power draw during fixed hours, and the target carries Gaussian noise with standard deviation 0.5.
One reading arrives per simulated minute for 3 days, and the error is averaged from the second
day. `family-eval` selects the LMS step size and the IDBD meta step size on the tuning seeds and
fails if they differ from the constants the story page uses.

## Bounds

Every run recorded here uses `Bounds::none()`. The beta floor, the beta ceiling and the
per-update increment cap are all unset. Sutton's reported experiments ran without the safety bounds, and the paper
reproductions must match that. `docs/benchmarks.md` is the exception, and only because it
measures both settings side by side.

## Parameter grids and seeds

`sutton1992-exp1`, 20000 warm-up and 10000 measured samples, seeds 1 to 20, initial step size
0.05:

- LMS alpha: 0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.10
- IDBD theta: 0, 0.0005, 0.001, 0.002, 0.004, 0.006, 0.008, 0.010, 0.012, 0.015, 0.020

`evaluate`, 20000 warm-up and 10000 measured samples on each of the four tasks, initial step
size 0.05. Tuning runs on seeds 1001 to 1005, evaluation on seeds 1 to 10:

- LMS alpha: 0.001, 0.002, 0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.10
- IDBD theta: 0, 0.0005, 0.001, 0.002, 0.004, 0.006, 0.008, 0.010, 0.015

`sutton1992-exp2`, 250000 samples with step sizes recorded every 1000, initial step size 0.05,
seeds 1 to 5. Two configurations: theta 0.001, which is the paper's, and theta 0.01, which is an
adapted demonstration and labeled as such wherever it appears. The Figure 5 comparison runs LMS
over the five relevant inputs alone on seeds 1 to 20, with 20000 warm-up and 10000 measured
samples, over alpha 0.05, 0.08, 0.10, 0.11, 0.12, 0.13, 0.14, 0.15, 0.17, 0.20, 0.25.

`bench` draws its fixture from seed 20241102 and measures 20, 1000 and 10000 inputs under both
`Bounds::none()` and `Bounds::sutton_safety()`.

## Measurement

The reported error is always the mean of the squared pre-update error over the measured samples
only. Warm-up samples are learned from but not scored. A learner that fails numerically stops
there, keeps no error value, and stays visible in the output as a failure with its sample index
and the error message.

Summaries are the mean over seeds and the standard error of that mean, using the Bessel-corrected
sample standard deviation. A parameter value with any failed seed is never selectable as a best
setting, because a mean over the surviving seeds would hide the risk.

## Outputs

| directory | contents |
| --- | --- |
| `docs/results/sutton1992-experiment1/` | `raw.csv`, `summary.csv`, `figure3.svg`, `README.md` |
| `docs/results/evaluation/` | `raw.csv`, `summary.csv`, `README.md` |
| `docs/results/sutton1992-experiment2/` | `trajectories.csv`, `final-step-sizes.csv`, `figure5-sweep.csv`, `figure4.svg`, `adapted-demonstration.svg`, `figure5.svg`, `README.md` |
| `docs/results/autostep/` | `raw.csv`, `summary.csv`, `step-sizes.csv`, `README.md` |
| `docs/results/house/` | `raw.csv`, `summary.csv`, `step-sizes.csv`, `README.md` |
| `docs/` | `benchmarks.md`, this file |

The plots are hand-generated SVG with no plotting dependency. They carry an explicit white
background and state the generator and crate version in the figure itself.

## Tests

The statistics, CSV, SVG and grid-selection helpers have unit tests that run in milliseconds:

```sh
cargo test -p idbd-lab
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The long runs live only in the binaries, so `cargo test` stays fast.
