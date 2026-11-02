//! Minimal RFC 4180 CSV writing, so recorded results need no dependency to produce.

use std::fmt::Write as _;

/// Returns `field` quoted if a comma, double quote, carriage return or newline would
/// otherwise break the record. Interior double quotes are doubled.
pub fn escape(field: &str) -> String {
    if field.contains([',', '"', '\r', '\n']) {
        let mut quoted = String::with_capacity(field.len() + 2);
        quoted.push('"');
        for c in field.chars() {
            if c == '"' {
                quoted.push('"');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted
    } else {
        field.to_owned()
    }
}

/// A CSV document built one record at a time, terminated with `\n`.
#[derive(Debug, Clone, Default)]
pub struct Csv {
    text: String,
}

impl Csv {
    /// Creates an empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one record, escaping each field.
    pub fn row<I, S>(&mut self, fields: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut first = true;
        for field in fields {
            if !first {
                self.text.push(',');
            }
            first = false;
            self.text.push_str(&escape(field.as_ref()));
        }
        self.text.push('\n');
    }

    /// Returns the document.
    pub fn into_string(self) -> String {
        self.text
    }
}

/// Formats `value` with enough digits to reproduce a measurement, or `""` for `None`.
///
/// Seventeen significant digits round-trip every `f64`.
pub fn number(value: Option<f64>) -> String {
    match value {
        Some(v) => {
            let mut text = String::new();
            write!(text, "{v:.17e}").expect("writing to a String cannot fail");
            text
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_fields_are_left_alone() {
        assert_eq!(escape("idbd"), "idbd");
        assert_eq!(escape(""), "");
        assert_eq!(escape("0.05"), "0.05");
    }

    #[test]
    fn separators_and_quotes_are_escaped() {
        assert_eq!(escape("a,b"), "\"a,b\"");
        assert_eq!(escape("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape("line\nbreak"), "\"line\nbreak\"");
        assert_eq!(escape("carriage\rreturn"), "\"carriage\rreturn\"");
    }

    #[test]
    fn rows_are_comma_separated_and_newline_terminated() {
        let mut csv = Csv::new();
        csv.row(["learner", "value", "note"]);
        csv.row(["lms", "0.04", "step size, fixed"]);
        assert_eq!(
            csv.into_string(),
            "learner,value,note\nlms,0.04,\"step size, fixed\"\n"
        );
    }

    #[test]
    fn an_empty_row_still_terminates() {
        let mut csv = Csv::new();
        csv.row(Vec::<String>::new());
        assert_eq!(csv.into_string(), "\n");
    }

    #[test]
    fn numbers_round_trip_and_missing_values_are_empty() {
        let value = 1.234_567_890_123_456_7_e-3_f64;
        let text = number(Some(value));
        let parsed: f64 = text.parse().expect("the recorded form parses back");
        assert!((parsed - value).abs() < f64::EPSILON, "{text}");
        assert_eq!(number(None), "");
    }
}
