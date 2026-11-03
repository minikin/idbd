#!/usr/bin/env python3
"""Independent IDBD and Autostep reference calculations for the `idbd` crate's fixtures.

This script shares no code with the Rust crate. It evaluates Sutton's (1992) IDBD
recurrence (equations 3 to 6) and Table 1 of Mahmood et al. (2012), Autostep, in
50-digit decimal arithmetic with the Python standard library, then writes
`reference_data.rs`, which the Rust tests compare against using the
equation tolerance abs(a - b) <= 1e-12 + 1e-10 * max(|a|, |b|).

Regenerate with:

    python3 crates/idbd/tests/scenarios/fixtures/reference.py > crates/idbd/tests/scenarios/fixtures/reference_data.rs
"""

from decimal import Decimal, getcontext

getcontext().prec = 50


def d(value):
    return Decimal(str(value))


def run(alpha0, theta, examples, min_beta=None, max_beta=None, max_inc=None):
    n = len(examples[0][0])
    beta0 = d(alpha0).ln()
    w = [Decimal(0)] * n
    b = [beta0] * n
    h = [Decimal(0)] * n
    theta = d(theta)
    steps = []
    for features, target in examples:
        x = [d(v) for v in features]
        prediction = sum((wi * xi for wi, xi in zip(w, x)), Decimal(0))
        error = d(target) - prediction
        bounded = 0
        for i in range(n):
            if x[i] == 0:
                continue
            raw = theta * error * x[i] * h[i]
            proposal = b[i] + raw
            inc = raw
            if max_inc is not None:
                inc = max(-d(max_inc), min(d(max_inc), inc))
            beta = b[i] + inc
            if min_beta is not None:
                beta = max(beta, min_beta)
            if max_beta is not None:
                beta = min(beta, max_beta)
            if beta != proposal:
                bounded += 1
            alpha = beta.exp()
            step = alpha * error * x[i]
            decay = max(Decimal(0), 1 - alpha * x[i] * x[i])
            b[i] = beta
            w[i] = w[i] + step
            h[i] = h[i] * decay + step
        steps.append((features, target, prediction, error, bounded, list(w), list(b), list(h)))
    return steps


def run_autostep(alpha0, mu, tau, examples):
    """Table 1 of Mahmood, Sutton, Degris and Pilarski (2012), line by line."""
    n = len(examples[0][0])
    w = [Decimal(0)] * n
    a = [d(alpha0)] * n
    h = [Decimal(0)] * n
    v = [Decimal(0)] * n
    mu, tau = d(mu), d(tau)
    steps = []
    for features, target in examples:
        x = [d(value) for value in features]
        delta = d(target) - sum((wi * xi for wi, xi in zip(w, x)), Decimal(0))
        prediction = d(target) - delta
        for i in range(n):
            g = delta * x[i] * h[i]
            v[i] = max(abs(g), v[i] + 1 / tau * a[i] * x[i] ** 2 * (abs(g) - v[i]))
            if v[i] != 0:
                a[i] = a[i] * (mu * g / v[i]).exp()
        effective = sum((a[i] * x[i] ** 2 for i in range(n)), Decimal(0))
        m = max(effective, Decimal(1))
        for i in range(n):
            a[i] = a[i] / m
            w[i] = w[i] + a[i] * delta * x[i]
            h[i] = h[i] * (1 - a[i] * x[i] ** 2) + a[i] * delta * x[i]
        steps.append((features, target, prediction, delta, effective, list(w), list(a), list(h), list(v)))
    return steps


def emit_autostep(name, alpha0, mu, tau, steps):
    print(f"pub(crate) const {name}: AutostepFixture = AutostepFixture {{")
    print(f"    initial_step_size: {fmt(alpha0)},")
    print(f"    meta_step_size: {fmt(mu)},")
    print(f"    time_scale: {fmt(tau)},")
    print("    steps: &[")
    for features, target, prediction, error, effective, w, a, h, v in steps:
        print("        AutostepStep {")
        print(f"            features: &[{', '.join(fmt(value) for value in features)}],")
        print(f"            target: {fmt(target)},")
        print(f"            prediction: {fmt(prediction)},")
        print(f"            error: {fmt(error)},")
        print(f"            effective_step_size: {fmt(effective)},")
        print(f"            weights: &[{', '.join(fmt(value) for value in w)}],")
        print(f"            step_sizes: &[{', '.join(fmt(value) for value in a)}],")
        print(f"            traces: &[{', '.join(fmt(value) for value in h)}],")
        print(f"            normalizers: &[{', '.join(fmt(value) for value in v)}],")
        print("        },")
    print("    ],")
    print("};")
    print()


def fmt(value):
    """Shortest decimal that round-trips to the nearest f64."""
    text = repr(float(value))
    return text if any(c in text for c in ".e") else text + ".0"


def emit(name, alpha0, theta, bounds, steps):
    min_beta, max_beta, max_inc = bounds
    print(f"pub(crate) const {name}: Fixture = Fixture {{")
    print(f"    initial_step_size: {fmt(alpha0)},")
    print(f"    meta_step_size: {fmt(theta)},")
    for label, value in (("min_beta", min_beta), ("max_beta", max_beta), ("max_beta_increment", max_inc)):
        rendered = "None" if value is None else f"Some({fmt(value)})"
        print(f"    {label}: {rendered},")
    print("    steps: &[")
    for features, target, prediction, error, bounded, w, b, h in steps:
        print("        Step {")
        print(f"            features: &[{', '.join(fmt(v) for v in features)}],")
        print(f"            target: {fmt(target)},")
        print(f"            prediction: {fmt(prediction)},")
        print(f"            error: {fmt(error)},")
        print(f"            bounded_features: {bounded},")
        print(f"            weights: &[{', '.join(fmt(v) for v in w)}],")
        print(f"            betas: &[{', '.join(fmt(v) for v in b)}],")
        print(f"            traces: &[{', '.join(fmt(v) for v in h)}],")
        print("        },")
    print("    ],")
    print("};")
    print()


SEQUENCE = [
    ([1.0, 2.0, 0.5], 1.0),
    ([2.0, -1.0, 0.0], 0.5),
    ([0.3, 0.7, -1.2], -0.4),
    ([-1.5, 0.0, 2.0], 1.7),
    ([5.0, 0.1, 0.2], 3.0),
    ([0.0, -2.2, 1.1], -1.0),
    ([1.1, 1.2, 1.3], 2.2),
    ([-0.4, 0.9, -0.8], 0.0),
]

print("// @generated by reference.py. Do not edit by hand.")
print("// Independent 50-digit decimal evaluation of Sutton (1992), equations 3 to 6, and of")
print("// Table 1 of Mahmood et al. (2012).")
print()
print('#![expect(\n    clippy::unreadable_literal,\n    clippy::approx_constant,\n    reason = \"generated full-precision reference values\"\n)]')
print()
print("use super::{AutostepFixture, AutostepStep, Fixture, Step};")
print()
emit("C06_C07", 0.1, 0.01, (None, None, None),
     run(0.1, 0.01, [([1.0, 2.0], 1.0), ([2.0, -1.0], 0.5)]))
emit("SEQUENCE_UNBOUNDED", 0.05, 0.05, (None, None, None), run(0.05, 0.05, SEQUENCE))
beta0 = d(0.05).ln()
bounds = (beta0 - d("0.002"), beta0 + d("0.003"), d("0.0015"))
emit("SEQUENCE_BOUNDED", 0.05, 0.5, bounds,
     run(0.05, 0.5, SEQUENCE, min_beta=bounds[0], max_beta=bounds[1], max_inc=bounds[2]))
# The paper's defaults. Step 5 has sum(alpha x^2) > 1, so the overshoot guard acts.
emit_autostep("AUTOSTEP_DEFAULTS", 0.1, 0.01, 10000, run_autostep(0.1, 0.01, 10000, SEQUENCE))
# A large mu and a short time scale make every term of the normalizer update matter.
emit_autostep("AUTOSTEP_FAST", 0.2, 0.5, 2, run_autostep(0.2, 0.5, 2, SEQUENCE))
