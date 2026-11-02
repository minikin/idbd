//! Command-line parsing for the lab binaries, using only the standard library.

use std::path::{Path, PathBuf};

/// Where a binary writes its recorded results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    out: PathBuf,
}

impl Options {
    /// Returns the output directory.
    pub fn out(&self) -> &Path {
        &self.out
    }
}

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Run and write results into the given directory.
    Run(Options),
    /// Print usage and exit.
    Help,
}

/// Parses `--out <dir>` and `--help` from `args`, which must exclude the program name.
///
/// `default_out` is used when `--out` is absent, so every binary has a documented
/// default location under `docs/`.
///
/// # Errors
///
/// Returns a message naming the offending argument for an unknown flag, a positional
/// argument, or `--out` without a value.
pub fn parse<I, S>(args: I, default_out: &str) -> Result<Invocation, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Option<PathBuf> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "-h" | "--help" => return Ok(Invocation::Help),
            "--out" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--out needs a directory".to_owned())?;
                out = Some(PathBuf::from(value.as_ref()));
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    Ok(Invocation::Run(Options {
        out: out.unwrap_or_else(|| PathBuf::from(default_out)),
    }))
}

/// Returns the arguments of the current process, excluding the program name.
pub fn arguments() -> Vec<String> {
    std::env::args().skip(1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Invocation, String> {
        parse(args.iter().copied(), "docs/results/default")
    }

    #[test]
    fn no_arguments_uses_the_documented_default() {
        let Ok(Invocation::Run(options)) = run(&[]) else {
            panic!("an empty command line is valid");
        };
        assert_eq!(options.out(), Path::new("docs/results/default"));
    }

    #[test]
    fn out_overrides_the_default() {
        let Ok(Invocation::Run(options)) = run(&["--out", "/tmp/elsewhere"]) else {
            panic!("--out with a value is valid");
        };
        assert_eq!(options.out(), Path::new("/tmp/elsewhere"));
    }

    #[test]
    fn help_wins_over_other_arguments() {
        assert_eq!(run(&["--help"]), Ok(Invocation::Help));
        assert_eq!(run(&["-h", "--out", "x"]), Ok(Invocation::Help));
    }

    #[test]
    fn bad_command_lines_name_the_problem() {
        assert_eq!(run(&["--out"]), Err("--out needs a directory".to_owned()));
        assert_eq!(
            run(&["--seeds", "5"]),
            Err("unexpected argument --seeds".to_owned())
        );
        assert_eq!(run(&["extra"]), Err("unexpected argument extra".to_owned()));
    }
}
