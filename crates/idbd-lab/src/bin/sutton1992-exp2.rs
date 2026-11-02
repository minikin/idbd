//! Scenario E02: Sutton (1992), Experiment 2, and an adapted demonstration beside it.
//!
//! Records how IDBD separates the step sizes of the five relevant inputs from the
//! fifteen irrelevant ones over 250,000 samples, and sweeps a fixed step size over the
//! relevant inputs alone for the comparison in the paper's Figure 5.

use std::process::ExitCode;

use idbd::{Bounds, Config};
use idbd_lab::alphas::{Checkpoint, Trajectory, record};
use idbd_lab::cli::{Invocation, arguments, parse};
use idbd_lab::csv::{Csv, number};
use idbd_lab::lms::Lms;
use idbd_lab::plot::{Anchor, Scale, Svg, nice_axis, tick_label};
use idbd_lab::report::{magnitude, parameter, plus_minus, provenance, write};
use idbd_lab::stats::{Summary, exact_f64};
use idbd_lab::sweep::{Failure, Point, SeedRun, best};
use idbd_lab::world::{GROUP_SIZE, NUM_FEATURES, Preset, SIGN_FLIP_PERIOD, World};

/// Samples in one Experiment 2 run.
const SAMPLES: u64 = 250_000;

/// Samples between recorded step-size checkpoints.
const INTERVAL: u64 = 1_000;

/// Starting step size of every input, as in the paper.
const INITIAL_STEP_SIZE: f64 = 0.05;

/// Meta step size of the paper configuration.
const PAPER_THETA: f64 = 0.001;

/// Meta step size of the adapted demonstration, which is not in the paper.
const ADAPTED_THETA: f64 = 0.01;

/// Seeds for the long runs.
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

/// Fixed step sizes for the Figure 5 comparison, bracketing the paper's 0.13.
const FIXED_RATE_GRID: [f64; 11] = [
    0.05, 0.08, 0.10, 0.11, 0.12, 0.13, 0.14, 0.15, 0.17, 0.20, 0.25,
];

/// Seeds for the Figure 5 comparison.
const SWEEP_SEEDS: [u64; 20] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
];

const SWEEP_WARMUP: u64 = 20_000;
const SWEEP_MEASURED: u64 = 10_000;

/// Mean step size Sutton reports for the relevant inputs after 250,000 samples.
const PAPER_RELEVANT: f64 = 0.13;

/// Half-width Sutton reports around that mean.
const PAPER_RELEVANT_SPREAD: f64 = 0.015;

/// Upper bound Sutton reports for every irrelevant step size after 250,000 samples.
const PAPER_IRRELEVANT_MAX: f64 = 0.007;

/// Minimum Sutton reports for the Figure 5 fixed-rate sweep.
const PAPER_FIGURE5_MINIMUM: f64 = 0.13;

const DEFAULT_OUT: &str = "docs/results/sutton1992-experiment2";

const RELEVANT_COLOR: &str = "#ff6f61";
// Both series are IDBD. Blue is reserved for LMS across the project.
const IRRELEVANT_COLOR: &str = "#6b7480";
const INK: &str = "#14181f";
const MUTED: &str = "#5c6673";
const GRID_COLOR: &str = "#d8dee6";
const PAPER_COLOR: &str = "#8a94a3";
const BAND_COLOR: &str = "#eceff3";

const LEFT: f64 = 92.0;
const RIGHT: f64 = 860.0;
const TOP: f64 = 112.0;
const BOTTOM: f64 = 436.0;
const WIDTH: f64 = 900.0;
const HEIGHT: f64 = 540.0;

/// One long configuration and its runs.
#[derive(Debug)]
struct LongRun {
    label: &'static str,
    file: &'static str,
    theta: f64,
    from_paper: bool,
    trajectories: Vec<Trajectory>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = match parse(arguments(), DEFAULT_OUT)? {
        Invocation::Help => {
            println!("sutton1992-exp2 [--out <dir>]\n\ndefault --out {DEFAULT_OUT}");
            return Ok(());
        }
        Invocation::Run(options) => options,
    };
    eprintln!("{}", provenance());

    let mut runs = Vec::new();
    for (label, file, theta, from_paper) in [
        ("paper configuration", "figure4.svg", PAPER_THETA, true),
        (
            "adapted demonstration, not in the paper",
            "adapted-demonstration.svg",
            ADAPTED_THETA,
            false,
        ),
    ] {
        eprintln!("{label}: theta {}, {SAMPLES} samples", parameter(theta));
        let config = Config::new(INITIAL_STEP_SIZE, theta)
            .and_then(|c| c.with_bounds(Bounds::none()))
            .map_err(|e| e.to_string())?;
        let mut trajectories = Vec::with_capacity(SEEDS.len());
        for &seed in &SEEDS {
            let trajectory = record(Preset::Sutton1992, seed, SAMPLES, INTERVAL, config)?;
            eprintln!(
                "  seed {seed}: relevant alpha {}, irrelevant alpha {}, largest irrelevant {}",
                plus_minus(trajectory.final_relevant()),
                plus_minus(trajectory.final_irrelevant()),
                trajectory
                    .final_irrelevant_max()
                    .map_or_else(|| "-".to_owned(), magnitude)
            );
            if let Some(failure) = &trajectory.failure {
                eprintln!("  seed {seed} failed at sample {}", failure.sample);
            }
            trajectories.push(trajectory);
        }
        runs.push(LongRun {
            label,
            file,
            theta,
            from_paper,
            trajectories,
        });
    }

    eprintln!("fixed-rate sweep over the relevant inputs only");
    let mut sweep = Vec::with_capacity(FIXED_RATE_GRID.len());
    for &step_size in &FIXED_RATE_GRID {
        let mut runs = Vec::with_capacity(SWEEP_SEEDS.len());
        for &seed in &SWEEP_SEEDS {
            let (mse, failure) = relevant_only_lms(step_size, seed)?;
            runs.push(SeedRun { seed, mse, failure });
        }
        let point = Point {
            value: step_size,
            runs,
        };
        eprintln!(
            "  alpha {} -> MSE {}, {} failed",
            parameter(step_size),
            plus_minus(point.summary()),
            point.failures()
        );
        sweep.push(point);
    }

    let out = options.out();
    write(out, "trajectories.csv", &trajectories_csv(&runs))?;
    write(out, "final-step-sizes.csv", &final_step_sizes_csv(&runs))?;
    write(out, "figure5-sweep.csv", &sweep_csv(&sweep))?;
    for long_run in &runs {
        write(out, long_run.file, &trajectory_figure(long_run))?;
    }
    write(out, "figure5.svg", &sweep_figure(&sweep))?;
    write(out, "README.md", &readme(&runs, &sweep))?;
    Ok(())
}

/// Runs fixed-step LMS on the five relevant inputs alone, which is the Figure 5 setup:
/// the irrelevant step sizes are held at zero, so those inputs cannot move a weight.
fn relevant_only_lms(step_size: f64, seed: u64) -> Result<(Option<f64>, Option<Failure>), String> {
    let mut lms = Lms::new(GROUP_SIZE, step_size).map_err(|e| e.to_string())?;
    let mut world = World::new(Preset::Sutton1992, seed);
    let mut features = [0.0; NUM_FEATURES];
    let mut events = Vec::new();
    let mut sum = 0.0;
    let samples = SWEEP_WARMUP + SWEEP_MEASURED;
    for t in 1..=samples {
        let target = world.next_sample(&mut features, &mut events);
        match lms.update(&features[..GROUP_SIZE], target) {
            Ok(outcome) => {
                if t > SWEEP_WARMUP {
                    sum += outcome.error * outcome.error;
                }
            }
            Err(error) => {
                return Ok((
                    None,
                    Some(Failure {
                        sample: t,
                        reason: error.to_string(),
                    }),
                ));
            }
        }
    }
    let mse = sum / exact_f64(SWEEP_MEASURED);
    if mse.is_finite() {
        return Ok((Some(mse), None));
    }
    // As in `experiment::run`: a sum that overflowed while the weights stayed finite is a
    // failure of the measurement, not a missing value.
    Ok((
        None,
        Some(Failure {
            sample: samples,
            reason: format!(
                "no finite mean over {SWEEP_MEASURED} measured samples: squared errors sum to \
                 {sum}"
            ),
        }),
    ))
}

fn trajectories_csv(runs: &[LongRun]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "run",
        "from_paper",
        "theta",
        "initial_step_size",
        "seed",
        "sample",
        "relevant_mean",
        "relevant_min",
        "relevant_max",
        "irrelevant_mean",
        "irrelevant_max",
    ]);
    for long_run in runs {
        for trajectory in &long_run.trajectories {
            for checkpoint in &trajectory.checkpoints {
                csv.row([
                    long_run.label.to_owned(),
                    long_run.from_paper.to_string(),
                    number(Some(long_run.theta)),
                    number(Some(INITIAL_STEP_SIZE)),
                    trajectory.seed.to_string(),
                    checkpoint.sample.to_string(),
                    number(Some(checkpoint.relevant_mean)),
                    number(Some(checkpoint.relevant_min)),
                    number(Some(checkpoint.relevant_max)),
                    number(Some(checkpoint.irrelevant_mean)),
                    number(Some(checkpoint.irrelevant_max)),
                ]);
            }
        }
    }
    csv.into_string()
}

fn final_step_sizes_csv(runs: &[LongRun]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "run",
        "theta",
        "seed",
        "feature",
        "relevant",
        "step_size",
        "failure_sample",
        "failure_reason",
    ]);
    for long_run in runs {
        for trajectory in &long_run.trajectories {
            for (feature, &step_size) in trajectory.final_step_sizes.iter().enumerate() {
                csv.row([
                    long_run.label.to_owned(),
                    number(Some(long_run.theta)),
                    trajectory.seed.to_string(),
                    feature.to_string(),
                    (feature < GROUP_SIZE).to_string(),
                    number(Some(step_size)),
                    trajectory
                        .failure
                        .as_ref()
                        .map_or_else(String::new, |f| f.sample.to_string()),
                    trajectory
                        .failure
                        .as_ref()
                        .map_or_else(String::new, |f| f.reason.clone()),
                ]);
            }
        }
    }
    csv.into_string()
}

fn sweep_csv(sweep: &[Point]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "step_size",
        "seed",
        "warmup",
        "measured",
        "mse",
        "failure_sample",
        "failure_reason",
    ]);
    for point in sweep {
        for run in &point.runs {
            csv.row([
                number(Some(point.value)),
                run.seed.to_string(),
                SWEEP_WARMUP.to_string(),
                SWEEP_MEASURED.to_string(),
                number(run.mse),
                run.failure
                    .as_ref()
                    .map_or_else(String::new, |f| f.sample.to_string()),
                run.failure
                    .as_ref()
                    .map_or_else(String::new, |f| f.reason.clone()),
            ]);
        }
    }
    csv.into_string()
}

/// A checkpoint averaged across the seeds that reached it.
#[derive(Debug)]
struct Aggregate {
    sample: u64,
    /// How many seeds reached this checkpoint. A seed that failed earlier is absent from
    /// every later point, so the plotted curve is not an average over the same seeds
    /// throughout.
    seeds: usize,
    relevant_mean: f64,
    irrelevant_mean: f64,
    irrelevant_max: f64,
}

fn aggregate(trajectories: &[Trajectory]) -> Vec<Aggregate> {
    let longest = trajectories
        .iter()
        .max_by_key(|t| t.checkpoints.len())
        .map(|t| t.checkpoints.clone())
        .unwrap_or_default();
    longest
        .iter()
        .filter_map(|reference| {
            let matching: Vec<&Checkpoint> = trajectories
                .iter()
                .filter_map(|t| t.checkpoints.iter().find(|c| c.sample == reference.sample))
                .collect();
            let mean = |values: Vec<f64>| Summary::new(&values).map(Summary::mean);
            Some(Aggregate {
                sample: reference.sample,
                seeds: matching.len(),
                relevant_mean: mean(matching.iter().map(|c| c.relevant_mean).collect())?,
                irrelevant_mean: mean(matching.iter().map(|c| c.irrelevant_mean).collect())?,
                irrelevant_max: mean(matching.iter().map(|c| c.irrelevant_max).collect())?,
            })
        })
        .collect()
}

/// States how many seeds the plotted curves average, naming both counts when a seed
/// failed partway and later checkpoints cover fewer of them.
fn seed_count_phrase(series: &[Aggregate]) -> String {
    let fewest = series.iter().map(|a| a.seeds).min().unwrap_or(0);
    let most = series.iter().map(|a| a.seeds).max().unwrap_or(0);
    if fewest == most {
        format!("mean over {most} seeds")
    } else {
        format!("mean over {most} seeds falling to {fewest} where seeds failed")
    }
}

fn trajectory_figure(long_run: &LongRun) -> String {
    let series = aggregate(&long_run.trajectories);
    let highest = series
        .iter()
        .map(|a| a.relevant_mean.max(a.irrelevant_max))
        .chain(std::iter::once(PAPER_RELEVANT + PAPER_RELEVANT_SPREAD))
        .filter(|v| v.is_finite())
        .reduce(f64::max)
        .unwrap_or(0.2);
    let (y_upper, y_step) = nice_axis(highest);
    let x = Scale::new((0.0, exact_f64(SAMPLES)), (LEFT, RIGHT));
    let y = Scale::new((0.0, y_upper), (BOTTOM, TOP));

    let mut svg = Svg::new(WIDTH, HEIGHT);
    svg.rect(0.0, 0.0, WIDTH, HEIGHT, "#ffffff");
    svg.text(
        (LEFT, 36.0),
        Anchor::Start,
        19.0,
        INK,
        &format!(
            "IDBD step sizes over {SAMPLES} samples, theta {}",
            parameter(long_run.theta)
        ),
    );
    svg.text(
        (LEFT, 56.0),
        Anchor::Start,
        12.5,
        if long_run.from_paper { MUTED } else { INK },
        long_run.label,
    );
    svg.text(
        (LEFT, 74.0),
        Anchor::Start,
        12.5,
        MUTED,
        &format!(
            "Sutton 1992 Experiment 1 task, initial step size {}, no beta bounds, {}.",
            parameter(INITIAL_STEP_SIZE),
            seed_count_phrase(&series)
        ),
    );

    if long_run.from_paper {
        let band_top = y.at(PAPER_RELEVANT + PAPER_RELEVANT_SPREAD);
        let band_bottom = y.at(PAPER_RELEVANT - PAPER_RELEVANT_SPREAD);
        svg.rect(
            LEFT,
            band_top,
            RIGHT - LEFT,
            band_bottom - band_top,
            BAND_COLOR,
        );
    }

    draw_value_axis(&mut svg, &y, y_upper, y_step, "Step size alpha");

    if long_run.from_paper {
        for (value, label) in [
            (
                PAPER_RELEVANT,
                "Sutton Figure 4: relevant alphas 0.13 +/- 0.015",
            ),
            (
                PAPER_IRRELEVANT_MAX,
                "Sutton Figure 4: every irrelevant alpha below 0.007",
            ),
        ] {
            let py = y.at(value);
            svg.line((LEFT, py), (RIGHT, py), PAPER_COLOR, 1.0, "6 5");
            svg.text(
                (RIGHT - 4.0, py - 6.0),
                Anchor::End,
                11.5,
                PAPER_COLOR,
                label,
            );
        }
    }

    for (index, (color, dashes, label, pick)) in series_definitions().into_iter().enumerate() {
        let line: Vec<(f64, f64)> = series
            .iter()
            .map(|a| (x.at(exact_f64(a.sample)), y.at(pick(a))))
            .collect();
        svg.polyline(&line, color, 2.2, dashes);
        let row = f64::from(u32::try_from(index).unwrap_or(0));
        let legend_y = TOP + 22.0 + 18.0 * row;
        svg.line(
            (LEFT + 14.0, legend_y - 4.0),
            (LEFT + 34.0, legend_y - 4.0),
            color,
            2.2,
            dashes,
        );
        svg.text((LEFT + 42.0, legend_y), Anchor::Start, 12.0, INK, label);
    }

    draw_sample_axis(&mut svg, &x);
    svg.into_string()
}

fn draw_sample_axis(svg: &mut Svg, x: &Scale) {
    svg.line((LEFT, BOTTOM), (RIGHT, BOTTOM), INK, 1.4, "");
    for fraction in 0_u64..=5 {
        let sample = SAMPLES * fraction / 5;
        let px = x.at(exact_f64(sample));
        svg.line((px, BOTTOM), (px, BOTTOM + 6.0), INK, 1.2, "");
        svg.text(
            (px, BOTTOM + 22.0),
            Anchor::Middle,
            12.0,
            INK,
            &sample.to_string(),
        );
    }
    svg.text(
        (f64::midpoint(LEFT, RIGHT), BOTTOM + 46.0),
        Anchor::Middle,
        13.5,
        INK,
        "Samples processed",
    );
    svg.text(
        (LEFT, HEIGHT - 26.0),
        Anchor::Start,
        11.5,
        MUTED,
        &provenance(),
    );
}

fn draw_value_axis(svg: &mut Svg, y: &Scale, upper: f64, step: f64, title: &str) {
    let mut tick = 0.0;
    while tick <= upper + step / 2.0 {
        let py = y.at(tick);
        svg.line((LEFT, py), (RIGHT, py), GRID_COLOR, 1.0, "");
        svg.text(
            (LEFT - 10.0, py + 4.0),
            Anchor::End,
            12.0,
            MUTED,
            &tick_label(tick, step),
        );
        tick += step;
    }
    svg.vertical_text(
        (34.0, f64::midpoint(TOP, BOTTOM)),
        Anchor::Middle,
        13.0,
        INK,
        title,
    );
}

type SeriesPick = fn(&Aggregate) -> f64;

fn series_definitions() -> [(&'static str, &'static str, &'static str, SeriesPick); 3] {
    [
        (
            RELEVANT_COLOR,
            "",
            "Mean step size of the 5 relevant inputs",
            |a| a.relevant_mean,
        ),
        (
            IRRELEVANT_COLOR,
            "",
            "Mean step size of the 15 irrelevant inputs",
            |a| a.irrelevant_mean,
        ),
        (
            IRRELEVANT_COLOR,
            "5 4",
            "Largest step size among the irrelevant inputs",
            |a| a.irrelevant_max,
        ),
    ]
}

fn sweep_figure(sweep: &[Point]) -> String {
    let highest = sweep
        .iter()
        .filter_map(|p| p.summary().map(|s| s.mean() + s.standard_error()))
        .filter(|v| v.is_finite())
        .reduce(f64::max)
        .unwrap_or(1.0);
    let (y_upper, y_step) = nice_axis(highest);
    let last = FIXED_RATE_GRID.last().copied().unwrap_or(0.25);
    let x = Scale::new((0.0, last), (LEFT, RIGHT));
    let y = Scale::new((0.0, y_upper), (BOTTOM, TOP));

    let mut svg = Svg::new(WIDTH, HEIGHT);
    svg.rect(0.0, 0.0, WIDTH, HEIGHT, "#ffffff");
    svg.text(
        (LEFT, 36.0),
        Anchor::Start,
        19.0,
        INK,
        "Fixed step size on the relevant inputs alone",
    );
    svg.text(
        (LEFT, 56.0),
        Anchor::Start,
        12.5,
        MUTED,
        &format!(
            "Sutton 1992 Figure 5 setup: LMS over the {GROUP_SIZE} relevant inputs, the other \
             {} held out. {SWEEP_WARMUP} warm-up + {SWEEP_MEASURED} measured samples, {} seeds.",
            NUM_FEATURES - GROUP_SIZE,
            SWEEP_SEEDS.len()
        ),
    );

    draw_value_axis(
        &mut svg,
        &y,
        y_upper,
        y_step,
        "Mean squared error over the measured samples",
    );

    let marker = x.at(PAPER_FIGURE5_MINIMUM);
    svg.line((marker, TOP), (marker, BOTTOM), PAPER_COLOR, 1.0, "6 5");
    svg.text(
        (marker + 6.0, TOP + 14.0),
        Anchor::Start,
        11.5,
        PAPER_COLOR,
        "Sutton Figure 5: minimum near 0.13",
    );

    let line: Vec<(f64, f64)> = sweep
        .iter()
        .filter_map(|p| p.summary().map(|s| (x.at(p.value), y.at(s.mean()))))
        .collect();
    svg.polyline(&line, RELEVANT_COLOR, 2.4, "");
    for point in sweep {
        let Some(summary) = point.summary() else {
            continue;
        };
        let px = x.at(point.value);
        svg.circle((px, y.at(summary.mean())), 4.0, RELEVANT_COLOR);
    }

    svg.line((LEFT, BOTTOM), (RIGHT, BOTTOM), INK, 1.4, "");
    for &tick in &FIXED_RATE_GRID {
        let px = x.at(tick);
        svg.line((px, BOTTOM), (px, BOTTOM + 6.0), INK, 1.2, "");
        svg.text(
            (px, BOTTOM + 22.0),
            Anchor::Middle,
            11.0,
            INK,
            &parameter(tick),
        );
    }
    svg.text(
        (f64::midpoint(LEFT, RIGHT), BOTTOM + 46.0),
        Anchor::Middle,
        13.5,
        INK,
        "Fixed step size alpha on the relevant inputs",
    );
    svg.text(
        (LEFT, HEIGHT - 26.0),
        Anchor::Start,
        11.5,
        MUTED,
        &provenance(),
    );
    svg.into_string()
}

fn readme(runs: &[LongRun], sweep: &[Point]) -> String {
    let mut sections = vec![header(), protocol_section()];
    for long_run in runs {
        sections.push(long_run_section(long_run));
    }
    sections.push(sweep_section(sweep));
    sections.push(files_section());
    sections.concat()
}

fn header() -> String {
    format!(
        "# Sutton 1992, Experiment 2: step-size separation\n\n\
         Scenario E02. Produced by `cargo run --release -p idbd-lab --bin sutton1992-exp2`.\n\n\
         {}\n\n",
        provenance()
    )
}

fn protocol_section() -> String {
    format!(
        "## Protocol\n\n\
         - Task: the Sutton (1992) Experiment 1 tracking task. {NUM_FEATURES} independent \
         standard normal inputs, the first {GROUP_SIZE} relevant with coefficients +1 or -1 drawn \
         from the seed. One of the five, chosen uniformly at random, flips sign before samples \
         21, 41, 61 and so on, every {SIGN_FLIP_PERIOD} samples. The same five stay relevant for \
         the whole run.\n\
         - Each run processes {SAMPLES} samples with initial step size {alpha0} on every input \
         and beta bounds disabled (`Bounds::none()`).\n\
         - Step sizes are recorded every {INTERVAL} samples and at the final sample.\n\
         - Seeds {first}..={last}. A seed fixes the inputs, the initial signs and which \
         coefficient flips at each boundary.\n\n",
        alpha0 = parameter(INITIAL_STEP_SIZE),
        first = SEEDS.first().copied().unwrap_or_default(),
        last = SEEDS.last().copied().unwrap_or_default(),
    )
}

fn long_run_section(long_run: &LongRun) -> String {
    let heading = if long_run.from_paper {
        format!(
            "## Paper configuration, theta {}",
            parameter(long_run.theta)
        )
    } else {
        format!(
            "## Adapted demonstration, theta {}\n\nThis configuration is not in the paper. It \
             raises the meta step size by a factor of ten to show faster separation, and its \
             numbers must not be read as a reproduction of anything Sutton reports.",
            parameter(long_run.theta)
        )
    };

    let mut rows = vec![format!(
        "{heading}\n\n![Step sizes over {SAMPLES} samples]({})\n\nFinal step sizes after \
         {SAMPLES} samples:\n\n\
         | seed | relevant mean +/- SE | relevant range | irrelevant mean +/- SE | largest \
         irrelevant | failure |\n| --- | --- | --- | --- | --- | --- |\n",
        long_run.file
    )];
    for trajectory in &long_run.trajectories {
        let relevant: Vec<f64> = trajectory.final_step_sizes[..GROUP_SIZE].to_vec();
        rows.push(format!(
            "| {} | {} | {} to {} | {} | {} | {} |\n",
            trajectory.seed,
            plus_minus(trajectory.final_relevant()),
            relevant
                .iter()
                .copied()
                .reduce(f64::min)
                .map_or_else(|| "-".to_owned(), magnitude),
            relevant
                .iter()
                .copied()
                .reduce(f64::max)
                .map_or_else(|| "-".to_owned(), magnitude),
            plus_minus(trajectory.final_irrelevant()),
            trajectory
                .final_irrelevant_max()
                .map_or_else(|| "-".to_owned(), magnitude),
            trajectory
                .failure
                .as_ref()
                .map_or_else(|| "none".to_owned(), |f| format!("sample {}", f.sample)),
        ));
    }

    if long_run.from_paper {
        rows.push(figure4_comparison(long_run));
    }
    rows.concat()
}

fn figure4_comparison(long_run: &LongRun) -> String {
    let relevant: Vec<f64> = long_run
        .trajectories
        .iter()
        .filter_map(|t| t.final_relevant().map(Summary::mean))
        .collect();
    let irrelevant_max: Vec<f64> = long_run
        .trajectories
        .iter()
        .filter_map(Trajectory::final_irrelevant_max)
        .collect();
    let across_seeds = Summary::new(&relevant);
    let worst_irrelevant = irrelevant_max.iter().copied().reduce(f64::max);
    let within_spread =
        across_seeds.is_some_and(|s| (s.mean() - PAPER_RELEVANT).abs() <= PAPER_RELEVANT_SPREAD);
    let under_bound = worst_irrelevant.is_some_and(|m| m < PAPER_IRRELEVANT_MAX);

    format!(
        "\n### Against Figure 4\n\n\
         Sutton reports that after 250,000 samples the fifteen irrelevant step sizes were all \
         below {PAPER_IRRELEVANT_MAX} and the five relevant ones were {PAPER_RELEVANT} +/- \
         {PAPER_RELEVANT_SPREAD}.\n\n\
         - Measured relevant step size, averaged within a run and then across the {seeds} seeds: \
         {measured_relevant}. That is {relevant_verdict} the reported interval.\n\
         - Largest irrelevant step size seen in any seed: {measured_irrelevant}. That is \
         {irrelevant_verdict} the reported bound.\n\n\
         The shaded band in the plot marks the reported interval and the dashed lines mark the \
         two reported values. The same protocol differences as Experiment 1 apply: the paper \
         leaves its generator, seed and initial signs unstated, and it reports one run where \
         this records {seeds}.\n\n",
        seeds = long_run.trajectories.len(),
        measured_relevant = plus_minus(across_seeds),
        relevant_verdict = if within_spread { "inside" } else { "outside" },
        measured_irrelevant = worst_irrelevant.map_or_else(|| "-".to_owned(), magnitude),
        irrelevant_verdict = if under_bound { "under" } else { "above" },
    )
}

fn sweep_section(sweep: &[Point]) -> String {
    let lowest = best(sweep);
    let mut rows = vec![format!(
        "## Figure 5 comparison: a fixed step size on the relevant inputs\n\n\
         ![Fixed step size against error](figure5.svg)\n\n\
         Sutton's Figure 5 sweeps a fixed learning rate for the relevant inputs while the \
         irrelevant ones are held at zero, and reports a minimum near {PAPER_FIGURE5_MINIMUM} \
         +/- 0.01. The run below reproduces that setup directly: LMS over the {GROUP_SIZE} \
         relevant inputs only, with {SWEEP_WARMUP} warm-up and {SWEEP_MEASURED} measured \
         samples on seeds {first}..={last}.\n\n\
         | alpha | mean MSE | standard error | failed |\n| --- | --- | --- | --- |\n",
        first = SWEEP_SEEDS.first().copied().unwrap_or_default(),
        last = SWEEP_SEEDS.last().copied().unwrap_or_default(),
    )];
    for point in sweep {
        let summary = point.summary();
        rows.push(format!(
            "| {} | {} | {} | {} |\n",
            parameter(point.value),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.mean())),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.standard_error())),
            point.failures()
        ));
    }
    match lowest {
        Some(point) => rows.push(format!(
            "\nLowest measured value on this grid: alpha {}, MSE {}.\n\n",
            parameter(point.value),
            plus_minus(point.summary())
        )),
        None => rows.push("\nNo grid value completed every seed.\n\n".to_owned()),
    }
    rows.concat()
}

fn files_section() -> String {
    "## Files\n\n\
     - `trajectories.csv`: one row per run, seed and checkpoint, with the relevant mean, range \
     and the irrelevant mean and maximum.\n\
     - `final-step-sizes.csv`: one row per run, seed and input, with its step size at the end of \
     the run.\n\
     - `figure5-sweep.csv`: one row per fixed step size and seed.\n\
     - `figure4.svg`, `adapted-demonstration.svg`, `figure5.svg`: the plots above.\n"
        .to_owned()
}
