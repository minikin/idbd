# Autostep against tuned LMS and tuned IDBD

Scenario E05. Produced by `cargo run --release -p idbd-lab --bin autostep-eval`.

idbd-lab 0.1.0, generator xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)

## Method

The tasks, seeds, warm-up, measured window and the LMS and IDBD grids are those of [scenario E01](../evaluation/README.md): 20000 warm-up samples, then the squared pre-update error averaged over 10000 samples, with parameters selected on seeds 1001..=1005 and reported on seeds 1..=10. The LMS and IDBD numbers below come from rerunning that procedure and match E01.

Autostep (Mahmood, Sutton, Degris and Pilarski, 2012) starts every input at step size 0.005, the 0.1 / n rule the paper used on its idealized problems, with tau = 10000. It runs twice per task:

- **tuned**: mu selected from 0, 0.001, 0.003, 0.01, 0.03, 0.1, 0.3, 1 on the tuning seeds, exactly like the LMS and IDBD parameters.
- **default**: mu = 0.01, the value the paper recommends for every problem, with no tuning at all.

A failed run is kept in the tables with its sample and message, and a grid value with any failed tuning run cannot be selected.

## Evaluation results

Mean MSE +/- standard error over the evaluation seeds. The selected parameter is in parentheses.

| task | LMS | IDBD | Autostep, tuned | Autostep, default |
| --- | --- | --- | --- | --- |
| `sutton1992` | 3.422 +/- 0.02089 (0.03) | 1.516 +/- 0.01034 (0.015) | 2.052 +/- 0.02711 (0.1) | 2.267 +/- 0.01585 |
| `stationary` | 2.116e-39 +/- 5.542e-40 (0.06) | 9.611e-40 +/- 2.702e-40 (0.01) | 1.164e-33 +/- 1.159e-33 (0.1) | 6.710e-31 +/- 1.581e-31 |
| `drifting` | 0.219 +/- 0.002174 (0.04) | 0.110 +/- 8.361e-4 (0.015) | 0.1318 +/- 0.003172 (0.1) | 0.1852 +/- 0.004081 |
| `changing-relevance` | 3.428 +/- 0.02711 (0.03) | 2.519 +/- 0.02381 (0.015) | 3.448 +/- 0.03214 (0.03) | 4.022 +/- 0.02755 |

Paired by seed, Autostep at the default mu minus each tuned baseline. A negative value means Autostep had the lower error on that task.

| task | default minus LMS | default minus IDBD | default minus tuned Autostep |
| --- | --- | --- | --- |
| `sutton1992` | -1.155 +/- 0.02652 | 0.7504 +/- 0.01966 | 0.2153 +/- 0.02675 |
| `stationary` | 6.710e-31 +/- 1.581e-31 | 6.710e-31 +/- 1.581e-31 | 6.698e-31 +/- 1.584e-31 |
| `drifting` | -0.03373 +/- 0.00485 | 0.07519 +/- 0.004263 | 0.0534 +/- 0.005445 |
| `changing-relevance` | 0.5943 +/- 0.01511 | 1.503 +/- 0.02022 | 0.574 +/- 0.02899 |

## Sensitivity to mu

Mean MSE over the tuning seeds for each mu. The paper's claim is that one mu works across problems, so this table shows how much the error moves with mu on each task.

| mu | `sutton1992` | `stationary` | `drifting` | `changing-relevance` |
| --- | --- | --- | --- | --- |
| 0 | 4.440 +/- 0.04014 | 1.505e-30 +/- 4.382e-31 | 0.8616 +/- 0.03418 | 4.421 +/- 0.01788 |
| 0.001 | 4.279 +/- 0.03934 | 1.668e-30 +/- 3.427e-31 | 0.6646 +/- 0.02126 | 4.388 +/- 0.01754 |
| 0.003 | 3.850 +/- 0.03564 | 1.407e-30 +/- 2.780e-31 | 0.4256 +/- 0.01728 | 4.315 +/- 0.01652 |
| 0.01 | 2.312 +/- 0.04923 | 7.828e-31 +/- 1.558e-31 | 0.181 +/- 0.01306 | 3.985 +/- 0.01188 |
| 0.03 | 2.136 +/- 0.05918 | 2.924e-31 +/- 5.627e-32 | 0.1431 +/- 0.005508 | 3.430 +/- 0.04035 |
| 0.1 | 2.037 +/- 0.03535 | 1.102e-32 +/- 1.099e-32 | 0.1272 +/- 0.006183 | 4.068 +/- 0.04126 |
| 0.3 | 2.066 +/- 0.02689 | 7.445e-6 +/- 7.445e-6 | 0.1288 +/- 0.006908 | 4.114 +/- 0.03048 |
| 1 | no completed run, 5 failed | no completed run, 5 failed | 8.598 (1 seed, no spread), 4 failed | no completed run, 5 failed |

19 tuning runs failed, 19 of them at the step-size stage. That stage fails when a step size leaves the finite, strictly positive range. A large mu lets a step size shrink by up to exp(-mu) per update until it underflows to zero, and the crate reports that instead of continuing with a step size that could never grow again.

LMS at alpha 0.1 ran to the end of the stream without a non-finite value, so the runs are recorded as completed, but the error grew without bound: the mean is above 100 or the standard error is not finite. Treat those rows as divergence, not as a measurement.

## Step sizes on Sutton's task

Autostep at the default mu, 250000 samples of task `sutton1992`, the length of Sutton's Experiment 2. The first 5 inputs are relevant. Final step sizes per seed, in `step-sizes.csv`:

| seed | relevant: mean | relevant: min to max | irrelevant: mean | irrelevant: min to max |
| --- | --- | --- | --- | --- |
| 1 | 0.04523 | 0.03092 to 0.05224 | 1.634e-10 | 1.321e-10 to 2.012e-10 |
| 2 | 0.03727 | 0.0178 to 0.05437 | 1.142e-9 | 9.159e-10 to 1.493e-9 |
| 3 | 0.04545 | 0.03355 to 0.05913 | 1.890e-10 | 1.535e-10 to 2.252e-10 |
| 4 | 0.03482 | 0.02595 to 0.04119 | 1.941e-10 | 1.678e-10 to 2.175e-10 |
| 5 | 0.04169 | 0.02687 to 0.05246 | 4.468e-10 | 3.805e-10 to 5.261e-10 |

## Reading these results

Each number belongs to this protocol, these grids and these synthetic tasks, which have 20 standard normal inputs and a noise-free target. Autostep's initial step size and tau were not tuned. Nothing here establishes that one learner is better in general.
