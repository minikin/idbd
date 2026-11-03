# Changelog

All notable changes to the `idbd` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `Idbd`, a dense linear learner implementing equations 3 to 6 of Sutton (1992), with
  `predict`, `update`, inspection of weights, log step sizes, traces and step sizes, and full
  `snapshot` and `from_state` checkpoints.
- `Config` and `Bounds`, validated at construction. `Bounds::sutton_safety()` provides the
  optional beta floor of -10 and per-step limit of 2.
- Typed errors: `ConfigError`, `Error` with the failed `Stage` and coordinate, and `StateError`.
- `autostep::Autostep`, the Autostep learner of Mahmood, Sutton, Degris and Pilarski (2012),
  with its own `Config`, `State`, `StateError` and `Update`, and the stages `Normalizer` and
  `EffectiveStepSize`.
- Optional `serde` feature: `Serialize` and `Deserialize` for `Config`, `Bounds`, `State` and
  `Idbd`, and for the Autostep types. Deserialization validates configurations and learners
  like their constructors.

### Numerical and evaluation limitations

- Equation fixtures agree with an independent 50-digit decimal reference within
  `1e-12 + 1e-10 * max(|a|, |b|)`. Bitwise agreement is promised only for identical replay on
  the same build and target.
- The paper's experiments are reproduced approximately with recorded seeds and a documented
  generator. The paper's own random draws are unpublished, so exact numbers are not reproduced.
