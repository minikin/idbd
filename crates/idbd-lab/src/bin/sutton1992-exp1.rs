//! Scenario E04: an approximate replication of Sutton (1992), Experiment 1, Figure 3.
//!
//! Sweeps the fixed LMS step size and the IDBD meta step size over recorded grids on
//! the paper's tracking task, with 20 seeds per setting, and writes the raw per-seed
//! results, a summary and a two-axis plot.

use std::process::ExitCode;

use idbd::Bounds;
use idbd_lab::cli::{Invocation, arguments, parse};
use idbd_lab::csv::{Csv, number};
use idbd_lab::plot::{Anchor, Scale, Svg, nice_axis};
use idbd_lab::report::{magnitude, parameter, plus_minus, provenance, write};
use idbd_lab::stats::Summary;
use idbd_lab::sweep::{Family, Point, Protocol, best, measure, paired_difference};
use idbd_lab::world::{GROUP_SIZE, NUM_FEATURES, Preset, SIGN_FLIP_PERIOD};

/// Fixed LMS step sizes. Sutton's Figure 3 runs its LMS axis from 0 to about 0.09.
const LMS_GRID: [f64; 11] = [
    0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.10,
];

/// IDBD meta step sizes. Sutton's Figure 3 runs its theta axis from 0 to 0.02.
const IDBD_GRID: [f64; 11] = [
    0.0, 0.0005, 0.001, 0.002, 0.004, 0.006, 0.008, 0.010, 0.012, 0.015, 0.020,
];

/// Seeds for the measured runs. Every learner at every grid value sees these streams.
const SEEDS: [u64; 20] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
];

/// Starting step size of every IDBD input, as in the paper.
const INITIAL_STEP_SIZE: f64 = 0.05;

const WARMUP: u64 = 20_000;
const MEASURED: u64 = 10_000;

const DEFAULT_OUT: &str = "docs/results/sutton1992-experiment1";

/// Best mean MSE Sutton reports for LMS in Experiment 1.
const PAPER_LMS_MSE: f64 = 3.5;

/// Mean MSE Sutton reports for IDBD over a broad range of theta in Experiment 1.
const PAPER_IDBD_MSE: f64 = 1.5;

/// Largest mean the plot's y axis covers. A grid value above it is drawn as a caret at
/// the top edge and read from the table instead, so one diverging step size cannot
/// flatten the rest of the figure.
const DISPLAY_MAX: f64 = 12.0;

const IDBD_COLOR: &str = "#ff6f61";
const LMS_COLOR: &str = "#2b6cb0";
const INK: &str = "#14181f";
const MUTED: &str = "#5c6673";
const GRID_COLOR: &str = "#d8dee6";
const PAPER_COLOR: &str = "#8a94a3";

const LEFT: f64 = 92.0;
const RIGHT: f64 = 860.0;
const TOP: f64 = 120.0;
const BOTTOM: f64 = 436.0;
const WIDTH: f64 = 900.0;
const HEIGHT: f64 = 584.0;

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
            println!("sutton1992-exp1 [--out <dir>]\n\ndefault --out {DEFAULT_OUT}");
            return Ok(());
        }
        Invocation::Run(options) => options,
    };

    let protocol = Protocol {
        preset: Preset::Sutton1992,
        warmup: WARMUP,
        measured: MEASURED,
    };
    let idbd_family = Family::Idbd {
        initial_step_size: INITIAL_STEP_SIZE,
        bounds: Bounds::none(),
    };

    eprintln!("{}", provenance());
    eprintln!(
        "protocol: {} seeds x ({WARMUP} warm-up + {MEASURED} measured) samples, no beta bounds",
        SEEDS.len()
    );

    let lms = sweep(Family::Lms, &protocol, &LMS_GRID)?;
    let idbd = sweep(idbd_family, &protocol, &IDBD_GRID)?;

    let out = options.out();
    write(out, "raw.csv", &raw_csv(&lms, &idbd))?;
    write(out, "summary.csv", &summary_csv(&lms, &idbd))?;
    write(out, "figure3.svg", &figure(&lms, &idbd))?;
    write(out, "README.md", &readme(&lms, &idbd))?;
    Ok(())
}

fn sweep(family: Family, protocol: &Protocol, grid: &[f64]) -> Result<Vec<Point>, String> {
    let mut points = Vec::with_capacity(grid.len());
    for &value in grid {
        let point = measure(family, protocol, value, &SEEDS)?;
        let summary = point.summary();
        eprintln!(
            "{} {} {} -> MSE {} over {} runs, {} failed",
            family.learner_id(),
            family.parameter(),
            parameter(value),
            plus_minus(summary),
            summary.map_or(0, Summary::count),
            point.failures()
        );
        points.push(point);
    }
    Ok(points)
}

fn raw_csv(lms: &[Point], idbd: &[Point]) -> String {
    let mut csv = Csv::new();
    csv.row([
        "learner",
        "parameter",
        "parameter_value",
        "initial_step_size",
        "seed",
        "warmup",
        "measured",
        "mse",
        "failure_sample",
        "failure_reason",
    ]);
    let families = [
        ("lms", "alpha", String::new(), lms),
        ("idbd", "theta", number(Some(INITIAL_STEP_SIZE)), idbd),
    ];
    for (learner, parameter_name, initial, points) in families {
        for point in points {
            for seed_run in &point.runs {
                csv.row([
                    learner.to_owned(),
                    parameter_name.to_owned(),
                    number(Some(point.value)),
                    initial.clone(),
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
    }
    csv.into_string()
}

fn summary_csv(lms: &[Point], idbd: &[Point]) -> String {
    let mut csv = Csv::new();
    csv.row([
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
    for (learner, parameter_name, points) in [("lms", "alpha", lms), ("idbd", "theta", idbd)] {
        for point in points {
            let completed = point.completed();
            let summary = point.summary();
            csv.row([
                learner.to_owned(),
                parameter_name.to_owned(),
                number(Some(point.value)),
                SEEDS.len().to_string(),
                completed.len().to_string(),
                point.failures().to_string(),
                number(summary.map(Summary::mean)),
                number(summary.map(Summary::standard_error)),
                number(completed.iter().copied().reduce(f64::min)),
                number(completed.iter().copied().reduce(f64::max)),
            ]);
        }
    }
    csv.into_string()
}

#[derive(Debug)]
struct Curve<'a> {
    points: &'a [Point],
    axis_max: f64,
    color: &'static str,
    label: String,
}

fn figure(lms: &[Point], idbd: &[Point]) -> String {
    let highest = [lms, idbd]
        .into_iter()
        .flatten()
        .filter_map(|p| p.summary().map(|s| s.mean() + s.standard_error()))
        .filter(|&top| top.is_finite() && top <= DISPLAY_MAX)
        .reduce(f64::max)
        .unwrap_or(1.0);
    let (y_upper, y_step) = nice_axis(highest);
    let y = Scale::new((0.0, y_upper), (BOTTOM, TOP + 16.0));

    let mut svg = Svg::new(WIDTH, HEIGHT);
    draw_frame(&mut svg, &y, y_upper, y_step);

    let curves = [
        Curve {
            points: lms,
            axis_max: 0.10,
            color: LMS_COLOR,
            label: "LMS, fixed step size alpha (bottom axis)".to_owned(),
        },
        Curve {
            points: idbd,
            axis_max: 0.020,
            color: IDBD_COLOR,
            label: format!(
                "IDBD, meta step size theta (top axis), initial alpha {}",
                parameter(INITIAL_STEP_SIZE)
            ),
        },
    ];
    for (index, curve) in curves.iter().enumerate() {
        draw_curve(&mut svg, curve, &y, index);
    }

    draw_axis(
        &mut svg,
        &Scale::new((0.0, 0.10), (LEFT, RIGHT)),
        AxisSide::Bottom,
        LMS_COLOR,
        "LMS step size alpha",
        &[0.0, 0.02, 0.04, 0.06, 0.08, 0.10],
    );
    draw_axis(
        &mut svg,
        &Scale::new((0.0, 0.020), (LEFT, RIGHT)),
        AxisSide::Top,
        IDBD_COLOR,
        "IDBD meta step size theta",
        &[0.0, 0.005, 0.010, 0.015, 0.020],
    );

    for (row, note) in [
        "Points are means over the seeds. Bars are one standard error of the mean.",
        "A caret at the top edge marks a mean above the axis. A hollow marker means some seeds failed.",
        &provenance(),
    ]
    .into_iter()
    .enumerate()
    {
        let offset = 62.0 - 18.0 * f64::from(u32::try_from(row).unwrap_or(0));
        svg.text((LEFT, HEIGHT - offset), Anchor::Start, 11.5, MUTED, note);
    }
    svg.into_string()
}

fn draw_frame(svg: &mut Svg, y: &Scale, y_upper: f64, y_step: f64) {
    svg.rect(0.0, 0.0, WIDTH, HEIGHT, "#ffffff");
    svg.text(
        (LEFT, 36.0),
        Anchor::Start,
        19.0,
        INK,
        "Sutton (1992) Experiment 1, approximate replication",
    );
    svg.text(
        (LEFT, 56.0),
        Anchor::Start,
        12.5,
        MUTED,
        &format!(
            "{NUM_FEATURES} inputs, {GROUP_SIZE} relevant, one sign flip every \
             {SIGN_FLIP_PERIOD} samples. {WARMUP} warm-up + {MEASURED} measured samples, \
             {seeds} seeds, no beta bounds.",
            seeds = SEEDS.len()
        ),
    );

    let mut tick = 0.0;
    while tick <= y_upper + y_step / 2.0 {
        let py = y.at(tick);
        svg.line((LEFT, py), (RIGHT, py), GRID_COLOR, 1.0, "");
        svg.text(
            (LEFT - 10.0, py + 4.0),
            Anchor::End,
            12.0,
            MUTED,
            &format!("{tick:.0}"),
        );
        tick += y_step;
    }
    svg.vertical_text(
        (34.0, f64::midpoint(TOP, BOTTOM)),
        Anchor::Middle,
        13.0,
        INK,
        "Mean squared error over the measured samples",
    );

    for (value, label) in [
        (PAPER_LMS_MSE, "Sutton reports best LMS near 3.5"),
        (PAPER_IDBD_MSE, "Sutton reports IDBD near 1.5"),
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

fn draw_curve(svg: &mut Svg, curve: &Curve<'_>, y: &Scale, legend_row: usize) {
    let x = Scale::new((0.0, curve.axis_max), (LEFT, RIGHT));
    let (_, upper) = y.domain();
    let line: Vec<(f64, f64)> = curve
        .points
        .iter()
        .filter_map(|p| {
            p.summary()
                .map(|s| (x.at(p.value), y.at(s.mean().min(upper))))
        })
        .collect();
    svg.polyline(&line, curve.color, 2.4, "");
    for point in curve.points {
        let Some(summary) = point.summary() else {
            continue;
        };
        let px = x.at(point.value);
        if summary.mean() > upper {
            let top = y.at(upper);
            svg.line((px - 5.0, top + 9.0), (px, top + 1.0), curve.color, 2.0, "");
            svg.line((px + 5.0, top + 9.0), (px, top + 1.0), curve.color, 2.0, "");
            continue;
        }
        let low = y.at(summary.mean() - summary.standard_error());
        let high = y.at((summary.mean() + summary.standard_error()).min(upper));
        svg.line((px, low), (px, high), curve.color, 1.4, "");
        svg.line((px - 3.0, low), (px + 3.0, low), curve.color, 1.4, "");
        svg.line((px - 3.0, high), (px + 3.0, high), curve.color, 1.4, "");
        svg.circle((px, y.at(summary.mean())), 4.0, curve.color);
        if point.failures() > 0 {
            svg.circle((px, y.at(summary.mean())), 2.0, "#ffffff");
        }
    }
    let row = f64::from(u32::try_from(legend_row).unwrap_or(0));
    let legend_y = TOP + 22.0 + 20.0 * row;
    svg.circle((LEFT + 18.0, legend_y - 4.0), 4.5, curve.color);
    svg.text(
        (LEFT + 30.0, legend_y),
        Anchor::Start,
        12.5,
        INK,
        &curve.label,
    );
}

#[derive(Debug, Clone, Copy)]
enum AxisSide {
    Top,
    Bottom,
}

fn draw_axis(
    svg: &mut Svg,
    scale: &Scale,
    side: AxisSide,
    color: &str,
    title: &str,
    ticks: &[f64],
) {
    let (baseline, tick_end, label_y, title_y) = match side {
        AxisSide::Bottom => (BOTTOM, BOTTOM + 6.0, BOTTOM + 22.0, BOTTOM + 46.0),
        AxisSide::Top => (TOP, TOP - 6.0, TOP - 12.0, TOP - 32.0),
    };
    svg.line((LEFT, baseline), (RIGHT, baseline), color, 1.8, "");
    for &tick in ticks {
        let px = scale.at(tick);
        svg.line((px, baseline), (px, tick_end), color, 1.4, "");
        svg.text((px, label_y), Anchor::Middle, 12.0, INK, &parameter(tick));
    }
    svg.text(
        (f64::midpoint(LEFT, RIGHT), title_y),
        Anchor::Middle,
        13.5,
        color,
        title,
    );
}

fn readme(lms: &[Point], idbd: &[Point]) -> String {
    [
        header(),
        protocol_section(),
        results_section(lms, idbd),
        best_section(lms, idbd),
        paper_section(),
        failure_section(lms, idbd),
        files_section(),
    ]
    .concat()
}

fn header() -> String {
    format!(
        "# Sutton 1992, Experiment 1: approximate replication\n\n\
         Scenario E04. Produced by `cargo run --release -p idbd-lab --bin sutton1992-exp1`.\n\n\
         {}\n\n",
        provenance()
    )
}

fn protocol_section() -> String {
    format!(
        "## Protocol\n\n\
         - Task: the `{preset}` preset. {NUM_FEATURES} independent standard normal inputs. The \
         first {GROUP_SIZE} are relevant with coefficients +1 or -1, drawn from the seed, and the \
         target is their noise-free weighted sum. One of the five, chosen uniformly at random, \
         flips sign before samples 21, 41, 61 and so on, every {SIGN_FLIP_PERIOD} samples.\n\
         - Each run processes {WARMUP} warm-up samples and then {MEASURED} measured samples. The \
         reported error averages the squared pre-update error over the measured samples only.\n\
         - Seeds {first}..={last}, the same streams for every learner at every grid value. A seed \
         fixes the inputs, the initial coefficient signs and which coefficient flips at each \
         boundary.\n\
         - IDBD starts every input at step size {alpha0} with zero weights and traces, and runs \
         with beta bounds disabled (`Bounds::none()`), as the paper's experiments did.\n\
         - LMS alpha grid: {lms_grid}.\n\
         - IDBD theta grid: {idbd_grid}.\n\n",
        preset = Preset::Sutton1992.id(),
        first = SEEDS.first().copied().unwrap_or_default(),
        last = SEEDS.last().copied().unwrap_or_default(),
        alpha0 = parameter(INITIAL_STEP_SIZE),
        lms_grid = grid_list(&LMS_GRID),
        idbd_grid = grid_list(&IDBD_GRID),
    )
}

fn results_section(lms: &[Point], idbd: &[Point]) -> String {
    format!(
        "## Measured results\n\n\
         ![LMS and IDBD error against their step-size parameters](figure3.svg)\n\n\
         A grid value whose mean exceeds {DISPLAY_MAX} is drawn as a caret at the top edge of \
         the plot, with its value in the table below.\n\n\
         LMS, fixed step size:\n\n{}\nIDBD, meta step size:\n\n{}\n",
        table("alpha", lms),
        table("theta", idbd)
    )
}

fn best_section(lms: &[Point], idbd: &[Point]) -> String {
    let (Some(lms_best), Some(idbd_best)) = (best(lms), best(idbd)) else {
        return "## Best settings\n\nNo grid value completed every seed, so no setting is \
                selectable. The tables above keep the failures.\n\n"
            .to_owned();
    };
    format!(
        "## Best settings\n\n\
         - LMS: alpha {lms_value}, MSE {lms_mse}.\n\
         - IDBD: theta {idbd_value}, MSE {idbd_mse}.\n\n\
         Each is the lowest mean over the {seeds} seeds among grid values whose every run \
         completed. Pairing the two learners seed by seed, IDBD at theta {idbd_value} is \
         {gap} lower than LMS at alpha {lms_value}.\n\n\
         {lms_runner}{idbd_runner}\
         The numbers cover one task, one protocol and these grids. They do not say how either \
         learner behaves on other streams, and the grid spacing bounds how precisely the minima \
         are located.\n\n",
        seeds = SEEDS.len(),
        lms_value = parameter(lms_best.value),
        lms_mse = plus_minus(lms_best.summary()),
        idbd_value = parameter(idbd_best.value),
        idbd_mse = plus_minus(idbd_best.summary()),
        gap = plus_minus(paired_difference(lms_best, idbd_best)),
        lms_runner = runner_up_note("LMS", "alpha", lms, lms_best),
        idbd_runner = runner_up_note("IDBD", "theta", idbd, idbd_best),
    )
}

/// Describes how far the selected grid value is from the next best one, paired by seed,
/// so a reader can see whether the grid resolves the minimum.
fn runner_up_note(learner: &str, parameter_name: &str, points: &[Point], chosen: &Point) -> String {
    let runner_up = points
        .iter()
        .filter(|p| p.failures() == 0)
        .filter(|p| (p.value - chosen.value).abs() > f64::EPSILON)
        .filter_map(|p| p.summary().map(|s| (p, s.mean())))
        .reduce(|low, next| if next.1 < low.1 { next } else { low });
    let Some((point, _)) = runner_up else {
        return String::new();
    };
    let Some(difference) = paired_difference(point, chosen) else {
        return String::new();
    };
    format!(
        "The next best {learner} value is {parameter_name} {}, which is {} higher per seed.\n\n",
        parameter(point.value),
        plus_minus(Some(difference))
    )
}

fn paper_section() -> String {
    format!(
        "## Comparison with the paper\n\n\
         Sutton reports a best LMS mean squared error near {PAPER_LMS_MSE} and an IDBD error near \
         {PAPER_IDBD_MSE} across a broad range of theta, with standard errors under 0.1 \
         (Figure 3). The curves above have the same shape: LMS has one shallow minimum in alpha, \
         and IDBD sits below it across most of the theta grid. The dashed lines in the plot mark \
         the two paper values.\n\n\
         Differences in protocol:\n\n\
         - The paper leaves its random generator, its seed and its initial coefficient signs \
         unstated. Inputs, sign choices and the five initial signs here all come from the \
         generator named above, seeded as recorded.\n\
         - The paper averages one long run per setting. This sweep uses {seeds} independent seeds \
         per setting, so the standard error measures spread across streams rather than within \
         one.\n\
         - The grids above are not the paper's sample points, which are unpublished. A minimum \
         read off eleven grid values is located to within one grid spacing.\n\
         - Arithmetic is IEEE 754 binary64 in Rust, summed in index order. The paper states \
         neither its precision nor its summation order.\n\n\
         These numbers are not expected to match the paper's samples digit for digit, and no \
         claim is made that they do.\n\n",
        seeds = SEEDS.len(),
    )
}

fn failure_section(lms: &[Point], idbd: &[Point]) -> String {
    let failures: usize = lms.iter().chain(idbd).map(Point::failures).sum();
    let total = (LMS_GRID.len() + IDBD_GRID.len()) * SEEDS.len();
    let counted = if failures == 0 {
        format!("## Failures\n\nNo run stopped early. All {total} completed.\n\n")
    } else {
        format!(
            "## Failures\n\n{failures} of {total} runs stopped on a numerical failure. \
             `raw.csv` records the sample index and the learner's message for each one, and \
             `summary.csv` counts them per grid value. A grid value with any failure is not \
             selectable as a best setting, because a mean over the surviving seeds would hide \
             the risk.\n\n"
        )
    };
    [counted, diverged_note(lms, idbd)].concat()
}

/// Names the grid values whose arithmetic stayed finite while the error grew without
/// bound, since they are not failures and not comparable either.
fn diverged_note(lms: &[Point], idbd: &[Point]) -> String {
    let mut named = Vec::new();
    for (learner, parameter_name, points) in [("LMS", "alpha", lms), ("IDBD", "theta", idbd)] {
        for point in points {
            if point.summary().is_some_and(|s| s.mean() > DISPLAY_MAX) {
                named.push(format!(
                    "{learner} at {parameter_name} {}",
                    parameter(point.value)
                ));
            }
        }
    }
    if named.is_empty() {
        return String::new();
    }
    format!(
        "{} ran to the end of the stream without a non-finite value, so the runs are recorded as \
         completed, but the error grew without bound. Treat those rows as divergence, not as a \
         measurement.\n\n",
        named.join(" and ")
    )
}

fn files_section() -> String {
    "## Files\n\n\
     - `raw.csv`: one row per learner, grid value and seed, with the measured MSE or the failure \
     that ended the run.\n\
     - `summary.csv`: one row per learner and grid value, with the mean, standard error, range \
     and failure count.\n\
     - `figure3.svg`: both curves on shared axes, LMS on the bottom x axis and IDBD on the top \
     one.\n"
        .to_owned()
}

fn grid_list(grid: &[f64]) -> String {
    grid.iter()
        .map(|&v| parameter(v))
        .collect::<Vec<_>>()
        .join(", ")
}

fn table(parameter_name: &str, points: &[Point]) -> String {
    let mut rows = vec![format!(
        "| {parameter_name} | mean MSE | standard error | completed | failed |\n\
         | --- | --- | --- | --- | --- |\n"
    )];
    for point in points {
        let summary = point.summary();
        rows.push(format!(
            "| {} | {} | {} | {} | {} |\n",
            parameter(point.value),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.mean())),
            summary.map_or_else(|| "-".to_owned(), |s| magnitude(s.standard_error())),
            summary.map_or(0, Summary::count),
            point.failures()
        ));
    }
    rows.concat()
}
