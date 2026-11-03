# IDBD against a tuned LMS baseline

Scenario E01. Produced by `cargo run --release -p idbd-lab --bin evaluate`.

idbd-lab 0.1.0, generator xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)

## Method

Four seeded tasks, each with 20 independent standard normal inputs and a noise-free target. Two learners: fixed-step LMS, and IDBD starting every input at step size 0.05. Both run with beta bounds disabled (`Bounds::none()`) and zero initial weights and traces.

- Tuning: every grid value runs on seeds 1001..=1005. The selected value is the one with the lowest mean MSE among grid values whose every tuning run completed.
- Evaluation: the selected value runs on seeds 1..=10, which no tuning run used. Both learners see the identical stream for a given seed, because the task is generated from the seed alone.
- Each run processes 20000 warm-up samples, then 10000 measured samples. The reported error averages the squared pre-update error over the measured samples only.
- LMS alpha grid: 0.001, 0.002, 0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.1.
- IDBD theta grid: 0, 0.0005, 0.001, 0.002, 0.004, 0.006, 0.008, 0.01, 0.015.

A failed run is kept in the tables and in `raw.csv` with the sample index and the learner's message. It is never replaced by a substitute value, and a grid value with any failure cannot be selected during tuning.

## Evaluation results

Measured on the evaluation seeds, after tuning:

| task | LMS alpha | LMS MSE | IDBD theta | IDBD MSE | LMS minus IDBD, paired by seed |
| --- | --- | --- | --- | --- | --- |
| `sutton1992` | 0.03 | 3.422 +/- 0.02089 | 0.015 | 1.516 +/- 0.01034 | 1.905 +/- 0.01964 |
| `stationary` | 0.06 | 2.116e-39 +/- 5.542e-40 | 0.01 | 9.611e-40 +/- 2.702e-40 | 1.155e-39 +/- 6.661e-40 |
| `drifting` | 0.04 | 0.219 +/- 0.002174 | 0.015 | 0.110 +/- 8.361e-4 | 0.1089 +/- 0.001519 |
| `changing-relevance` | 0.03 | 3.428 +/- 0.02711 | 0.015 | 2.519 +/- 0.02381 | 0.9088 +/- 0.01862 |

A positive difference means IDBD had the lower error on that task. The sign and the size belong to this task, this protocol and these grids, and nothing here establishes an ordering that holds in general.

## Task `sutton1992`

Sutton (1992), Experiment 1. The first 5 inputs are relevant with coefficients +1 or -1 drawn from the seed. One of them, chosen uniformly at random, flips sign before samples 21, 41, 61 and so on, every 20 samples. The other 15 inputs never affect the target.

Tuning, mean MSE over the tuning seeds:

LMS (alpha):

| alpha | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0.001 | 4.896 | 0.03671 | 0 |
| 0.002 | 4.774 | 0.03882 | 0 |
| 0.005 | 4.440 | 0.04014 | 0 |
| 0.01 | 4.023 | 0.03848 | 0 |
| 0.02 | 3.557 | 0.03952 | 0 |
| 0.03 | 3.380 | 0.03996 | 0 |
| 0.04 | 3.414 | 0.04015 | 0 |
| 0.05 | 3.673 | 0.04144 | 0 |
| 0.06 | 4.280 | 0.04713 | 0 |
| 0.08 | 9.785 | 0.1997 | 0 |
| 0.1 | 1.328e177 | inf | 0 |

IDBD (theta):

| theta | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0 | 3.673 | 0.04144 | 0 |
| 0.0005 | 2.376 | 0.01661 | 0 |
| 0.001 | 2.070 | 0.01266 | 0 |
| 0.002 | 1.846 | 0.01052 | 0 |
| 0.004 | 1.682 | 0.009414 | 0 |
| 0.006 | 1.612 | 0.009216 | 0 |
| 0.008 | 1.575 | 0.009288 | 0 |
| 0.01 | 1.552 | 0.009444 | 0 |
| 0.015 | 1.523 | 0.009943 | 0 |

Evaluation, one row per seed:

| seed | LMS MSE | IDBD MSE |
| --- | --- | --- |
| 1 | 3.361 | 1.537 |
| 2 | 3.322 | 1.467 |
| 3 | 3.438 | 1.523 |
| 4 | 3.517 | 1.527 |
| 5 | 3.403 | 1.557 |
| 6 | 3.392 | 1.479 |
| 7 | 3.391 | 1.536 |
| 8 | 3.504 | 1.559 |
| 9 | 3.390 | 1.487 |
| 10 | 3.501 | 1.493 |
| mean +/- SE | 3.422 +/- 0.02089 | 1.516 +/- 0.01034 |

Failed evaluation runs: LMS 0, IDBD 0.

## Task `stationary`

The first 5 coefficients are +1 or -1 drawn from the seed and never change. Nothing in the task is nonstationary, so a small fixed step size can keep shrinking its error.

Tuning, mean MSE over the tuning seeds:

LMS (alpha):

| alpha | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0.001 | 1.517e-18 | 2.686e-19 | 0 |
| 0.002 | 1.329e-29 | 2.540e-30 | 0 |
| 0.005 | 1.505e-30 | 4.382e-31 | 0 |
| 0.01 | 4.308e-31 | 4.380e-32 | 0 |
| 0.02 | 4.146e-32 | 2.825e-32 | 0 |
| 0.03 | 2.356e-39 | 1.079e-39 | 0 |
| 0.04 | 2.344e-39 | 6.890e-40 | 0 |
| 0.05 | 1.504e-39 | 4.099e-40 | 0 |
| 0.06 | 9.225e-40 | 2.617e-40 | 0 |
| 0.08 | 1.603e-30 | 7.311e-32 | 0 |
| 0.1 | 3.449e178 | inf | 0 |

IDBD (theta):

| theta | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0 | 1.818e-39 | 4.217e-40 | 0 |
| 0.0005 | 1.395e-39 | 4.546e-40 | 0 |
| 0.001 | 1.719e-39 | 4.705e-40 | 0 |
| 0.002 | 1.184e-39 | 2.139e-40 | 0 |
| 0.004 | 1.485e-39 | 4.547e-40 | 0 |
| 0.006 | 1.287e-39 | 2.888e-40 | 0 |
| 0.008 | 1.885e-39 | 5.485e-40 | 0 |
| 0.01 | 9.221e-40 | 1.769e-40 | 0 |
| 0.015 | 1.050e-39 | 1.425e-40 | 0 |

Evaluation, one row per seed:

| seed | LMS MSE | IDBD MSE |
| --- | --- | --- |
| 1 | 7.644e-40 | 1.020e-39 |
| 2 | 2.165e-39 | 3.221e-39 |
| 3 | 9.363e-40 | 5.132e-40 |
| 4 | 7.112e-40 | 1.326e-39 |
| 5 | 2.805e-39 | 5.090e-40 |
| 6 | 2.725e-39 | 6.445e-40 |
| 7 | 3.851e-39 | 5.172e-40 |
| 8 | 9.187e-40 | 4.164e-40 |
| 9 | 3.950e-40 | 1.049e-39 |
| 10 | 5.885e-39 | 3.950e-40 |
| mean +/- SE | 2.116e-39 +/- 5.542e-40 | 9.611e-40 +/- 2.702e-40 |

Failed evaluation runs: LMS 0, IDBD 0.

## Task `drifting`

An extension, not from the paper. The 5 relevant coefficients follow independent AR(1) processes with autocorrelation 0.999 and unit stationary variance, so the target relationship moves continuously instead of jumping.

Tuning, mean MSE over the tuning seeds:

LMS (alpha):

| alpha | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0.001 | 2.484 | 0.1914 | 0 |
| 0.002 | 1.675 | 0.1077 | 0 |
| 0.005 | 0.8616 | 0.03418 | 0 |
| 0.01 | 0.5015 | 0.01056 | 0 |
| 0.02 | 0.3016 | 0.005032 | 0 |
| 0.03 | 0.2382 | 0.004323 | 0 |
| 0.04 | 0.2157 | 0.004194 | 0 |
| 0.05 | 0.2162 | 0.004512 | 0 |
| 0.06 | 0.2401 | 0.005494 | 0 |
| 0.08 | 0.5118 | 0.01668 | 0 |
| 0.1 | 5.324e179 | inf | 0 |

IDBD (theta):

| theta | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0 | 0.2162 | 0.004512 | 0 |
| 0.0005 | 0.1994 | 0.003835 | 0 |
| 0.001 | 0.1861 | 0.003328 | 0 |
| 0.002 | 0.1666 | 0.002623 | 0 |
| 0.004 | 0.1435 | 0.001842 | 0 |
| 0.006 | 0.1306 | 0.001439 | 0 |
| 0.008 | 0.1226 | 0.001207 | 0 |
| 0.01 | 0.1171 | 0.001061 | 0 |
| 0.015 | 0.1084 | 8.650e-4 | 0 |

Evaluation, one row per seed:

| seed | LMS MSE | IDBD MSE |
| --- | --- | --- |
| 1 | 0.2174 | 0.1092 |
| 2 | 0.2287 | 0.112 |
| 3 | 0.2173 | 0.1073 |
| 4 | 0.2161 | 0.1081 |
| 5 | 0.221 | 0.1125 |
| 6 | 0.2122 | 0.1094 |
| 7 | 0.2259 | 0.1141 |
| 8 | 0.2187 | 0.1095 |
| 9 | 0.2262 | 0.1125 |
| 10 | 0.206 | 0.1058 |
| mean +/- SE | 0.219 +/- 0.002174 | 0.110 +/- 8.361e-4 |

Failed evaluation runs: LMS 0, IDBD 0.

## Task `changing-relevance`

An extension, not from the paper. Sutton's sign flips every 20 samples, plus every 5000 samples the relevant group moves to the next 5 inputs. The previously relevant coefficients become zero and the new group gets fresh signs. Over the measured interval this happens twice.

Tuning, mean MSE over the tuning seeds:

LMS (alpha):

| alpha | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0.001 | 4.846 | 0.0134 | 0 |
| 0.002 | 4.729 | 0.01203 | 0 |
| 0.005 | 4.421 | 0.01788 | 0 |
| 0.01 | 4.032 | 0.02264 | 0 |
| 0.02 | 3.580 | 0.02754 | 0 |
| 0.03 | 3.402 | 0.0285 | 0 |
| 0.04 | 3.434 | 0.03001 | 0 |
| 0.05 | 3.696 | 0.03529 | 0 |
| 0.06 | 4.317 | 0.04856 | 0 |
| 0.08 | 10.000 | 0.235 | 0 |
| 0.1 | 1.328e177 | inf | 0 |

IDBD (theta):

| theta | mean MSE | standard error | failed |
| --- | --- | --- | --- |
| 0 | 3.696 | 0.03529 | 0 |
| 0.0005 | 3.391 | 0.02812 | 0 |
| 0.001 | 3.330 | 0.02708 | 0 |
| 0.002 | 3.288 | 0.0248 | 0 |
| 0.004 | 3.154 | 0.01896 | 0 |
| 0.006 | 2.980 | 0.01573 | 0 |
| 0.008 | 2.827 | 0.01503 | 0 |
| 0.01 | 2.707 | 0.01574 | 0 |
| 0.015 | 2.518 | 0.01896 | 0 |

Evaluation, one row per seed:

| seed | LMS MSE | IDBD MSE |
| --- | --- | --- |
| 1 | 3.346 | 2.534 |
| 2 | 3.347 | 2.472 |
| 3 | 3.472 | 2.505 |
| 4 | 3.334 | 2.468 |
| 5 | 3.462 | 2.580 |
| 6 | 3.316 | 2.383 |
| 7 | 3.497 | 2.646 |
| 8 | 3.531 | 2.579 |
| 9 | 3.434 | 2.470 |
| 10 | 3.541 | 2.555 |
| mean +/- SE | 3.428 +/- 0.02711 | 2.519 +/- 0.02381 |

Failed evaluation runs: LMS 0, IDBD 0.

## What these numbers do and do not say

Each row measures one learner, tuned once on five streams, on ten other streams of the same task. The comparison is paired: for a given seed both learners consume the same inputs and targets in the same order.

The measured interval starts after 20000 warm-up samples, so the initial transient is excluded on every task. A learner that recovers slowly from a change but ends in a good place is treated the same as one that never leaves it.

LMS is tuned to one step size for the whole stream. On a task whose difficulty changes within a run, that single value is a compromise, and the comparison measures that compromise rather than LMS in general. Giving LMS a schedule, or tuning it per interval, would be a different experiment.

`raw.csv` holds every run, including the failures. `summary.csv` holds the per-grid-value aggregates for both phases.

On `stationary` both learners drive the measured error below 1e-12. Both have fit the task, the remaining digits are floating-point rounding, and the difference between them there measures nothing.
