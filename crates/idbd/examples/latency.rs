//! Predict request latency online across a deployment change.
//!
//! Run with:
//!
//! ```text
//! cargo run -p idbd --example latency
//! ```
//!
//! The data is synthetic. A service's latency in milliseconds depends on load, payload
//! size and cache hit ratio. At sample 3,000 a deployment replaces the cache, so the hit
//! ratio stops mattering and payload size matters twice as much. The example shows the
//! loop an application runs: prepare inputs, predict before the outcome is known, learn
//! from the observed latency, skip invalid samples, and checkpoint the complete learner
//! together with the stream position.

#![expect(clippy::print_stdout, reason = "an example reports to the terminal")]

use std::error::Error;
use std::fmt::Write as _;

use idbd::{Config, Idbd, State};

/// Samples in the stream.
const SAMPLES: u64 = 6_000;
/// The deployment change takes effect before this sample.
const DEPLOYMENT: u64 = 3_000;
/// A broken metric reports a NaN payload size for this sample.
const BROKEN_SAMPLE: u64 = 1_500;
/// The application checkpoints after this sample and resumes from the checkpoint.
const CHECKPOINT: u64 = 4_000;
/// Samples per row of the error report.
const REPORT_EVERY: u64 = 500;

/// One raw observation, in the units the service exports.
#[derive(Debug, Clone, Copy)]
struct Observation {
    /// Requests per second.
    load_rps: f64,
    /// Request payload in kilobytes.
    payload_kb: f64,
    /// Cache hit ratio in `[0, 1]`.
    cache_hit_ratio: f64,
    /// Observed latency in milliseconds, known only after the request completes.
    latency_ms: f64,
}

/// Typical latency and its spread, used to scale the target. With raw milliseconds the
/// first errors are around 60, the meta step `theta * error * x * h` grows quickly, and
/// the step size of the constant input overflows within a few samples.
const LATENCY_CENTER_MS: f64 = 50.0;
const LATENCY_SCALE_MS: f64 = 10.0;

/// Input preparation: center and scale each raw metric to roughly unit size, and add a
/// constant 1.0 so the learner can fit the baseline latency. IDBD's trace decay assumes
/// inputs of moderate magnitude, so unscaled metrics such as 800 requests per second
/// would make learning unstable.
fn prepare(o: &Observation) -> [f64; 4] {
    [
        (o.load_rps - 500.0) / 200.0,
        (o.payload_kb - 64.0) / 32.0,
        (o.cache_hit_ratio - 0.8) / 0.1,
        1.0,
    ]
}

fn scale_target(latency_ms: f64) -> f64 {
    (latency_ms - LATENCY_CENTER_MS) / LATENCY_SCALE_MS
}

/// A deterministic synthetic service. Its full state is the generator state and the
/// sample index, so a checkpoint can restore the stream position exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Service {
    rng: u64,
    sample: u64,
}

impl Service {
    fn new(seed: u64) -> Self {
        Self {
            rng: seed,
            sample: 0,
        }
    }

    /// `xorshift64*`, mapped to a uniform value in `[0, 1)`.
    fn uniform(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let bits = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 12;
        f64::from_bits((0x3FF << 52) | bits) - 1.0
    }

    fn next(&mut self) -> Observation {
        self.sample += 1;
        let load_rps = 300.0 + 400.0 * self.uniform();
        let payload_kb = 16.0 + 96.0 * self.uniform();
        let cache_hit_ratio = 0.65 + 0.3 * self.uniform();
        let noise_ms = 2.0 * (self.uniform() - 0.5);
        let before_deployment = self.sample < DEPLOYMENT;
        let (payload_ms_per_kb, cache_ms) = if before_deployment {
            (0.15, 40.0)
        } else {
            (0.30, 0.0)
        };
        let latency_ms = 30.0
            + 0.02 * load_rps
            + payload_ms_per_kb * payload_kb
            + cache_ms * (1.0 - cache_hit_ratio)
            + noise_ms;
        let payload_kb = if self.sample == BROKEN_SAMPLE {
            f64::NAN
        } else {
            payload_kb
        };
        Observation {
            load_rps,
            payload_kb,
            cache_hit_ratio,
            latency_ms,
        }
    }
}

/// A complete checkpoint: the learner state and the stream position. Restoring only the
/// weights would reset the adapted step sizes and traces.
fn save(learner: &Idbd, service: &Service) -> Result<String, std::fmt::Error> {
    let state = learner.snapshot();
    let mut text = String::new();
    writeln!(
        text,
        "initial_step_size {:e}",
        state.config.initial_step_size()
    )?;
    writeln!(text, "meta_step_size {:e}", state.config.meta_step_size())?;
    for (name, values) in [
        ("weights", &state.weights),
        ("betas", &state.betas),
        ("traces", &state.traces),
    ] {
        write!(text, "{name}")?;
        for value in values {
            // `{:e}` prints the shortest representation that parses back to the same f64.
            write!(text, " {value:e}")?;
        }
        writeln!(text)?;
    }
    writeln!(text, "stream {} {}", service.rng, service.sample)?;
    Ok(text)
}

fn load(text: &str) -> Result<(Idbd, Service), Box<dyn Error>> {
    let mut lines = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>());
    let mut field = |name: &str| -> Result<Vec<String>, Box<dyn Error>> {
        let line = lines.next().ok_or("checkpoint ends early")?;
        match line.split_first() {
            Some((head, rest)) if *head == name => {
                Ok(rest.iter().map(ToString::to_string).collect())
            }
            _ => Err(format!("expected {name}").into()),
        }
    };
    let number = |values: &[String]| -> Result<f64, Box<dyn Error>> {
        Ok(values.first().ok_or("missing value")?.parse()?)
    };
    let numbers = |values: Vec<String>| -> Result<Vec<f64>, Box<dyn Error>> {
        values.iter().map(|v| Ok(v.parse()?)).collect()
    };
    let config = Config::new(
        number(&field("initial_step_size")?)?,
        number(&field("meta_step_size")?)?,
    )?;
    let weights = numbers(field("weights")?)?;
    let betas = numbers(field("betas")?)?;
    let traces = numbers(field("traces")?)?;
    let stream = field("stream")?;
    let service = Service {
        rng: stream.first().ok_or("missing rng")?.parse()?,
        sample: stream.get(1).ok_or("missing sample")?.parse()?,
    };
    // from_state validates every value, so a corrupted file is rejected here.
    let learner = Idbd::from_state(State::new(config, weights, betas, traces))?;
    Ok((learner, service))
}

/// Mean absolute pre-update error per report interval, in milliseconds.
#[derive(Debug, Default)]
struct Report {
    absolute_error_ms: f64,
    samples: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initial step size 0.02 for every input and theta 0.01, both from a short trial on
    // data recorded before the deployment. No bounds.
    let mut learner = Idbd::new(4, Config::new(0.02, 0.01)?)?;
    let mut service = Service::new(0x1D8D_2026);
    let mut report = Report::default();
    let mut checkpoint = None;

    println!("synthetic latency stream, {SAMPLES} samples, deployment before sample {DEPLOYMENT}");
    println!(
        "samples          mean |error| ms   (observed minus predicted, predicted before learning)"
    );
    while service.sample < SAMPLES {
        let observation = service.next();
        let features = prepare(&observation);

        // Predict when the request arrives, learn when its latency is observed.
        match learner.predict(&features) {
            Ok(predicted) => {
                let update = learner.update(&features, scale_target(observation.latency_ms))?;
                debug_assert_eq!(
                    update.prediction.to_bits(),
                    predicted.to_bits(),
                    "update reports the prediction made before learning"
                );
                let predicted_ms = LATENCY_CENTER_MS + LATENCY_SCALE_MS * predicted;
                report.absolute_error_ms += (observation.latency_ms - predicted_ms).abs();
                report.samples += 1;
            }
            // The learner rejects the sample and keeps its state. The caller logs it and
            // moves on, and the stream position still advances.
            Err(error) => println!(
                "sample {}: skipped invalid sample ({error})",
                service.sample
            ),
        }
        if service.sample % REPORT_EVERY == 0 {
            let first = service.sample + 1 - REPORT_EVERY;
            let marker = if (first..=service.sample).contains(&DEPLOYMENT) {
                "  <- deployment change"
            } else {
                ""
            };
            println!(
                "{first:>5} to {:>5}   {:>8.3}   ({} learned){marker}",
                service.sample,
                report.absolute_error_ms / f64::from(report.samples),
                report.samples,
            );
            report = Report::default();
        }
        if service.sample == CHECKPOINT {
            checkpoint = Some(save(&learner, &service)?);
        }
    }

    // A restarted process resumes from the checkpoint and replays the rest of the stream.
    let text = checkpoint.ok_or("no checkpoint was written")?;
    let (mut resumed, mut resumed_service) = load(&text)?;
    while resumed_service.sample < SAMPLES {
        let observation = resumed_service.next();
        resumed.update(&prepare(&observation), scale_target(observation.latency_ms))?;
    }
    let identical = resumed.snapshot() == learner.snapshot();
    println!(
        "\nresumed from the checkpoint at sample {CHECKPOINT}: final state identical = {identical}"
    );

    println!("\nstep size per input after the stream:");
    let names = ["load", "payload", "cache hit ratio", "constant 1"];
    for ((name, alpha), weight) in names
        .iter()
        .zip(learner.step_sizes())
        .zip(learner.weights())
    {
        println!("  {name:<16} alpha {alpha:.5}   weight {weight:+.3}");
    }
    if !identical {
        return Err("the resumed learner diverged from the original".into());
    }
    Ok(())
}
