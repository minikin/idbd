# Measured core performance

Scenario C24. Produced by `cargo run --release -p idbd-lab --bin bench`.

idbd-lab 0.1.0, generator xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)

## Machine and build

| item | value |
| --- | --- |
| CPU | Apple M4 Max |
| Logical cores | 16 |
| OS | Darwin 27.0.0 arm64 |
| Compiler | rustc 1.97.1 (8bab26f4f 2026-07-14), binary: rustc, commit-hash: 8bab26f4f68e0e26f0bb7960be334d5b520ea452, commit-date: 2026-07-14, host: aarch64-apple-darwin, release: 1.97.1, LLVM version: 22.1.6 |
| Profile | release, `debug = "line-tables-only"`, default `opt-level = 3`, `codegen-units = 16`, no LTO |
| `RUSTFLAGS` | unset |

## Method

Timing uses `std::time::Instant` and `std::hint::black_box`, with no benchmark framework.

Five untimed passes over the first configuration run before any row is recorded, so the first rows are not measured while the CPU clock is still rising. One configuration is one input size, one learner and one operation. Both learners start every input at the same step size, and Autostep uses its defaults (mu 0.01, tau 10,000). The `exp only` row calls `f64::exp` once per input of each example and nothing else. It runs 3 untimed batches and then 101 timed batches. A batch calls the operation once per example over a preallocated pool, and the batch's elapsed time divided by its call count is one sample. The tables report the median of those samples and the 10th and 90th percentiles.

Every batch starts from a freshly constructed learner, so an update batch always begins at zero weights and traces. A batch never reuses an example, which keeps the learner from fitting the pool exactly and driving its residuals into the subnormal range, where arithmetic can take a different amount of time.

The fixture draws each input from a standard normal scaled by `1/sqrt(n)`, so the squared input norm stays near one at every size and one step size (0.05 with meta step size 0.01) is stable for all of them. The target is the sum of the first 5 inputs with random signs. The fixture is drawn from seed 20241102, so the same build measures the same work.

Construction, fixture generation and priming are outside every timed region. Allocation behaviour is covered by scenario C23, not here.

## Results

| features | learner | operation | median ns/call | p10 | p90 | ns per feature | calls per batch |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20 | IDBD, no bounds | predict | 8.67 | 7.94 | 8.72 | 0.433 | 4096 |
| 20 | IDBD, no bounds | update | 94.26 | 93.32 | 97.35 | 4.713 | 4096 |
| 20 | IDBD, Sutton safety | predict | 9.41 | 8.68 | 9.56 | 0.470 | 4096 |
| 20 | IDBD, Sutton safety | update | 97.34 | 96.15 | 104.0 | 4.867 | 4096 |
| 20 | Autostep | predict | 8.68 | 8.67 | 8.73 | 0.434 | 4096 |
| 20 | Autostep | update | 223.8 | 222.4 | 228.4 | 11.192 | 4096 |
| 20 | reference | exp only | 84.04 | 73.93 | 87.97 | 4.202 | 4096 |
| 1000 | IDBD, no bounds | predict | 745.6 | 735.3 | 760.3 | 0.746 | 2000 |
| 1000 | IDBD, no bounds | update | 4520.3 | 4467.7 | 4559.0 | 4.520 | 2000 |
| 1000 | IDBD, Sutton safety | predict | 744.0 | 732.8 | 756.5 | 0.744 | 2000 |
| 1000 | IDBD, Sutton safety | update | 4766.8 | 4695.9 | 4803.0 | 4.767 | 2000 |
| 1000 | Autostep | predict | 739.8 | 729.6 | 754.2 | 0.740 | 2000 |
| 1000 | Autostep | update | 12159 | 11953 | 12445 | 12.159 | 2000 |
| 1000 | reference | exp only | 4383.5 | 4230.0 | 4464.3 | 4.383 | 2000 |
| 10000 | IDBD, no bounds | predict | 7637.9 | 7444.2 | 7776.5 | 0.764 | 200 |
| 10000 | IDBD, no bounds | update | 51505 | 50291 | 52679 | 5.151 | 200 |
| 10000 | IDBD, Sutton safety | predict | 7965.6 | 7607.1 | 8394.0 | 0.797 | 200 |
| 10000 | IDBD, Sutton safety | update | 53003 | 52461 | 54447 | 5.300 | 200 |
| 10000 | Autostep | predict | 7830.6 | 7653.3 | 8083.3 | 0.783 | 200 |
| 10000 | Autostep | update | 122184 | 120469 | 124916 | 12.218 | 200 |
| 10000 | reference | exp only | 43755 | 42559 | 44168 | 4.376 | 200 |

The spread is batch-to-batch variation on a machine running other work, not a confidence interval for the operation.

## Where an update's time goes

Both learners call `exp` at most once per input on every update. The share below is the `exp only` median divided by the median of the unbounded IDBD update at the same size. The timings overlap inside the CPU, so the share is an estimate, not an exact split. A high share leaves little room for rewriting the remaining arithmetic, for example with SIMD.

| features | exp only, ns/call | IDBD update, ns/call | exp share | Autostep update / IDBD update |
| --- | --- | --- | --- | --- |
| 20 | 84.04 | 94.26 | 89% | 2.37 |
| 1000 | 4383.5 | 4520.3 | 97% | 2.69 |
| 10000 | 43755 | 51505 | 85% | 2.37 |

## What is not in these numbers

These are native timings of the Rust core on one machine. They do not include the WebAssembly binding, the JavaScript call overhead, data marshalling across that boundary, or any browser rendering. The demo's cost is a separate measurement and is not reported here.

Dense `f64` inputs only. A sparse representation would change both the per-call and the per-feature cost.
