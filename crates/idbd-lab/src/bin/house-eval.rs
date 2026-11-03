//! Scenario E06: the thermostat house behind the story page.
//!
//! Uses the E01 protocol from `idbd_lab::evaluation`: the LMS step size and the IDBD meta
//! step size are selected on the tuning seeds and reported on the evaluation seeds.
//! Autostep runs at the paper's defaults without tuning. Every learner runs on the house
//! and on the same house with switching turned off. A second part records IDBD's final
//! step size for every sensor at the selected meta step size.

use std::process::ExitCode;

use idbd::Bounds;
use idbd::autostep;
use idbd_lab::cli::{Invocation, arguments, parse};
use idbd_lab::csv::{Csv, number};
use idbd_lab::evaluation::{
    EVALUATION_SEEDS, IDBD_GRID, IDBD_INITIAL_STEP_SIZE, LMS_GRID, MEASURED, TUNING_SEEDS, Tuned,
    WARMUP, protocol, tune_and_evaluate,
};
use idbd_lab::experiment::{RunSpec, run as run_once};
use idbd_lab::house::{LEVELS, NOISE, NUM_SENSORS, RELEVANT, SENSORS, SWITCH_PROBABILITY};
use idbd_lab::report::{magnitude, parameter, plus_minus, provenance, write};
use idbd_lab::stats::Summary;
use idbd_lab::sweep::{Family, Point, measure, paired_difference};
use idbd_lab::task::Task;

const TASKS: [Task; 2] = [Task::House, Task::StillHouse];

/// Autostep's starting step size: the paper's 0.1 / n rule for 6 inputs.
const AUTOSTEP_INITIAL_STEP_SIZE: f64 = 0.1 / 6.0;

const DEFAULT_OUT: &str = "docs/results/house";

const _: () = assert!(
    NUM_SENSORS == 6,
    "AUTOSTEP_INITIAL_STEP_SIZE assumes 6 inputs"
);

struct TaskResult {
    task: Task,
    lms: Tuned,
    idbd: Tuned,
    autostep: Point,
}

struct StepSizes {
    seed: u64,
    sizes: Vec<f64>,
    failure: Option<String>,
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
            println!("house-eval [--out <dir>]\n\ndefault --out {DEFAULT_OUT}");
            return Ok(());
        }
        Invocation::Run(options) => options,
    };
    eprintln!("{}", provenance());
    let mut results = Vec::new();
    for task in TASKS {
        eprintln!("evaluating {}", task.id());
        results.push(evaluate(task)?);
    }
    let theta = results
        .first()
        .map(|r| r.idbd.selected)
        .ok_or("no house result")?;
    let step_sizes = EVALUATION_SEEDS
        .iter()
        .map(|&seed| step_sizes(theta, seed))
        .collect::<Result<Vec<_>, _>>()?;

    let out = options.out();
    write(out, "raw.csv", &raw_csv(&results))?;
    write(out, "summary.csv", &summary_csv(&results))?;
    write(out, "step-sizes.csv", &step_sizes_csv(theta, &step_sizes))?;
    write(out, "README.md", &readme(&results, theta, &step_sizes))?;
    Ok(())
}

const fn idbd_family() -> Family {
    Family::Idbd {
        initial_step_size: IDBD_INITIAL_STEP_SIZE,
        bounds: Bounds::none(),
    }
}

fn evaluate(task: Task) -> Result<TaskResult, String> {
    let lms = tune_and_evaluate(Family::Lms, &LMS_GRID, task)?;
    let idbd = tune_and_evaluate(idbd_family(), &IDBD_GRID, task)?;
    let autostep = measure(
        Family::Autostep {
            initial_step_size: AUTOSTEP_INITIAL_STEP_SIZE,
            time_scale: autostep::Config::DEFAULT_TIME_SCALE,
        },
        &protocol(task),
        autostep::Config::DEFAULT_META_STEP_SIZE,
        &EVALUATION_SEEDS,
    )?;
    Ok(TaskResult {
        task,
        lms,
        idbd,
        autostep,
    })
}

fn step_sizes(theta: f64, seed: u64) -> Result<StepSizes, String> {
    let result = run_once(&RunSpec {
        task: Task::House,
        seed,
        warmup: WARMUP,
        measured: MEASURED,
        learner: idbd_family().learner(theta)?,
    })?;
    Ok(StepSizes {
        seed,
        sizes: result.final_step_sizes,
        failure: result
            .failure
            .map(|(sample, reason)| format!("sample {sample}: {reason}")),
    })
}

fn learners(result: &TaskResult) -> [(&'static str, &'static str, &Point); 3] {
    [
        ("lms", "alpha", &result.lms.evaluation),
        ("idbd", "theta", &result.idbd.evaluation),
        ("autostep-default", "mu", &result.autostep),
    ]
}

fn raw_csv(results: &[TaskResult]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "phase",
        "task",
        "learner",
        "parameter",
        "parameter_value",
        "seed",
        "warmup",
        "measured",
        "mse",
        "failure_sample",
        "failure_reason",
    ]);
    for result in results {
        for tuned in [&result.lms, &result.idbd] {
            for point in &tuned.tuning {
                let family = tuned.family;
                rows(
                    &mut csv,
                    "tuning",
                    result.task,
                    family.learner_id(),
                    family.parameter(),
                    point,
                );
            }
        }
        for (label, name, point) in learners(result) {
            rows(&mut csv, "evaluation", result.task, label, name, point);
        }
    }
    csv.into_string()
}

fn rows(csv: &mut Csv, phase: &str, task: Task, label: &str, name: &str, point: &Point) {
    for run in &point.runs {
        csv.row([
            phase.to_owned(),
            task.id().to_owned(),
            label.to_owned(),
            name.to_owned(),
            number(Some(point.value)),
            run.seed.to_string(),
            WARMUP.to_string(),
            MEASURED.to_string(),
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

fn summary_csv(results: &[TaskResult]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "phase",
        "task",
        "learner",
        "parameter",
        "parameter_value",
        "seeds",
        "completed",
        "failures",
        "mse_mean",
        "mse_standard_error",
    ]);
    for result in results {
        for tuned in [&result.lms, &result.idbd] {
            for point in &tuned.tuning {
                let family = tuned.family;
                summary_row(
                    &mut csv,
                    "tuning",
                    result.task,
                    family.learner_id(),
                    family.parameter(),
                    point,
                );
            }
        }
        for (label, name, point) in learners(result) {
            summary_row(&mut csv, "evaluation", result.task, label, name, point);
        }
    }
    csv.into_string()
}

fn summary_row(csv: &mut Csv, phase: &str, task: Task, label: &str, name: &str, point: &Point) {
    let summary = point.summary();
    csv.row([
        phase.to_owned(),
        task.id().to_owned(),
        label.to_owned(),
        name.to_owned(),
        number(Some(point.value)),
        point.runs.len().to_string(),
        point.completed().len().to_string(),
        point.failures().to_string(),
        number(summary.map(Summary::mean)),
        number(summary.map(Summary::standard_error)),
    ]);
}

fn step_sizes_csv(theta: f64, runs: &[StepSizes]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "seed",
        "theta",
        "sensor",
        "name",
        "relevant",
        "final_step_size",
        "failure",
    ]);
    for run in runs {
        for (sensor, &alpha) in run.sizes.iter().enumerate() {
            csv.row([
                run.seed.to_string(),
                number(Some(theta)),
                sensor.to_string(),
                SENSORS.get(sensor).copied().unwrap_or_default().to_owned(),
                (sensor < RELEVANT).to_string(),
                number(Some(alpha)),
                run.failure.clone().unwrap_or_default(),
            ]);
        }
    }
    csv.into_string()
}

fn readme(results: &[TaskResult], theta: f64, step_sizes: &[StepSizes]) -> String {
    [
        header(),
        method_section(),
        results_section(results),
        step_size_section(theta, step_sizes),
        reading_section(),
    ]
    .concat()
}

fn header() -> String {
    format!(
        "# The thermostat house\n\n\
         Scenario E06. Produced by `cargo run --release -p idbd-lab --bin house-eval`.\n\n\
         {}\n\n",
        provenance()
    )
}

fn method_section() -> String {
    let levels = SENSORS
        .iter()
        .zip(LEVELS)
        .map(|(name, [strong, weak])| {
            format!("{name} {} or {}", parameter(strong), parameter(weak))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "## Method\n\n\
         The house in `idbd_lab::house` has {NUM_SENSORS} sensors with independent standard \
         normal readings. The target is the change in room temperature: the readings' dot \
         product with the current influences, plus Gaussian noise with standard deviation \
         {noise}. Only the first {RELEVANT} sensors have an influence ({levels}). Before every \
         reading after the first, each of them switches between its two levels with \
         probability {p}. The other {inert} sensors never matter. Task `still-house` is the \
         same house with switching turned off, so every relevant sensor keeps its first \
         influence.\n\n\
         The protocol is that of [scenario E01](../evaluation/README.md): {WARMUP} warm-up \
         samples, then the squared pre-update error averaged over {MEASURED} samples. The LMS \
         step size and the IDBD meta step size theta are selected on seeds \
         {tune_first}..={tune_last} from the E01 grids and reported on seeds \
         {eval_first}..={eval_last}. IDBD starts every input at step size {alpha0} with no beta \
         bounds. Autostep starts at {autostep_alpha0} (0.1 / n) with mu = {mu} and tau = \
         {tau}, the paper's defaults, and is not tuned.\n\n",
        noise = parameter(NOISE),
        p = magnitude(SWITCH_PROBABILITY),
        inert = NUM_SENSORS - RELEVANT,
        tune_first = TUNING_SEEDS.first().copied().unwrap_or_default(),
        tune_last = TUNING_SEEDS.last().copied().unwrap_or_default(),
        eval_first = EVALUATION_SEEDS.first().copied().unwrap_or_default(),
        eval_last = EVALUATION_SEEDS.last().copied().unwrap_or_default(),
        alpha0 = parameter(IDBD_INITIAL_STEP_SIZE),
        autostep_alpha0 = parameter(AUTOSTEP_INITIAL_STEP_SIZE),
        mu = parameter(autostep::Config::DEFAULT_META_STEP_SIZE),
        tau = parameter(autostep::Config::DEFAULT_TIME_SCALE),
    )
}

fn results_section(results: &[TaskResult]) -> String {
    let mut parts = vec![
        "## Results\n\n\
         Mean MSE +/- standard error over the evaluation seeds. The selected parameter is in \
         parentheses. The noise alone gives an MSE of 0.25.\n\n\
         | task | LMS | IDBD | Autostep, default |\n\
         | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    for result in results {
        parts.push(format!(
            "| `{}` | {} ({}) | {} ({}) | {} |\n",
            result.task.id(),
            cell(&result.lms.evaluation),
            parameter(result.lms.selected),
            cell(&result.idbd.evaluation),
            parameter(result.idbd.selected),
            cell(&result.autostep),
        ));
    }
    parts.push(
        "\nPaired by seed. A negative difference means the first learner had the lower error. \
         The last column counts the evaluation seeds on which IDBD's error was below LMS's.\n\n\
         | task | IDBD minus LMS | Autostep minus LMS | IDBD lower on |\n\
         | --- | --- | --- | --- |\n"
            .to_owned(),
    );
    for result in results {
        let (wins, paired) = lower_on(&result.idbd.evaluation, &result.lms.evaluation);
        parts.push(format!(
            "| `{}` | {} | {} | {wins} of {paired} seeds |\n",
            result.task.id(),
            plus_minus(paired_difference(
                &result.idbd.evaluation,
                &result.lms.evaluation
            )),
            plus_minus(paired_difference(&result.autostep, &result.lms.evaluation)),
        ));
    }
    parts.push("\n".to_owned());
    parts.concat()
}

/// Counts the seeds on which `a` completed with a lower MSE than `b`, and the seeds on
/// which both completed.
fn lower_on(a: &Point, b: &Point) -> (usize, usize) {
    let pairs: Vec<(f64, f64)> = a
        .runs
        .iter()
        .zip(&b.runs)
        .filter(|(left, right)| left.seed == right.seed)
        .filter_map(|(left, right)| Some((left.mse?, right.mse?)))
        .collect();
    (pairs.iter().filter(|(x, y)| x < y).count(), pairs.len())
}

fn cell(point: &Point) -> String {
    let failures = point.failures();
    if failures == 0 {
        plus_minus(point.summary())
    } else {
        format!("{}, {failures} failed", plus_minus(point.summary()))
    }
}

fn step_size_section(theta: f64, runs: &[StepSizes]) -> String {
    let mut parts = vec![format!(
        "## IDBD's step size per sensor\n\n\
         IDBD at the selected theta = {}, after {} samples of task `house`, one run per \
         evaluation seed. Per-seed values are in `step-sizes.csv`.\n\n\
         | sensor | relevant | mean | min to max |\n\
         | --- | --- | --- | --- |\n",
        parameter(theta),
        WARMUP + MEASURED,
    )];
    let failed: Vec<String> = runs
        .iter()
        .filter_map(|r| r.failure.as_ref().map(|f| format!("seed {}: {f}", r.seed)))
        .collect();
    for (sensor, name) in SENSORS.iter().enumerate() {
        let values: Vec<f64> = runs
            .iter()
            .filter(|r| r.failure.is_none())
            .filter_map(|r| r.sizes.get(sensor).copied())
            .collect();
        parts.push(format!(
            "| {name} | {} | {} | {} |\n",
            if sensor < RELEVANT { "yes" } else { "no" },
            Summary::new(&values).map_or_else(|| "-".to_owned(), |s| magnitude(s.mean())),
            range(&values),
        ));
    }
    if !failed.is_empty() {
        parts.push(format!("\nFailed runs: {}.\n", failed.join("; ")));
    }
    parts.push("\n".to_owned());
    parts.concat()
}

fn range(values: &[f64]) -> String {
    let min = values.iter().copied().reduce(f64::min);
    let max = values.iter().copied().reduce(f64::max);
    match (min, max) {
        (Some(min), Some(max)) => format!("{} to {}", magnitude(min), magnitude(max)),
        _ => "-".to_owned(),
    }
}

fn reading_section() -> String {
    "## Reading these results\n\n\
     The house is a synthetic task built for the story page, not data from a real \
     thermostat. Each number belongs to this protocol, these grids and this task. Nothing \
     here establishes that one learner is better in general.\n"
        .to_owned()
}
