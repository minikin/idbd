//! Hand-written SVG: a linear scale and an element builder, with no dependencies.
//!
//! The output is one self-contained file with an explicit background, so it reads the
//! same in a light or dark document viewer.

use std::fmt::Write as _;

/// Maps a data interval onto a pixel interval.
///
/// The pixel interval may run backwards, which is how a y axis puts the smallest value
/// at the bottom of the image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    min: f64,
    max: f64,
    low: f64,
    high: f64,
}

impl Scale {
    /// Creates a scale from a data interval to a pixel interval.
    ///
    /// # Panics
    ///
    /// Panics if the data interval is empty or not finite, which would make every
    /// mapped coordinate meaningless.
    pub fn new(data: (f64, f64), pixels: (f64, f64)) -> Self {
        let (min, max) = data;
        assert!(
            min.is_finite() && max.is_finite() && max > min,
            "data interval {min}..{max} must be finite and non-empty"
        );
        Self {
            min,
            max,
            low: pixels.0,
            high: pixels.1,
        }
    }

    /// Returns the pixel coordinate of `value`, extrapolating outside the interval.
    pub fn at(&self, value: f64) -> f64 {
        let fraction = (value - self.min) / (self.max - self.min);
        self.low + fraction * (self.high - self.low)
    }

    /// Returns the data interval.
    pub const fn domain(&self) -> (f64, f64) {
        (self.min, self.max)
    }
}

/// Horizontal placement of a text anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// Text starts at the given x.
    Start,
    /// Text is centered on the given x.
    Middle,
    /// Text ends at the given x.
    End,
}

impl Anchor {
    const fn attribute(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

/// Accumulates SVG elements and wraps them in a document.
#[derive(Debug, Clone)]
pub struct Svg {
    width: f64,
    height: f64,
    body: String,
}

impl Svg {
    /// Creates an empty document of the given pixel size.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            body: String::new(),
        }
    }

    /// Draws a filled rectangle.
    pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: &str) {
        self.push(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}"/>"#,
            round(x),
            round(y),
            round(width),
            round(height)
        ));
    }

    /// Draws a straight line. A non-empty `dashes` becomes `stroke-dasharray`.
    pub fn line(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        stroke: &str,
        width: f64,
        dashes: &str,
    ) {
        let dash = if dashes.is_empty() {
            String::new()
        } else {
            format!(r#" stroke-dasharray="{dashes}""#)
        };
        self.push(&format!(
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{stroke}" stroke-width="{}"{dash}/>"#,
            round(from.0),
            round(from.1),
            round(to.0),
            round(to.1),
            round(width)
        ));
    }

    /// Draws an open polyline through `points`. A non-empty `dashes` becomes
    /// `stroke-dasharray`.
    pub fn polyline(&mut self, points: &[(f64, f64)], stroke: &str, width: f64, dashes: &str) {
        if points.is_empty() {
            return;
        }
        let mut path = String::new();
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 {
                path.push(' ');
            }
            write!(path, "{},{}", round(x), round(y)).expect("writing to a String cannot fail");
        }
        let dash = if dashes.is_empty() {
            String::new()
        } else {
            format!(r#" stroke-dasharray="{dashes}""#)
        };
        self.push(&format!(
            r#"<polyline points="{path}" fill="none" stroke="{stroke}" stroke-width="{}" stroke-linejoin="round"{dash}/>"#,
            round(width)
        ));
    }

    /// Draws a filled circle.
    pub fn circle(&mut self, center: (f64, f64), radius: f64, fill: &str) {
        self.push(&format!(
            r#"<circle cx="{}" cy="{}" r="{}" fill="{fill}"/>"#,
            round(center.0),
            round(center.1),
            round(radius)
        ));
    }

    /// Draws text, escaping the content.
    pub fn text(&mut self, at: (f64, f64), anchor: Anchor, size: f64, fill: &str, content: &str) {
        self.push(&format!(
            r#"<text x="{}" y="{}" text-anchor="{}" font-size="{}" fill="{fill}">{}</text>"#,
            round(at.0),
            round(at.1),
            anchor.attribute(),
            round(size),
            escape(content)
        ));
    }

    /// Draws text rotated a quarter turn counter-clockwise, for a y axis label.
    pub fn vertical_text(
        &mut self,
        at: (f64, f64),
        anchor: Anchor,
        size: f64,
        fill: &str,
        content: &str,
    ) {
        let (x, y) = (round(at.0), round(at.1));
        self.push(&format!(
            r#"<text x="{x}" y="{y}" text-anchor="{}" font-size="{}" fill="{fill}" transform="rotate(-90 {x} {y})">{}</text>"#,
            anchor.attribute(),
            round(size),
            escape(content)
        ));
    }

    /// Returns the finished document.
    pub fn into_string(self) -> String {
        format!(
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" "#,
                r#"viewBox="0 0 {} {}" font-family="system-ui, -apple-system, Segoe UI, Helvetica, Arial, sans-serif">"#,
                "\n{}</svg>\n"
            ),
            round(self.width),
            round(self.height),
            round(self.width),
            round(self.height),
            self.body
        )
    }

    fn push(&mut self, element: &str) {
        self.body.push_str("  ");
        self.body.push_str(element);
        self.body.push('\n');
    }
}

/// Chooses an axis upper bound and tick step covering `0..=max` in three to eight
/// steps, using only 1, 2 or 5 times a power of ten.
///
/// # Panics
///
/// Panics unless `max` is finite, strictly positive and within `1e-9..1e30`.
pub fn nice_axis(max: f64) -> (f64, f64) {
    assert!(
        max.is_finite() && max > 0.0,
        "axis maximum {max} must be finite and positive"
    );
    let mut step = 1e-9_f64;
    for _ in 0..40 {
        for factor in [1.0, 2.0, 5.0] {
            let candidate = step * factor;
            if (3.0..=8.0).contains(&(max / candidate)) {
                return ((max / candidate).ceil() * candidate, candidate);
            }
        }
        step *= 10.0;
    }
    panic!("axis maximum {max} is outside the supported range");
}

/// Formats an axis tick with the decimals the tick step needs.
///
/// [`nice_axis`] can return a step of 0.5 or 0.05, which a fixed `{:.0}` would print as
/// 0, 0, 1, 2, 2 because halves round to even.
pub fn tick_label(tick: f64, step: f64) -> String {
    let mut decimals = 0_usize;
    let mut scaled = step.abs();
    while decimals < 9 && (scaled - scaled.round()).abs() > 1e-9 {
        scaled *= 10.0;
        decimals += 1;
    }
    format!("{tick:.decimals$}")
}

/// Escapes the five XML entities so labels cannot break the document.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Formats a coordinate with two decimals, trimming a trailing zero fraction.
fn round(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_labels_keep_the_decimals_their_step_needs() {
        assert_eq!(tick_label(2.0, 2.0), "2");
        assert_eq!(tick_label(0.5, 0.5), "0.5");
        assert_eq!(tick_label(1.5, 0.5), "1.5");
        assert_eq!(tick_label(0.15, 0.05), "0.15");
        assert_eq!(tick_label(0.002, 0.002), "0.002");
        // Every step nice_axis can return keeps its ticks distinct.
        for max in [0.004_f64, 0.03, 0.4, 4.0, 40.0, 4e5] {
            let (upper, step) = nice_axis(max);
            let labels: Vec<String> = (0..=8)
                .map(|i| tick_label(f64::from(i) * step, step))
                .collect();
            let unique: std::collections::BTreeSet<&String> = labels.iter().collect();
            assert_eq!(labels.len(), unique.len(), "max {max}: {labels:?}");
            assert!(upper >= max, "upper {upper} covers {max}");
        }
    }

    #[test]
    fn scales_map_the_endpoints() {
        let x = Scale::new((0.0, 0.1), (60.0, 560.0));
        assert!((x.at(0.0) - 60.0).abs() < 1e-9, "{}", x.at(0.0));
        assert!((x.at(0.1) - 560.0).abs() < 1e-9, "{}", x.at(0.1));
        assert!((x.at(0.05) - 310.0).abs() < 1e-9, "{}", x.at(0.05));
        assert_eq!(x.domain(), (0.0, 0.1));
    }

    #[test]
    fn a_reversed_pixel_interval_puts_the_minimum_last() {
        let y = Scale::new((0.0, 5.0), (400.0, 40.0));
        assert!((y.at(0.0) - 400.0).abs() < 1e-9, "{}", y.at(0.0));
        assert!((y.at(5.0) - 40.0).abs() < 1e-9, "{}", y.at(5.0));
        assert!(y.at(1.0) > y.at(4.0), "larger values sit higher");
    }

    #[test]
    fn values_outside_the_domain_extrapolate() {
        let x = Scale::new((1.0, 2.0), (0.0, 10.0));
        assert!((x.at(3.0) - 20.0).abs() < 1e-9, "{}", x.at(3.0));
        assert!((x.at(0.0) + 10.0).abs() < 1e-9, "{}", x.at(0.0));
    }

    #[test]
    #[should_panic(expected = "must be finite and non-empty")]
    fn an_empty_domain_is_rejected() {
        let _ = Scale::new((1.0, 1.0), (0.0, 10.0));
    }

    #[test]
    fn labels_are_xml_escaped() {
        assert_eq!(escape("a & b"), "a &amp; b");
        assert_eq!(escape("<alpha>"), "&lt;alpha&gt;");
        assert_eq!(escape(r#"say "it""#), "say &quot;it&quot;");
        assert_eq!(escape("it's"), "it&apos;s");
        assert_eq!(escape("plain"), "plain");
    }

    #[test]
    fn coordinates_lose_their_trailing_zeros() {
        assert_eq!(round(12.0), "12");
        assert_eq!(round(12.5), "12.5");
        assert_eq!(round(12.345), "12.35");
        assert_eq!(round(-0.004), "-0");
    }

    #[test]
    fn a_document_wraps_its_elements() {
        let mut svg = Svg::new(100.0, 50.0);
        svg.rect(0.0, 0.0, 100.0, 50.0, "#ffffff");
        svg.polyline(&[(0.0, 0.0), (10.0, 20.0)], "#ff6f61", 2.0, "");
        svg.text((5.0, 5.0), Anchor::Middle, 12.0, "#111111", "x < y");
        let document = svg.into_string();
        assert!(document.starts_with("<svg xmlns="), "{document}");
        assert!(document.ends_with("</svg>\n"), "{document}");
        assert!(document.contains(r#"viewBox="0 0 100 50""#), "{document}");
        assert!(document.contains("x &lt; y"), "{document}");
        assert!(document.contains(r#"points="0,0 10,20""#), "{document}");
    }

    #[test]
    fn nice_axes_cover_the_data_in_three_to_eight_steps() {
        for max in [0.02_f64, 0.1, 1.0, 3.6, 5.0, 12.0, 97.0, 1_234.0, 0.000_7] {
            let (upper, step) = nice_axis(max);
            assert!(upper >= max, "{max} needs an upper bound at or above it");
            let steps = upper / step;
            assert!(
                (3.0..=8.000_001).contains(&steps),
                "max {max} gave {steps} steps of {step}"
            );
            assert!(
                upper - max < step,
                "max {max} left more than one step of headroom"
            );
        }
    }

    #[test]
    #[should_panic(expected = "must be finite and positive")]
    fn a_zero_axis_maximum_is_rejected() {
        let _ = nice_axis(0.0);
    }

    #[test]
    fn an_empty_polyline_draws_nothing() {
        let mut svg = Svg::new(10.0, 10.0);
        svg.polyline(&[], "#000000", 1.0, "");
        assert!(!svg.into_string().contains("polyline"));
    }
}
