//! Scenario E01: IDBD against a tuned fixed-step LMS baseline on four seeded tasks.
//!
//! Each learner's one free parameter is selected on tuning seeds that no evaluation
//! seed shares. Both learners are then measured on the same evaluation streams.

use std::process::ExitCode;

use idbd::Bounds;
use idbd_lab::cli::{arguments, options_or_help};
use idbd_lab::csv::{Csv, number};
use idbd_lab::evaluation::{
    EVALUATION_SEEDS, IDBD_GRID, IDBD_INITIAL_STEP_SIZE, LMS_GRID, MEASURED, TASKS, TUNING_SEEDS,
    WARMUP, tune_and_evaluate,
};
use idbd_lab::report::{diverged_note, magnitude, parameter, plus_minus, provenance, write_all};
use idbd_lab::stats::Summary;
use idbd_lab::sweep::{Family, Point, paired_difference};
use idbd_lab::world::{GROUP_SIZE, NUM_FEATURES, Preset, RELEVANCE_SHIFT_PERIOD, SIGN_FLIP_PERIOD};

const INITIAL_STEP_SIZE: f64 = IDBD_INITIAL_STEP_SIZE;

const DEFAULT_OUT: &str = "docs/results/evaluation";

/// Everything measured for one task.
#[derive(Debug)]
struct TaskResult {
    preset: Preset,
    tuning: Vec<(Family, Vec<Point>)>,
    selected: Vec<(Family, f64)>,
    evaluation: Vec<(Family, Point)>,
}

impl TaskResult {
    fn point(&self, wanted: Family) -> Option<&Point> {
        self.evaluation
            .iter()
            .find(|(family, _)| family.learner_id() == wanted.learner_id())
            .map(|(_, point)| point)
    }

    fn selected_value(&self, wanted: Family) -> Option<f64> {
        self.selected
            .iter()
            .find(|(family, _)| family.learner_id() == wanted.learner_id())
            .map(|(_, value)| *value)
    }
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
    let Some(options) = options_or_help("evaluate", arguments(), DEFAULT_OUT)? else {
        return Ok(());
    };

    eprintln!("{}", provenance());
    let mut results = Vec::new();
    for preset in TASKS {
        results.push(evaluate(preset)?);
    }

    let out = options.out();
    write_all(
        out,
        &[
            ("raw.csv", raw_csv(&results)),
            ("summary.csv", summary_csv(&results)),
            ("README.md", readme(&results)),
        ],
    )?;
    Ok(())
}

fn families() -> [(Family, &'static [f64]); 2] {
    [
        (Family::Lms, &LMS_GRID),
        (
            Family::Idbd {
                initial_step_size: INITIAL_STEP_SIZE,
                bounds: Bounds::none(),
            },
            &IDBD_GRID,
        ),
    ]
}

fn evaluate(preset: Preset) -> Result<TaskResult, String> {
    let mut tuning = Vec::new();
    let mut selected = Vec::new();
    let mut evaluation = Vec::new();
    for (family, grid) in families() {
        eprintln!("tuning {} on {}", family.learner_id(), preset.id());
        let tuned = tune_and_evaluate(family, grid, preset.into())?;
        eprintln!(
            "  selected {} {}, evaluation MSE {} over {} seeds, {} failed",
            family.parameter(),
            parameter(tuned.selected),
            plus_minus(tuned.evaluation.summary()),
            tuned.evaluation.summary().map_or(0, Summary::count),
            tuned.evaluation.failures()
        );
        tuning.push((family, tuned.tuning));
        selected.push((family, tuned.selected));
        evaluation.push((family, tuned.evaluation));
    }
    Ok(TaskResult {
        preset,
        tuning,
        selected,
        evaluation,
    })
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
        for (family, points) in &result.tuning {
            for point in points {
                rows(&mut csv, "tuning", result.preset, *family, point);
            }
        }
        for (family, point) in &result.evaluation {
            rows(&mut csv, "evaluation", result.preset, *family, point);
        }
    }
    csv.into_string()
}

fn rows(csv: &mut Csv, phase: &str, preset: Preset, family: Family, point: &Point) {
    for seed_run in &point.runs {
        csv.row([
            phase.to_owned(),
            preset.id().to_owned(),
            family.learner_id().to_owned(),
            family.parameter().to_owned(),
            number(Some(point.value)),
            seed_run.seed.to_string(),
            WARMUP.to_string(),
            MEASURED.to_string(),
            number(seed_run.mse),
            seed_run
                .failure
                .as_ref()
                .map_or_else(String::new, |f| f.sample.to_string()),
            seed_run
                .failure
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
        "mse_min",
        "mse_max",
    ]);
    for result in results {
        for (family, points) in &result.tuning {
            for point in points {
                summary_row(&mut csv, "tuning", result.preset, *family, point);
            }
        }
        for (family, point) in &result.evaluation {
            summary_row(&mut csv, "evaluation", result.preset, *family, point);
        }
    }
    csv.into_string()
}

fn summary_row(csv: &mut Csv, phase: &str, preset: Preset, family: Family, point: &Point) {
    let completed = point.completed();
    let summary = point.summary();
    csv.row([
        phase.to_owned(),
        preset.id().to_owned(),
        family.learner_id().to_owned(),
        family.parameter().to_owned(),
        number(Some(point.value)),
        point.runs.len().to_string(),
        completed.len().to_string(),
        point.failures().to_string(),
        number(summary.map(Summary::mean)),
        number(summary.map(Summary::standard_error)),
        number(completed.iter().copied().reduce(f64::min)),
        number(completed.iter().copied().reduce(f64::max)),
    ]);
}

fn readme(results: &[TaskResult]) -> String {
    let mut sections = vec![header(), method_section(), overview_section(results)];
    for result in results {
        sections.push(task_section(result));
    }
    sections.push(reading_section());
    sections.push(converged_note(results));
    sections.concat()
}

fn header() -> String {
    format!(
        "# IDBD against a tuned LMS baseline\n\n\
         Scenario E01. Produced by `cargo run --release -p idbd-lab --bin evaluate`.\n\n\
         {}\n\n",
        provenance()
    )
}

fn method_section() -> String {
    format!(
        "## Method\n\n\
         Four seeded tasks, each with {NUM_FEATURES} independent standard normal inputs and a \
         noise-free target. Two learners: fixed-step LMS, and IDBD starting every input at step \
         size {alpha0}. Both run with beta bounds disabled (`Bounds::none()`) and zero initial \
         weights and traces.\n\n\
         - Tuning: every grid value runs on seeds {tune_first}..={tune_last}. The selected value \
         is the one with the lowest mean MSE among grid values whose every tuning run completed.\n\
         - Evaluation: the selected value runs on seeds {eval_first}..={eval_last}, which no \
         tuning run used. Both learners see the identical stream for a given seed, because the \
         task is generated from the seed alone.\n\
         - Each run processes {WARMUP} warm-up samples, then {MEASURED} measured samples. The \
         reported error averages the squared pre-update error over the measured samples only.\n\
         - LMS alpha grid: {lms_grid}.\n\
         - IDBD theta grid: {idbd_grid}.\n\n\
         A failed run is kept in the tables and in `raw.csv` with the sample index and the \
         learner's message. It is never replaced by a substitute value, and a grid value with any \
         failure cannot be selected during tuning.\n\n",
        alpha0 = parameter(INITIAL_STEP_SIZE),
        tune_first = TUNING_SEEDS.first().copied().unwrap_or_default(),
        tune_last = TUNING_SEEDS.last().copied().unwrap_or_default(),
        eval_first = EVALUATION_SEEDS.first().copied().unwrap_or_default(),
        eval_last = EVALUATION_SEEDS.last().copied().unwrap_or_default(),
        lms_grid = grid_list(&LMS_GRID),
        idbd_grid = grid_list(&IDBD_GRID),
    )
}

fn overview_section(results: &[TaskResult]) -> String {
    let mut rows = vec![
        "## Evaluation results\n\nMeasured on the evaluation seeds, after tuning:\n\n\
         | task | LMS alpha | LMS MSE | IDBD theta | IDBD MSE | LMS minus IDBD, paired by seed |\n\
         | --- | --- | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    let idbd = idbd_family();
    for result in results {
        let lms_point = result.point(Family::Lms);
        let idbd_point = result.point(idbd);
        let difference = match (lms_point, idbd_point) {
            (Some(l), Some(i)) => plus_minus(paired_difference(l, i)),
            _ => "-".to_owned(),
        };
        rows.push(format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            result.preset.id(),
            result
                .selected_value(Family::Lms)
                .map_or_else(|| "-".to_owned(), parameter),
            lms_point.map_or_else(|| "-".to_owned(), |p| plus_minus(p.summary())),
            result
                .selected_value(idbd)
                .map_or_else(|| "-".to_owned(), parameter),
            idbd_point.map_or_else(|| "-".to_owned(), |p| plus_minus(p.summary())),
            difference,
        ));
    }
    rows.push(
        "\nA positive difference means IDBD had the lower error on that task. The sign and the \
         size belong to this task, this protocol and these grids, and nothing here establishes \
         an ordering that holds in general.\n\n"
            .to_owned(),
    );
    rows.concat()
}

fn task_section(result: &TaskResult) -> String {
    let idbd = idbd_family();
    let mut parts = vec![format!(
        "## Task `{}`\n\n{}\n\n",
        result.preset.id(),
        description(result.preset)
    )];

    parts.push("Tuning, mean MSE over the tuning seeds:\n\n".to_owned());
    for (family, points) in &result.tuning {
        parts.push(format!(
            "{} ({}):\n\n{}\n",
            family.learner_id().to_uppercase(),
            family.parameter(),
            tuning_table(family.parameter(), points)
        ));
    }

    parts.push("Evaluation, one row per seed:\n\n".to_owned());
    parts.push(seed_table(result.point(Family::Lms), result.point(idbd)));

    let lms_failures = result.point(Family::Lms).map_or(0, Point::failures);
    let idbd_failures = result.point(idbd).map_or(0, Point::failures);
    parts.push(format!(
        "\nFailed evaluation runs: LMS {lms_failures}, IDBD {idbd_failures}.\n\n"
    ));
    parts.push(tuning_divergence(result));
    parts.concat()
}

/// The divergence note for one task's tuning tables.
fn tuning_divergence(result: &TaskResult) -> String {
    let names: Vec<String> = result
        .tuning
        .iter()
        .map(|(family, _)| family.learner_id().to_uppercase())
        .collect();
    let groups: Vec<(&str, &str, &[Point])> = result
        .tuning
        .iter()
        .zip(&names)
        .map(|((family, points), name)| (name.as_str(), family.parameter(), points.as_slice()))
        .collect();
    diverged_note(&groups)
}

fn description(preset: Preset) -> String {
    match preset {
        Preset::Sutton1992 => format!(
            "Sutton (1992), Experiment 1. The first {GROUP_SIZE} inputs are relevant with \
             coefficients +1 or -1 drawn from the seed. One of them, chosen uniformly at random, \
             flips sign before samples 21, 41, 61 and so on, every {SIGN_FLIP_PERIOD} samples. \
             The other {} inputs never affect the target.",
            NUM_FEATURES - GROUP_SIZE
        ),
        Preset::Stationary => format!(
            "The first {GROUP_SIZE} coefficients are +1 or -1 drawn from the seed and never \
             change. Nothing in the task is nonstationary, so a small fixed step size can keep \
             shrinking its error."
        ),
        Preset::Drifting => format!(
            "An extension, not from the paper. The {GROUP_SIZE} relevant coefficients follow \
             independent AR(1) processes with autocorrelation 0.999 and unit stationary variance, \
             so the target relationship moves continuously instead of jumping."
        ),
        Preset::ChangingRelevance => format!(
            "An extension, not from the paper. Sutton's sign flips every {SIGN_FLIP_PERIOD} \
             samples, plus every {RELEVANCE_SHIFT_PERIOD} samples the relevant group moves to the \
             next {GROUP_SIZE} inputs. The previously relevant coefficients become zero and the \
             new group gets fresh signs. Over the measured interval this happens twice."
        ),
    }
}

fn seed_table(lms: Option<&Point>, idbd: Option<&Point>) -> String {
    let mut rows = vec!["| seed | LMS MSE | IDBD MSE |\n| --- | --- | --- |\n".to_owned()];
    for (index, &seed) in EVALUATION_SEEDS.iter().enumerate() {
        rows.push(format!(
            "| {seed} | {} | {} |\n",
            cell(lms, index),
            cell(idbd, index)
        ));
    }
    rows.push(format!(
        "| mean +/- SE | {} | {} |\n",
        lms.map_or_else(|| "-".to_owned(), |p| plus_minus(p.summary())),
        idbd.map_or_else(|| "-".to_owned(), |p| plus_minus(p.summary())),
    ));
    rows.concat()
}

fn cell(point: Option<&Point>, index: usize) -> String {
    let Some(seed_run) = point.and_then(|p| p.runs.get(index)) else {
        return "-".to_owned();
    };
    match (seed_run.mse, &seed_run.failure) {
        (Some(mse), _) => magnitude(mse),
        (None, Some(failure)) => format!("failed at sample {}", failure.sample),
        (None, None) => "-".to_owned(),
    }
}

fn tuning_table(parameter_name: &str, points: &[Point]) -> String {
    let mut rows = vec![format!(
        "| {parameter_name} | mean MSE | standard error | failed |\n| --- | --- | --- | --- |\n"
    )];
    for point in points {
        let summary = point.summary();
        rows.push(format!(
            "| {} | {} | {} | {} |\n",
            parameter(point.value),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.mean())),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.standard_error())),
            point.failures()
        ));
    }
    rows.concat()
}

fn reading_section() -> String {
    format!(
        "## What these numbers do and do not say\n\n\
     Each row measures one learner, tuned once on five streams, on ten other streams of the same \
     task. The comparison is paired: for a given seed both learners consume the same inputs and \
     targets in the same order.\n\n\
     The measured interval starts after {WARMUP} warm-up samples, so the initial transient is \
     excluded on every task. Every error after the warm-up counts, so a learner that recovers \
     slowly from a change inside the measured interval pays for it.\n\n\
     LMS is tuned to one step size for the whole stream. On a task whose difficulty changes \
     within a run, that single value is a compromise, and the comparison measures that \
     compromise rather than LMS in general. Giving LMS a schedule, or tuning it per interval, \
     would be a different experiment.\n\n\
     `raw.csv` holds every run, including the failures. `summary.csv` holds the per-grid-value \
     aggregates for both phases.\n"
    )
}

/// Largest mean squared error still treated as a real measurement. Below it both
/// learners have fit the task and the remaining digits are rounding.
const CONVERGED_BELOW: f64 = 1e-12;

/// Names the tasks where both learners reached the arithmetic floor, so a reader does
/// not read a ratio into two numbers that are both effectively zero.
fn converged_note(results: &[TaskResult]) -> String {
    let converged: Vec<String> = results
        .iter()
        .filter(|result| {
            [Family::Lms, idbd_family()].into_iter().all(|family| {
                result
                    .point(family)
                    .and_then(Point::summary)
                    .is_some_and(|s| s.mean() < CONVERGED_BELOW)
            })
        })
        .map(|result| format!("`{}`", result.preset.id()))
        .collect();
    if converged.is_empty() {
        return String::new();
    }
    format!(
        "\nOn {} both learners drive the measured error below {CONVERGED_BELOW:e}. Both have fit \
         the task, the remaining digits are floating-point rounding, and the difference between \
         them there measures nothing.\n",
        converged.join(" and ")
    )
}

fn idbd_family() -> Family {
    Family::Idbd {
        initial_step_size: INITIAL_STEP_SIZE,
        bounds: Bounds::none(),
    }
}

fn grid_list(grid: &[f64]) -> String {
    grid.iter()
        .map(|&v| parameter(v))
        .collect::<Vec<_>>()
        .join(", ")
}
