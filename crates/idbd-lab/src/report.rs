//! Writing recorded results to a directory, and the formatting the reports share.

use std::fs;
use std::path::Path;

use crate::rng::GENERATOR;
use crate::stats::Summary;
use crate::sweep::Point;

/// Version of this crate, recorded in every report so a number can be traced to a build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates `dir` if needed, writes `contents` to `dir/name` and reports the path on
/// standard error.
///
/// # Errors
///
/// Returns a message naming the path if the directory cannot be created or the file
/// cannot be written.
pub fn write(dir: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = dir.join(name);
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    fs::write(&path, contents).map_err(|e| format!("writing {}: {e}", path.display()))?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// Writes every `(name, contents)` pair into `dir` with [`write`], in order.
///
/// # Errors
///
/// Returns the first failure from [`write`]. Files before it stay written.
pub fn write_all(dir: &Path, files: &[(&str, String)]) -> Result<(), String> {
    files
        .iter()
        .try_for_each(|(name, contents)| write(dir, name, contents))
}

/// Returns the line every report carries: crate version and generator identity.
pub fn provenance() -> String {
    format!("idbd-lab {VERSION}, generator {GENERATOR}")
}

/// Formats a summary as `mean +/- standard error`.
///
/// A single measurement has no spread to report, and `+/- 0.000` would read as a
/// measured zero, so it is named instead.
pub fn plus_minus(summary: Option<Summary>) -> String {
    match summary {
        Some(s) if s.count() < 2 => format!("{} (1 seed, no spread)", magnitude(s.mean())),
        Some(s) => format!(
            "{} +/- {}",
            magnitude(s.mean()),
            magnitude(s.standard_error())
        ),
        None => "no completed run".to_owned(),
    }
}

/// Mean squared error above which a completed grid value is reported as divergence. The
/// largest finite steady state in the recorded sweeps is below 1e3 (LMS at alpha 0.09 on
/// Sutton's task, whose mean-square contraction factor `1 - 2 alpha + 22 alpha^2` is 0.998)
/// and the diverged ones are above 1e160, so the bound is far from both.
pub const DIVERGED_ABOVE: f64 = 1e10;

/// Names the grid values whose runs completed while the error grew without bound, so a
/// reader does not take a row like `1.328e177` for a measurement.
///
/// Each group is a learner name, the name of its swept parameter and its points. A point
/// has diverged when its mean is above [`DIVERGED_ABOVE`] or not finite. A value named
/// by more than one group appears once. Returns an empty string when nothing diverged.
pub fn diverged_note(groups: &[(&str, &str, &[Point])]) -> String {
    let mut named: Vec<String> = Vec::new();
    for (learner, parameter_name, points) in groups {
        for point in *points {
            if !diverged(point) {
                continue;
            }
            let name = format!("{learner} at {parameter_name} {}", parameter(point.value));
            if !named.contains(&name) {
                named.push(name);
            }
        }
    }
    if named.is_empty() {
        return String::new();
    }
    format!(
        "{} ran to the end of the stream without a non-finite value, so the runs are \
         recorded as completed, but the error grew without bound: the mean squared error \
         is above {bound:e}. Treat those rows as divergence, not as a measurement.\n\n",
        named.join(" and "),
        bound = DIVERGED_ABOVE,
    )
}

fn diverged(point: &Point) -> bool {
    point
        .summary()
        .is_some_and(|s| !s.mean().is_finite() || s.mean() > DIVERGED_ABOVE)
}

/// Formats a measured value with about four significant digits and at least three
/// decimals, switching to scientific notation outside `0.001..10000` so a converged
/// error and a diverged one are both readable.
pub fn magnitude(value: f64) -> String {
    let abs = value.abs();
    if !value.is_finite() || (abs > 0.0 && !(1e-3..1e4).contains(&abs)) {
        return format!("{value:.3e}");
    }
    let decimals = if abs >= 1.0 {
        3
    } else if abs >= 0.1 {
        4
    } else if abs >= 0.01 {
        5
    } else if abs > 0.0 {
        6
    } else {
        3
    };
    trim_trailing_zeros(&format!("{value:.decimals$}"), MINIMUM_DECIMALS)
}

/// Decimals every formatted measurement keeps, so a table column stays aligned.
const MINIMUM_DECIMALS: usize = 3;

fn trim_trailing_zeros(text: &str, keep: usize) -> String {
    let Some(point) = text.find('.') else {
        return text.to_owned();
    };
    let floor = point + 1 + keep;
    let mut end = text.len();
    while end > floor && text.as_bytes().get(end - 1) == Some(&b'0') {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Formats a parameter value with the smallest number of decimals that distinguishes
/// the grids used here, trimming trailing zeros.
pub fn parameter(value: f64) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0');
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_creates_the_directory_and_the_file() {
        let dir = std::env::temp_dir().join(format!("idbd-lab-report-{}", std::process::id()));
        let nested = dir.join("nested");
        write(&nested, "out.txt", "contents").expect("writable temp directory");
        let read = fs::read_to_string(nested.join("out.txt")).expect("file exists");
        fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(read, "contents");
    }

    #[test]
    fn write_all_writes_every_file() {
        let dir = std::env::temp_dir().join(format!("idbd-lab-report-all-{}", std::process::id()));
        write_all(
            &dir,
            &[("a.txt", "a".to_owned()), ("b.txt", "b".to_owned())],
        )
        .expect("writable temp directory");
        let read =
            ["a.txt", "b.txt"].map(|name| fs::read_to_string(dir.join(name)).expect("file exists"));
        fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(read, ["a", "b"]);
    }

    #[test]
    fn plus_minus_pairs_the_mean_with_its_uncertainty() {
        let summary = Summary::new(&[1.0, 2.0, 3.0]);
        assert_eq!(plus_minus(summary), "2.000 +/- 0.5774");
        assert_eq!(plus_minus(None), "no completed run");
    }

    #[test]
    fn plus_minus_names_a_single_measurement_instead_of_a_zero_spread() {
        let one = Summary::new(&[8.598]);
        assert_eq!(plus_minus(one), "8.598 (1 seed, no spread)");
    }

    fn point(value: f64, mses: &[f64]) -> Point {
        Point {
            value,
            runs: mses
                .iter()
                .enumerate()
                .map(|(index, &mse)| crate::sweep::SeedRun {
                    seed: u64::try_from(index).expect("a test index fits u64") + 1,
                    mse: Some(mse),
                    failure: None,
                })
                .collect(),
        }
    }

    #[test]
    fn diverged_note_names_means_above_the_bound_and_keeps_a_large_steady_state() {
        let large = point(0.09, &[44.0, 597.0]);
        let above = point(0.1, &[1e160, 6e177]);
        let infinite = point(0.2, &[1.0, f64::INFINITY]);
        let points = vec![large, above, infinite];
        let note = diverged_note(&[("LMS", "alpha", &points)]);
        assert!(
            note.starts_with("LMS at alpha 0.1 and LMS at alpha 0.2 ran to the end"),
            "{note}"
        );
        assert!(note.contains("is above 1e10."), "{note}");
    }

    #[test]
    fn diverged_note_is_empty_when_nothing_diverged_and_names_a_value_once() {
        let healthy = vec![point(0.03, &[3.4, 3.5])];
        assert_eq!(diverged_note(&[("LMS", "alpha", &healthy)]), "");
        let at_bound = vec![point(0.09, &[DIVERGED_ABOVE, DIVERGED_ABOVE])];
        assert_eq!(diverged_note(&[("LMS", "alpha", &at_bound)]), "");
        let twice = vec![point(0.1, &[1e16, 2e16])];
        let note = diverged_note(&[("LMS", "alpha", &twice), ("LMS", "alpha", &twice)]);
        assert!(note.starts_with("LMS at alpha 0.1 ran to"), "{note}");
    }

    #[test]
    fn magnitudes_switch_to_scientific_outside_the_readable_range() {
        assert_eq!(magnitude(0.0), "0.000");
        assert_eq!(magnitude(1.7e-9), "1.700e-9");
        assert_eq!(magnitude(2.6e186), "2.600e186");
        assert_eq!(magnitude(f64::INFINITY), "inf");
    }

    #[test]
    fn magnitudes_keep_four_significant_digits_and_three_decimals() {
        assert_eq!(magnitude(3.4021), "3.402");
        assert_eq!(magnitude(2.0), "2.000");
        assert_eq!(magnitude(0.129_13), "0.1291");
        assert_eq!(magnitude(0.017), "0.017");
        assert_eq!(magnitude(0.007_828), "0.007828");
        assert_eq!(magnitude(0.001), "0.001");
        assert_eq!(magnitude(-0.5), "-0.500");
    }

    #[test]
    fn parameters_keep_only_the_digits_they_need() {
        assert_eq!(parameter(0.0), "0");
        assert_eq!(parameter(0.05), "0.05");
        assert_eq!(parameter(0.0005), "0.0005");
        assert_eq!(parameter(0.1), "0.1");
        assert_eq!(parameter(0.012), "0.012");
        assert_eq!(parameter(13.0), "13");
    }

    #[test]
    fn provenance_names_the_crate_and_the_generator() {
        let line = provenance();
        assert!(line.contains(VERSION), "{line}");
        assert!(line.contains("xoshiro256"), "{line}");
    }
}
