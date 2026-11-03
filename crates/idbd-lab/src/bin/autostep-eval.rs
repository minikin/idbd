//! Scenario E05: Autostep against tuned LMS and tuned IDBD on the E01 tasks.
//!
//! Uses the E01 protocol from `idbd_lab::evaluation` unchanged. Autostep runs twice per
//! task: once with mu tuned on the tuning seeds like the other learners, and once at the
//! paper's default mu = 0.01 without any tuning, which is the claim Autostep makes. A
//! second part runs Sutton's Experiment 2 task for 250,000 samples and records the final
//! step sizes of the relevant and irrelevant inputs.

use std::fmt::Write as _;
use std::process::ExitCode;

use idbd::Bounds;
use idbd::autostep::{self, Autostep};
use idbd_lab::cli::{Invocation, arguments, parse};
use idbd_lab::csv::{Csv, number};
use idbd_lab::evaluation::{
    EVALUATION_SEEDS, IDBD_GRID, IDBD_INITIAL_STEP_SIZE, LMS_GRID, MEASURED, TASKS, TUNING_SEEDS,
    Tuned, WARMUP, protocol, tune_and_evaluate,
};
use idbd_lab::report::{magnitude, parameter, plus_minus, provenance, write};
use idbd_lab::stats::Summary;
use idbd_lab::sweep::{Family, Point, measure, paired_difference};
use idbd_lab::world::{GROUP_SIZE, NUM_FEATURES, Preset, World};

/// The initial step size the paper used on its idealized problems: 0.1 / n.
const INITIAL_STEP_SIZE: f64 = 0.1 / 20.0;

/// The paper's default meta step size.
const DEFAULT_MU: f64 = autostep::Config::DEFAULT_META_STEP_SIZE;

/// The paper's default time scale, used for every Autostep run here.
const TIME_SCALE: f64 = autostep::Config::DEFAULT_TIME_SCALE;

/// Meta step sizes offered to the tuning procedure. Includes the default.
const MU_GRID: [f64; 8] = [0.0, 0.001, 0.003, 0.01, 0.03, 0.1, 0.3, 1.0];

/// Samples in the step-size run on Sutton's task, as in Experiment 2.
const STEP_SIZE_SAMPLES: u64 = 250_000;

/// Seeds of the step-size run.
const STEP_SIZE_SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

const DEFAULT_OUT: &str = "docs/results/autostep";

const _: () = assert!(NUM_FEATURES == 20, "INITIAL_STEP_SIZE assumes 20 inputs");

struct TaskResult {
    preset: Preset,
    lms: Tuned,
    idbd: Tuned,
    autostep: Tuned,
    default: Point,
}

struct StepSizes {
    seed: u64,
    relevant: Vec<f64>,
    irrelevant: Vec<f64>,
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
            println!("autostep-eval [--out <dir>]\n\ndefault --out {DEFAULT_OUT}");
            return Ok(());
        }
        Invocation::Run(options) => options,
    };
    eprintln!("{}", provenance());
    let mut results = Vec::new();
    for preset in TASKS {
        eprintln!("evaluating {}", preset.id());
        results.push(evaluate(preset)?);
    }
    let step_sizes: Vec<StepSizes> = STEP_SIZE_SEEDS.iter().map(|&s| step_sizes(s)).collect();

    let out = options.out();
    write(out, "raw.csv", &raw_csv(&results))?;
    write(out, "summary.csv", &summary_csv(&results))?;
    write(out, "step-sizes.csv", &step_sizes_csv(&step_sizes))?;
    write(out, "README.md", &readme(&results, &step_sizes))?;
    Ok(())
}

const fn autostep_family() -> Family {
    Family::Autostep {
        initial_step_size: INITIAL_STEP_SIZE,
        time_scale: TIME_SCALE,
    }
}

fn evaluate(preset: Preset) -> Result<TaskResult, String> {
    let lms = tune_and_evaluate(Family::Lms, &LMS_GRID, preset.into())?;
    let idbd = tune_and_evaluate(
        Family::Idbd {
            initial_step_size: IDBD_INITIAL_STEP_SIZE,
            bounds: Bounds::none(),
        },
        &IDBD_GRID,
        preset.into(),
    )?;
    let autostep = tune_and_evaluate(autostep_family(), &MU_GRID, preset.into())?;
    let default = measure(
        autostep_family(),
        &protocol(preset.into()),
        DEFAULT_MU,
        &EVALUATION_SEEDS,
    )?;
    Ok(TaskResult {
        preset,
        lms,
        idbd,
        autostep,
        default,
    })
}

fn step_sizes(seed: u64) -> StepSizes {
    let config = autostep::Config::new(INITIAL_STEP_SIZE)
        .and_then(|c| c.with_time_scale(TIME_SCALE))
        .expect("the constants form a valid configuration");
    let mut learner = Autostep::new(NUM_FEATURES, config).expect("NUM_FEATURES is nonzero");
    let mut world = World::new(Preset::Sutton1992, seed);
    let mut x = [0.0; NUM_FEATURES];
    let mut events = Vec::new();
    let mut failure = None;
    for t in 1..=STEP_SIZE_SAMPLES {
        let target = world.next_sample(&mut x, &mut events);
        if let Err(e) = learner.update(&x, target) {
            failure = Some(format!("sample {t}: {e}"));
            break;
        }
    }
    let (relevant, irrelevant) = learner.step_sizes().split_at(GROUP_SIZE);
    StepSizes {
        seed,
        relevant: relevant.to_vec(),
        irrelevant: irrelevant.to_vec(),
        failure,
    }
}

fn learners(result: &TaskResult) -> [(&'static str, &Point); 4] {
    [
        ("lms", &result.lms.evaluation),
        ("idbd", &result.idbd.evaluation),
        ("autostep-tuned", &result.autostep.evaluation),
        ("autostep-default", &result.default),
    ]
}

fn parameter_name(label: &str) -> &'static str {
    match label {
        "lms" => "alpha",
        "idbd" => "theta",
        _ => "mu",
    }
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
        for tuned in [&result.lms, &result.idbd, &result.autostep] {
            for point in &tuned.tuning {
                rows(
                    &mut csv,
                    "tuning",
                    result.preset,
                    tuned.family.learner_id(),
                    point,
                );
            }
        }
        for (label, point) in learners(result) {
            rows(&mut csv, "evaluation", result.preset, label, point);
        }
    }
    csv.into_string()
}

fn rows(csv: &mut Csv, phase: &str, preset: Preset, label: &str, point: &Point) {
    for run in &point.runs {
        csv.row([
            phase.to_owned(),
            preset.id().to_owned(),
            label.to_owned(),
            parameter_name(label).to_owned(),
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
        for tuned in [&result.lms, &result.idbd, &result.autostep] {
            for point in &tuned.tuning {
                summary_row(
                    &mut csv,
                    "tuning",
                    result.preset,
                    tuned.family.learner_id(),
                    point,
                );
            }
        }
        for (label, point) in learners(result) {
            summary_row(&mut csv, "evaluation", result.preset, label, point);
        }
    }
    csv.into_string()
}

fn summary_row(csv: &mut Csv, phase: &str, preset: Preset, label: &str, point: &Point) {
    let summary = point.summary();
    csv.row([
        phase.to_owned(),
        preset.id().to_owned(),
        label.to_owned(),
        parameter_name(label).to_owned(),
        number(Some(point.value)),
        point.runs.len().to_string(),
        point.completed().len().to_string(),
        point.failures().to_string(),
        number(summary.map(Summary::mean)),
        number(summary.map(Summary::standard_error)),
    ]);
}

fn step_sizes_csv(runs: &[StepSizes]) -> String {
    let mut csv = Csv::new();
    csv.row(["seed", "input", "relevant", "final_step_size", "failure"]);
    for run in runs {
        let all = run.relevant.iter().map(|&a| (true, a));
        let all = all.chain(run.irrelevant.iter().map(|&a| (false, a)));
        for (input, (relevant, alpha)) in all.enumerate() {
            csv.row([
                run.seed.to_string(),
                input.to_string(),
                relevant.to_string(),
                number(Some(alpha)),
                run.failure.clone().unwrap_or_default(),
            ]);
        }
    }
    csv.into_string()
}

fn readme(results: &[TaskResult], step_sizes: &[StepSizes]) -> String {
    [
        header(),
        method_section(),
        overview_section(results),
        sensitivity_section(results),
        step_size_section(step_sizes),
        reading_section(),
    ]
    .concat()
}

fn header() -> String {
    format!(
        "# Autostep against tuned LMS and tuned IDBD\n\n\
         Scenario E05. Produced by `cargo run --release -p idbd-lab --bin autostep-eval`.\n\n\
         {}\n\n",
        provenance()
    )
}

fn method_section() -> String {
    format!(
        "## Method\n\n\
         The tasks, seeds, warm-up, measured window and the LMS and IDBD grids are those of \
         [scenario E01](../evaluation/README.md): {WARMUP} warm-up samples, then the squared \
         pre-update error averaged over {MEASURED} samples, with parameters selected on seeds \
         {tune_first}..={tune_last} and reported on seeds {eval_first}..={eval_last}. The LMS \
         and IDBD numbers below come from rerunning that procedure and match E01.\n\n\
         Autostep (Mahmood, Sutton, Degris and Pilarski, 2012) starts every input at step size \
         {alpha0}, the 0.1 / n rule the paper used on its idealized problems, with tau = \
         {tau}. It runs twice per task:\n\n\
         - **tuned**: mu selected from {mu_grid} on the tuning seeds, exactly like the LMS and \
         IDBD parameters.\n\
         - **default**: mu = {mu}, the value the paper recommends for every problem, with no \
         tuning at all.\n\n\
         A failed run is kept in the tables with its sample and message, and a grid value with \
         any failed tuning run cannot be selected.\n\n",
        tune_first = TUNING_SEEDS.first().copied().unwrap_or_default(),
        tune_last = TUNING_SEEDS.last().copied().unwrap_or_default(),
        eval_first = EVALUATION_SEEDS.first().copied().unwrap_or_default(),
        eval_last = EVALUATION_SEEDS.last().copied().unwrap_or_default(),
        alpha0 = parameter(INITIAL_STEP_SIZE),
        tau = parameter(TIME_SCALE),
        mu_grid = grid_list(&MU_GRID),
        mu = parameter(DEFAULT_MU),
    )
}

fn overview_section(results: &[TaskResult]) -> String {
    let mut parts = vec![
        "## Evaluation results\n\n\
         Mean MSE +/- standard error over the evaluation seeds. The selected parameter is in \
         parentheses.\n\n\
         | task | LMS | IDBD | Autostep, tuned | Autostep, default |\n\
         | --- | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    for result in results {
        parts.push(format!(
            "| `{}` | {} ({}) | {} ({}) | {} ({}) | {} |\n",
            result.preset.id(),
            cell(&result.lms.evaluation),
            parameter(result.lms.selected),
            cell(&result.idbd.evaluation),
            parameter(result.idbd.selected),
            cell(&result.autostep.evaluation),
            parameter(result.autostep.selected),
            cell(&result.default),
        ));
    }
    parts.push(
        "\nPaired by seed, Autostep at the default mu minus each tuned baseline. A negative \
         value means Autostep had the lower error on that task.\n\n\
         | task | default minus LMS | default minus IDBD | default minus tuned Autostep |\n\
         | --- | --- | --- | --- |\n"
            .to_owned(),
    );
    for result in results {
        parts.push(format!(
            "| `{}` | {} | {} | {} |\n",
            result.preset.id(),
            plus_minus(paired_difference(&result.default, &result.lms.evaluation)),
            plus_minus(paired_difference(&result.default, &result.idbd.evaluation)),
            plus_minus(paired_difference(
                &result.default,
                &result.autostep.evaluation
            )),
        ));
    }
    parts.push("\n".to_owned());
    parts.concat()
}

fn cell(point: &Point) -> String {
    let failures = point.failures();
    if failures == 0 {
        plus_minus(point.summary())
    } else {
        format!("{}, {failures} failed", plus_minus(point.summary()))
    }
}

fn sensitivity_section(results: &[TaskResult]) -> String {
    let mut parts = vec![
        "## Sensitivity to mu\n\n\
         Mean MSE over the tuning seeds for each mu. The paper's claim is that one mu works \
         across problems, so this table shows how much the error moves with mu on each task.\n\n"
            .to_owned(),
    ];
    let mut header = "| mu |".to_owned();
    let mut rule = "| --- |".to_owned();
    for result in results {
        let _ = write!(header, " `{}` |", result.preset.id());
        rule.push_str(" --- |");
    }
    parts.push(format!("{header}\n{rule}\n"));
    for (index, &mu) in MU_GRID.iter().enumerate() {
        let mut row = format!("| {} |", parameter(mu));
        for result in results {
            let point = &result.autostep.tuning[index];
            let _ = write!(row, " {} |", cell(point));
        }
        parts.push(format!("{row}\n"));
    }
    let failed: Vec<&str> = results
        .iter()
        .flat_map(|r| &r.autostep.tuning)
        .flat_map(|p| &p.runs)
        .filter_map(|run| run.failure.as_ref().map(|f| f.reason.as_str()))
        .collect();
    let at_step_size = failed
        .iter()
        .filter(|reason| reason.contains("step size stage"))
        .count();
    parts.push(format!(
        "\n{} tuning runs failed, {at_step_size} of them at the step-size stage. That stage \
         fails when a step size leaves the finite, strictly positive range. A large mu lets a \
         step size shrink by up to exp(-mu) per update until it underflows to zero, and the \
         crate reports that instead of continuing with a step size that could never grow \
         again.\n\n",
        failed.len()
    ));
    parts.concat()
}

fn step_size_section(runs: &[StepSizes]) -> String {
    let mut parts = vec![format!(
        "## Step sizes on Sutton's task\n\n\
         Autostep at the default mu, {STEP_SIZE_SAMPLES} samples of task `sutton1992`, the \
         length of Sutton's Experiment 2. The first {GROUP_SIZE} inputs are relevant. Final \
         step sizes per seed, in `step-sizes.csv`:\n\n\
         | seed | relevant: mean | relevant: min to max | irrelevant: mean | irrelevant: min to max |\n\
         | --- | --- | --- | --- | --- |\n"
    )];
    for run in runs {
        if let Some(failure) = &run.failure {
            parts.push(format!("| {} | failed at {failure} | | | |\n", run.seed));
            continue;
        }
        parts.push(format!(
            "| {} | {} | {} | {} | {} |\n",
            run.seed,
            mean(&run.relevant),
            range(&run.relevant),
            mean(&run.irrelevant),
            range(&run.irrelevant),
        ));
    }
    parts.push("\n".to_owned());
    parts.concat()
}

fn mean(values: &[f64]) -> String {
    Summary::new(values).map_or_else(|| "-".to_owned(), |s| magnitude(s.mean()))
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
     Each number belongs to this protocol, these grids and these synthetic tasks, which have \
     20 standard normal inputs and a noise-free target. Autostep's initial step size and \
     tau were not tuned. Nothing here establishes that one learner is better in general.\n"
        .to_owned()
}

fn grid_list(grid: &[f64]) -> String {
    grid.iter()
        .map(|&v| parameter(v))
        .collect::<Vec<_>>()
        .join(", ")
}
