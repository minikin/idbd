# idbd

Incremental Delta-Bar-Delta (IDBD) for Rust: a linear online learner that keeps one adaptive
step size per input.

IDBD (Sutton, 1992) is the LMS rule with a separate learning rate `alpha_i = exp(beta_i)` for
every input. It adjusts each `beta_i` by gradient descent on the squared prediction error. Inputs
whose recent weight updates keep pointing the same way get larger steps. Inputs whose updates
cancel out, such as pure noise, get smaller ones. On streams where only some inputs carry signal
and the relationship drifts, this lets the learner track the signal without a hand-tuned global
learning rate.

```toml
[dependencies]
idbd = "0.1"
```

## Example

```rust
use idbd::{Config, Idbd};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initial step size 0.05, meta step size (theta) 0.01, no bounds
    let config = Config::new(0.05, 0.01)?;
    // three inputs plus a constant 1.0 that learns the intercept
    let mut learner = Idbd::new(4, config)?;

    let stream = [
        ([0.4, -1.2, 0.7, 1.0], 1.9),
        ([1.1, 0.3, -0.5, 1.0], 2.6),
        ([-0.8, 0.9, 0.2, 1.0], -0.4),
    ];
    for (features, target) in &stream {
        // predict before learning, then learn from the observed target
        let report = learner.update(features, *target)?;
        println!("predicted {:.3}, error {:.3}", report.prediction, report.error);
    }

    for (i, alpha) in learner.step_sizes().enumerate() {
        println!("input {i}: step size {alpha:.4}");
    }

    // a full checkpoint: configuration, weights, step sizes and traces
    let checkpoint = learner.snapshot();
    let resumed = Idbd::from_state(checkpoint)?;
    assert_eq!(resumed.weights(), learner.weights());
    Ok(())
}
```

## The update

For features `x`, target `y*`, weights `w`, traces `h` and meta step size `theta`:

```text
prediction = sum_i(w_i * x_i)
error      = y* - prediction
for each i, using the same pre-update error:
    beta_i  = beta_i + theta * error * x_i * h_i        (uses the old trace)
    alpha_i = exp(beta_i)
    w_i     = w_i + alpha_i * error * x_i                (uses the new alpha)
    h_i     = h_i * max(0, 1 - alpha_i * x_i^2) + alpha_i * error * x_i
```

These are equations 3 to 6 of the paper. Weights and traces start at zero and every beta starts
at `ln(initial_step_size)`. A meta step size of zero turns IDBD into LMS with fixed per-input
step sizes.

## Behavior you can rely on

- **Checked, atomic updates.** Every input, target and intermediate value is validated. A
  failed update returns a typed error and leaves the configuration, weights, betas and traces
  bit-for-bit unchanged.
- **No allocation on the learning path.** `predict`, `update` and the inspection methods do not
  allocate. The learner allocates its candidate buffer once, at construction.
- **Complete checkpoints.** `snapshot` and `from_state` save and restore all learning state and
  validate it on restore. Saving only the weights would reset the adapted step sizes.
- **Optional bounds.** `Bounds::sutton_safety()` applies the beta floor of -10 and the per-step
  limit of 2 that the paper suggests as precautions. They do not guarantee convergence and are
  off by default. The paper's experiments ran without them.
- **No dependencies, no `unsafe`.** MSRV 1.85.

## When to use it

Use IDBD for a linear prediction that learns one example at a time from a stream whose
relationship changes, when you expect some inputs to matter more than others and do not want to
tune one learning rate by hand. Examples: latency or load forecasting from a fixed feature set,
online calibration, and research on step-size adaptation.

Do not use it for nonlinear models, batch training, sparse high-dimensional inputs or TD
learning. This crate covers one linear output with dense `f64` inputs.

## Limitations

- **Scale matters, for inputs and the target.** The trace decay `1 - alpha_i * x_i^2` assumes
  inputs of moderate size, and the meta step `theta * error * x_i * h_i` grows with the square
  of the error scale. Standardize inputs and express the target in units where typical errors
  are near 1. In the latency example, raw millisecond targets overflow a step size within a few
  samples, and the update returns `Stage::StepSize` instead of learning.
- **Correlated inputs.** The derivation assumes that changing one step size mainly affects its
  own weight. With strongly correlated inputs that approximation is weaker.
- **Theta is still a parameter.** IDBD replaces one learning rate per input with one meta step
  size. Too large a theta can diverge. Recovery after the relevant inputs change depends on theta
  and on how small the step sizes of newly relevant inputs became.
- **Numerical results are platform-sensitive.** Identical replay on the same build and target is
  bitwise reproducible. Different targets agree within floating-point tolerance, not bit for bit.

## Reference

Richard S. Sutton. *Adapting Bias by Gradient Descent: An Incremental Version of
Delta-Bar-Delta.* In Proceedings of the Tenth National Conference on Artificial Intelligence
(AAAI-92), pages 171-176, 1992. <https://cdn.aaai.org/AAAI/1992/AAAI92-027.pdf>

Figure 2 of the paper prints the trace decay as a sum, `h_i + [1 - alpha_i x_i^2]^+`. Equation 6
and the derivation show a product, which this crate implements.

The repository at <https://github.com/minikin/idbd> has the reproduction of the paper's
experiments, the evaluation against a tuned LMS baseline, benchmarks and the IDBD Explorer
browser demo.

## License

Licensed under either the Apache License, Version 2.0 or the MIT license, at your option.
