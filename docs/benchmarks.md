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

One configuration is one input size, one bounds setting and one operation. It runs 3 untimed batches and then 101 timed batches. A batch calls the operation once per example over a preallocated pool, and the batch's elapsed time divided by its call count is one sample. The tables report the median of those samples and the 10th and 90th percentiles.

Every batch starts from a freshly constructed learner, so an update batch always begins at zero weights and traces. A batch never reuses an example, which keeps the learner from fitting the pool exactly and driving its residuals into the subnormal range, where arithmetic can take a different amount of time.

The fixture draws each input from a standard normal scaled by `1/sqrt(n)`, so the squared input norm stays near one at every size and one step size (0.05 with meta step size 0.01) is stable for all of them. The target is the sum of the first 5 inputs with random signs. The fixture is drawn from seed 20241102, so the same build measures the same work.

Construction, fixture generation and priming are outside every timed region. Allocation behaviour is covered by scenario C23, not here.

## Results

| features | bounds | operation | median ns/call | p10 | p90 | ns per feature | calls per batch |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20 | none | predict | 9.41 | 9.39 | 9.75 | 0.470 | 4096 |
| 20 | none | update | 96.98 | 95.67 | 98.07 | 4.849 | 4096 |
| 20 | sutton safety | predict | 8.65 | 8.63 | 8.66 | 0.432 | 4096 |
| 20 | sutton safety | update | 102.4 | 102.3 | 103.8 | 5.118 | 4096 |
| 1000 | none | predict | 833.9 | 832.4 | 845.5 | 0.834 | 2000 |
| 1000 | none | update | 4739.0 | 4711.7 | 4772.1 | 4.739 | 2000 |
| 1000 | sutton safety | predict | 837.9 | 832.7 | 850.2 | 0.838 | 2000 |
| 1000 | sutton safety | update | 5100.4 | 5069.9 | 5139.4 | 5.100 | 2000 |
| 10000 | none | predict | 8366.2 | 8291.5 | 8586.2 | 0.837 | 200 |
| 10000 | none | update | 52261 | 51975 | 54748 | 5.226 | 200 |
| 10000 | sutton safety | predict | 8422.5 | 8407.7 | 8457.3 | 0.842 | 200 |
| 10000 | sutton safety | update | 55661 | 55501 | 55826 | 5.566 | 200 |

The spread is batch-to-batch variation on a machine running other work, not a confidence interval for the operation.

## What is not in these numbers

These are native timings of the Rust core on one machine. They do not include the WebAssembly binding, the JavaScript call overhead, data marshalling across that boundary, or any browser rendering. The demo's cost is a separate measurement and is not reported here.

Dense `f64` inputs only. A sparse representation would change both the per-call and the per-feature cost.
