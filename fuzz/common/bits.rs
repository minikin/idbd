//! Bit patterns of a whole state. Derived `PartialEq` treats `-0.0` and `0.0` as equal,
//! but the crate promises bit-for-bit preservation, so the targets compare these.

use idbd::{State, autostep};

fn option_bits(value: Option<f64>) -> Option<u64> {
    value.map(f64::to_bits)
}

fn vector_bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// Every float in an IDBD state, the configuration and its bounds included.
pub(crate) fn idbd(state: &State) -> (Vec<Vec<u64>>, [u64; 2], [Option<u64>; 3]) {
    let bounds = state.config.bounds();
    (
        vec![
            vector_bits(&state.weights),
            vector_bits(&state.betas),
            vector_bits(&state.traces),
        ],
        [
            state.config.initial_step_size().to_bits(),
            state.config.meta_step_size().to_bits(),
        ],
        [
            option_bits(bounds.min_beta()),
            option_bits(bounds.max_beta()),
            option_bits(bounds.max_beta_increment()),
        ],
    )
}

/// Every float in an Autostep state, the configuration included.
pub(crate) fn autostep(state: &autostep::State) -> (Vec<Vec<u64>>, [u64; 3]) {
    (
        vec![
            vector_bits(&state.weights),
            vector_bits(&state.step_sizes),
            vector_bits(&state.traces),
            vector_bits(&state.normalizers),
        ],
        [
            state.config.initial_step_size().to_bits(),
            state.config.meta_step_size().to_bits(),
            state.config.time_scale().to_bits(),
        ],
    )
}
