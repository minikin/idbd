//! Scenario E07: the Family Assistant home behind the story page's game.
//!
//! The LMS step size and the IDBD meta step size are selected by mean squared error on
//! the tuning seeds, then reported on the evaluation seeds. Besides the error, every
//! learner is scored on its bars: how well the hours its weights mark match the hours in
//! which each sensor truly adds to the power draw.

use std::process::ExitCode;

use idbd_lab::cli::{arguments, options_or_help};
use idbd_lab::csv::{Csv, number};
use idbd_lab::evaluation::{EVALUATION_SEEDS, IDBD_INITIAL_STEP_SIZE, TUNING_SEEDS};
use idbd_lab::family::{
    DAYS, FAMILY_LMS_STEP_SIZE, FAMILY_THETA, FOUND_FLOOR, FOUND_FRACTION, GridPoint, HOURS,
    LMS_GRID, Learner, NOISE, NUM_SENSORS, READINGS_PER_HOUR, Run, SENSORS, THETA_GRID, run,
    select,
};
use idbd_lab::report::{magnitude, parameter, plus_minus, provenance, write_all};
use idbd_lab::stats::Summary;

const DEFAULT_OUT: &str = "docs/results/family";

/// One learner's selection and evaluation.
struct Evaluated {
    name: &'static str,
    parameter_name: &'static str,
    grid: Vec<GridPoint>,
    selected_index: usize,
    selected: f64,
    runs: Vec<(u64, Run)>,
}

fn main() -> ExitCode {
    match main_result() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn main_result() -> Result<(), String> {
    let Some(options) = options_or_help("family-eval", arguments(), DEFAULT_OUT)? else {
        return Ok(());
    };
    eprintln!("{}", provenance());
    eprintln!("selecting and evaluating LMS");
    let lms = evaluate("LMS", "alpha", &LMS_GRID, |step_size| Learner::Lms {
        step_size,
    })?;
    eprintln!("selecting and evaluating IDBD");
    let idbd = evaluate("IDBD", "theta", &THETA_GRID, |theta| Learner::Idbd {
        theta,
    })?;
    check_page_constants(idbd.selected, lms.selected)?;
    eprintln!("selecting and evaluating LMS on rescaled readings");
    let rescaled = evaluate("LMS, rescaled readings", "alpha", &LMS_GRID, |step_size| {
        Learner::RescaledLms { step_size }
    })?;
    let learners = [lms, idbd, rescaled];
    write_all(
        options.out(),
        &[
            ("tuning.csv", tuning_csv(&learners)),
            ("evaluation.csv", evaluation_csv(&learners)),
            ("bars.csv", bars_csv(&learners)),
            ("README.md", readme(&learners)),
        ],
    )
}

fn evaluate(
    name: &'static str,
    parameter_name: &'static str,
    grid: &[f64],
    learner: fn(f64) -> Learner,
) -> Result<Evaluated, String> {
    let (points, best) = select(grid, &TUNING_SEEDS, learner)?;
    let selected = points[best].value;
    let runs = EVALUATION_SEEDS
        .iter()
        .map(|&seed| run(seed, learner(selected)).map(|r| (seed, r)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{name} at {parameter_name} {}: {e}", parameter(selected)))?;
    Ok(Evaluated {
        name,
        parameter_name,
        grid: points,
        selected_index: best,
        selected,
        runs,
    })
}

/// Fails the run if the selection no longer matches the constants the story page uses,
/// so the page cannot cite this scenario for parameters it did not select.
fn check_page_constants(theta: f64, step_size: f64) -> Result<(), String> {
    for (what, selected, constant) in [
        ("IDBD theta", theta, FAMILY_THETA),
        ("LMS alpha", step_size, FAMILY_LMS_STEP_SIZE),
    ] {
        if (selected - constant).abs() > f64::EPSILON {
            return Err(format!(
                "{what} selected on the tuning seeds is {}, but idbd_lab::family hardcodes {}. \
                 The story page cites this scenario, so update the constant or explain the gap.",
                parameter(selected),
                parameter(constant)
            ));
        }
    }
    Ok(())
}

fn relevant() -> impl Iterator<Item = usize> {
    (0..NUM_SENSORS).filter(|&i| !SENSORS[i].hours.is_empty())
}

fn idle() -> impl Iterator<Item = usize> {
    (0..NUM_SENSORS).filter(|&i| SENSORS[i].hours.is_empty())
}

fn relevant_overlap(run: &Run) -> f64 {
    let found = run.bars.found();
    let sum: f64 = relevant().map(|i| found[i].overlap(SENSORS[i].hours)).sum();
    sum / 4.0
}

fn idle_peak(run: &Run) -> f64 {
    idle()
        .flat_map(|i| run.bars.bar(i))
        .fold(0.0, |max: f64, v| max.max(v.abs()))
}

fn mean_speed(run: &Run, sensors: impl Iterator<Item = usize>) -> f64 {
    let values: Vec<f64> = sensors.map(|i| run.speeds[i]).collect();
    values.iter().sum::<f64>() / f64::from(u32::try_from(values.len()).expect("few sensors"))
}

fn summary(learner: &Evaluated, metric: fn(&Run) -> f64) -> Option<Summary> {
    let values: Vec<f64> = learner.runs.iter().map(|(_, r)| metric(r)).collect();
    Summary::new(&values)
}

fn tuning_csv(learners: &[Evaluated]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "learner",
        "parameter",
        "value",
        "seeds",
        "failures",
        "mse_mean",
        "selected",
    ]);
    for learner in learners {
        for (index, point) in learner.grid.iter().enumerate() {
            csv.row([
                learner.name.to_owned(),
                learner.parameter_name.to_owned(),
                number(Some(point.value)),
                TUNING_SEEDS.len().to_string(),
                point.failures.to_string(),
                number(point.mean_squared_error),
                (index == learner.selected_index).to_string(),
            ]);
        }
    }
    csv.into_string()
}

fn evaluation_csv(learners: &[Evaluated]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "learner",
        "parameter",
        "value",
        "seed",
        "mse",
        "bar_score",
        "relevant_overlap",
        "idle_peak_kw",
        "relevant_speed",
        "idle_speed",
    ]);
    for learner in learners {
        for (seed, run) in &learner.runs {
            csv.row([
                learner.name.to_owned(),
                learner.parameter_name.to_owned(),
                number(Some(learner.selected)),
                seed.to_string(),
                number(Some(run.mean_squared_error)),
                number(Some(run.bars.score())),
                number(Some(relevant_overlap(run))),
                number(Some(idle_peak(run))),
                number(Some(mean_speed(run, relevant()))),
                number(Some(mean_speed(run, idle()))),
            ]);
        }
    }
    csv.into_string()
}

fn bars_csv(learners: &[Evaluated]) -> String {
    let mut csv = Csv::new();
    let mut header = vec!["learner".to_owned(), "seed".to_owned(), "sensor".to_owned()];
    header.extend((0..HOURS).map(|h| format!("h{h:02}")));
    csv.row(header);
    for learner in learners {
        for (seed, run) in &learner.runs {
            for (i, sensor) in SENSORS.iter().enumerate() {
                let mut row = vec![
                    learner.name.to_owned(),
                    seed.to_string(),
                    sensor.name.to_owned(),
                ];
                row.extend(run.bars.bar(i).iter().map(|v| number(Some(*v))));
                csv.row(row);
            }
        }
    }
    csv.into_string()
}

fn readme(learners: &[Evaluated]) -> String {
    [
        header(),
        method_section(),
        selection_section(learners),
        results_section(learners),
        reading_section(),
    ]
    .concat()
}

fn header() -> String {
    format!(
        "# Scenario E07: the Family Assistant home\n\n\
         Generated by `cargo run --release -p idbd-lab --bin family-eval`. {}.\n\n",
        provenance()
    )
}

fn sensor_table() -> String {
    let mut rows = vec![
        "| Sensor | Reading scale | Adds to the power draw | kW per standard deviation |\n\
         | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    for sensor in &SENSORS {
        let hours = (0..HOURS)
            .filter(|&h| sensor.hours.contains(h))
            .collect::<Vec<_>>();
        let when = match (hours.first(), hours.last()) {
            (Some(first), Some(last)) => format!("{first:02}:00 to {:02}:00", last + 1),
            _ => "never".to_owned(),
        };
        rows.push(format!(
            "| {} | {} | {when} | {} |\n",
            sensor.name,
            parameter(sensor.scale),
            parameter(sensor.influence)
        ));
    }
    rows.concat()
}

fn method_section() -> String {
    format!(
        "## Method\n\n\
         The home in `idbd_lab::family` has {NUM_SENSORS} sensors. Each reading is an \
         independent standard normal times the sensor's own reading scale. The target is the \
         power draw above the always-on base, in kW: the sum, over the sensors active at that \
         hour, of the influence times the standardized reading, plus Gaussian noise with \
         standard deviation {noise}. The noise alone gives a mean squared error of {floor}.\n\n\
         {table}\n\
         One reading arrives per simulated minute, {READINGS_PER_HOUR} an hour, for {DAYS} days. \
         The error is averaged from the start of the second day. Every learner starts from \
         zero weights, and IDBD starts every step size at {initial}.\n\n\
         A learner's bar for a sensor and an hour is its weight, scaled to kW per standard \
         deviation of the reading and averaged over the readings of that hour on all {DAYS} \
         days. The bar marks an hour as found when it reaches {fraction} of the sensor's peak, \
         unless the peak stays below {found_floor} kW, in which case the sensor is found to \
         never matter. A sensor's score is the overlap of the found hours with the true hours: \
         shared hours over hours in either. The bar score pools that over all {NUM_SENSORS} \
         sensors: hours marked right on any sensor over hours marked by the bars or the truth. \
         Leaving every sensor empty scores 0 and every wrongly marked hour lowers the score.\n\n\
         The LMS step size and the IDBD meta step size are each selected by mean squared error \
         on tuning seeds {tuning}, then reported on evaluation seeds {first} to {last}. A grid \
         value with any failed run is not selectable. Learning speeds are reported per sensor \
         as `alpha_i * scale_i^2`, the step size in units of a standardized reading.\n\n",
        noise = parameter(NOISE),
        floor = parameter(NOISE * NOISE),
        table = sensor_table(),
        initial = parameter(IDBD_INITIAL_STEP_SIZE),
        fraction = parameter(FOUND_FRACTION),
        found_floor = parameter(FOUND_FLOOR),
        tuning = TUNING_SEEDS.map(|s| s.to_string()).join(", "),
        first = EVALUATION_SEEDS[0],
        last = EVALUATION_SEEDS[EVALUATION_SEEDS.len() - 1],
    )
}

fn selection_section(learners: &[Evaluated]) -> String {
    let mut text = vec!["## Selection\n\n".to_owned()];
    for learner in learners {
        let cells: Vec<String> = learner
            .grid
            .iter()
            .map(|p| {
                let value = p
                    .mean_squared_error
                    .map_or_else(|| "failed".to_owned(), magnitude);
                let failed = if p.failures > 0 {
                    format!(", {} failed", p.failures)
                } else {
                    String::new()
                };
                format!("{} {}{failed}", parameter(p.value), value)
            })
            .collect();
        text.push(format!(
            "{} {}: selected {}. Mean squared error per grid value: {}.\n\n",
            learner.name,
            learner.parameter_name,
            parameter(learner.selected),
            cells.join(", ")
        ));
    }
    text.concat()
}

fn results_section(learners: &[Evaluated]) -> String {
    let mut text = vec![
        "## Results on the evaluation seeds\n\n\
         | Learner | Parameter | Mean squared error | Bar score | Overlap on the 4 sensors that matter | Highest bar on a sensor that never matters, kW | Speed, sensors that matter | Speed, sensors that never matter |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    for learner in learners {
        text.push(format!(
            "| {} | {} {} | {} | {} | {} | {} | {} | {} |\n",
            learner.name,
            learner.parameter_name,
            parameter(learner.selected),
            plus_minus(summary(learner, |r| r.mean_squared_error)),
            plus_minus(summary(learner, |r| r.bars.score())),
            plus_minus(summary(learner, relevant_overlap)),
            plus_minus(summary(learner, idle_peak)),
            plus_minus(summary(learner, |r| mean_speed(r, relevant()))),
            plus_minus(summary(learner, |r| mean_speed(r, idle()))),
        ));
    }
    text.push("\nEvery value is a mean over the evaluation seeds with one standard error. `bars.csv` holds every bar.\n\n".to_owned());
    text.concat()
}

fn reading_section() -> String {
    "## What these numbers do and do not say\n\n\
     The sensors read on different scales, and one fixed step size cannot suit all of them: \
     it is slow on the sensors with small readings and noisy on those with large ones. IDBD \
     gives every sensor its own step size. The third row is a control: LMS on readings \
     divided by each sensor's scale, so every reading has a standard deviation of 1, as the \
     `idbd` README recommends. With the scales evened out, a tuned fixed step size draws bars \
     as good as IDBD's and its error comes close. On this home IDBD's edge comes from the \
     different reading scales.\n\n\
     Bars trail the true hours. A weight needs time to grow after its sensor starts to \
     matter and to fade after it stops, so a found range can start or end an hour late, most \
     often on sensors with small readings.\n\n\
     Step sizes do not mark hours on this task: a sensor that matters at dinner every day \
     keeps a high step size around the clock. The bars therefore come from the weights, and \
     the step sizes are reported per sensor.\n\n\
     The home is synthetic. The learners predict the power draw from the readings and \
     switch nothing on or off.\n"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tuning_seeds_select_the_page_constants() {
        let (lms, best_lms) = select(&LMS_GRID, &TUNING_SEEDS, |step_size| Learner::Lms {
            step_size,
        })
        .expect("a value completes");
        let (idbd, best_idbd) = select(&THETA_GRID, &TUNING_SEEDS, |theta| Learner::Idbd { theta })
            .expect("a value completes");
        assert_eq!(
            check_page_constants(idbd[best_idbd].value, lms[best_lms].value),
            Ok(())
        );
    }

    #[test]
    fn a_moved_selection_fails_the_run_and_names_the_constant() {
        let error = check_page_constants(0.1, FAMILY_LMS_STEP_SIZE).expect_err("moved");
        assert!(
            error.starts_with("IDBD theta selected on the tuning seeds is 0.1,"),
            "{error}"
        );
        let error = check_page_constants(FAMILY_THETA, 0.5).expect_err("moved");
        assert!(error.starts_with("LMS alpha"), "{error}");
    }

    #[test]
    fn the_sensor_table_names_the_hours_or_never() {
        let table = sensor_table();
        assert!(
            table.contains("| Oven temperature | 2 | 17:00 to 20:00 | 1 |"),
            "{table}"
        );
        assert!(table.contains("| Barometer | 0.5 | never | 0 |"), "{table}");
    }
}
