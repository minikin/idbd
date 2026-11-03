# The thermostat house

Scenario E06. Produced by `cargo run --release -p idbd-lab --bin house-eval`.

idbd-lab 0.1.0, generator xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)

## Method

The house in `idbd_lab::house` has 6 sensors with independent standard normal readings. The target is the change in room temperature: the readings' dot product with the current influences, plus Gaussian noise with standard deviation 0.5. Only the first 3 sensors have an influence (sun on window 1.2 or 0.2, oven 0.9 or 0.1, front door -1.1 or -0.1). Before every reading after the first, each of them switches between its two levels with probability 0.01667. The other 3 sensors never matter. Task `still-house` is the same house with switching turned off, so every relevant sensor keeps its first influence.

The protocol is that of [scenario E01](../evaluation/README.md): 20000 warm-up samples, then the squared pre-update error averaged over 10000 samples. The LMS step size and the IDBD meta step size theta are selected on seeds 1001..=1005 from the E01 grids and reported on seeds 1..=10. IDBD starts every input at step size 0.05 with no beta bounds. Autostep starts at 0.0167 (0.1 / n) with mu = 0.01 and tau = 10000, the paper's defaults, and is not tuned.

## Results

Mean MSE +/- standard error over the evaluation seeds. The selected parameter is in parentheses. The noise alone gives an MSE of 0.25.

| task | LMS | IDBD | Autostep, default |
| --- | --- | --- | --- |
| `house` | 0.6295 +/- 0.002847 (0.06) | 0.5431 +/- 0.003719 (0.015) | 0.5867 +/- 0.00366 |
| `still-house` | 0.252 +/- 0.001184 (0.001) | 0.269 +/- 0.001194 (0.015) | 0.2617 +/- 0.001219 |

Paired by seed. A negative difference means the first learner had the lower error. The last column counts the evaluation seeds on which IDBD's error was below LMS's.

| task | IDBD minus LMS | Autostep minus LMS | IDBD lower on |
| --- | --- | --- | --- |
| `house` | -0.0864 +/- 0.001153 | -0.04283 +/- 0.003247 | 10 of 10 seeds |
| `still-house` | 0.01696 +/- 1.563e-4 | 0.009638 +/- 1.231e-4 | 0 of 10 seeds |

## IDBD's step size per sensor

IDBD at the selected theta = 0.015, after 30000 samples of task `house`, one run per evaluation seed. Per-seed values are in `step-sizes.csv`.

| sensor | relevant | mean | min to max |
| --- | --- | --- | --- |
| sun on window | yes | 0.1189 | 0.1015 to 0.132 |
| oven | yes | 0.08883 | 0.07857 to 0.09696 |
| front door | yes | 0.1169 | 0.1007 to 0.137 |
| wi-fi signal | no | 0.01168 | 0.011 to 0.01251 |
| doorbell battery | no | 0.01161 | 0.0111 to 0.01214 |
| clock | no | 0.01195 | 0.01132 to 0.01314 |

## Reading these results

The house is a synthetic task built for the story page, not data from a real thermostat. Each number belongs to this protocol, these grids and this task. Nothing here establishes that one learner is better in general.
