//! The SVG render of one visual specification.
//!
//! The render is deterministic: the same specification and the same options
//! always give the same bytes. Every figure carries `role="img"`, a `<title>`,
//! and a `<desc>` that holds [`super::VisualSpec::text_equivalent`], so a screen
//! reader reads the same facts the picture shows.
//!
//! The render draws nothing before [`super::VisualSpec::validate`] passes.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::{LabeledPoint, VisualError, VisualSpec};

mod figure;

use figure::{
    coordinate_body, curve_body, fraction_body, geometry_body, number_line_body,
    special_triangle_body,
};

/// The drawing options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// The prefix of the `id` of the `<title>` and the `<desc>`.
    ///
    /// Two figures on one page need two prefixes, or the `aria-labelledby` of
    /// the second figure points at the title of the first.
    pub id_prefix: String,
    /// The width of the view box.
    pub width: i64,
    /// The height of the view box.
    pub height: i64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            id_prefix: "visual".to_owned(),
            width: 480,
            height: 200,
        }
    }
}

impl RenderOptions {
    /// The options with one prefix.
    #[must_use]
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            id_prefix: prefix.to_owned(),
            ..Self::default()
        }
    }
}

/// Draw one visual as SVG.
///
/// The call validates first. An invalid figure gives the [`VisualError`] and no
/// bytes, because a drawn but wrong picture teaches a wrong fact.
pub fn render(spec: &VisualSpec, options: &RenderOptions) -> Result<String, VisualError> {
    spec.validate()?;
    let body = match spec {
        VisualSpec::NumberLine(figure) => number_line_body(figure, options)?,
        VisualSpec::Fraction(figure) => fraction_body(figure, options),
        VisualSpec::Coordinate(figure) => coordinate_body(figure, options)?,
        VisualSpec::Geometry(figure) => geometry_body(figure, options)?,
        VisualSpec::Curve(figure) => curve_body(figure, options)?,
        VisualSpec::SpecialTriangle(figure) => special_triangle_body(figure, options)?,
    };
    Ok(frame(spec, options, &body))
}

/// One drawn figure, as the API sends it to the browser.
///
/// The browser never repeats the geometry: it prints these bytes and reads this
/// text. `web/src/components/MathVisual.tsx` is the reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedVisual {
    /// The family: `number_line`, `fraction`, `coordinate`, or `geometry`.
    pub kind: String,
    /// The SVG bytes.
    pub svg: String,
    /// The accessible equivalent.
    pub text: String,
}

/// Draw every figure of one problem, and drop each figure the check refuses.
///
/// The drop is the safe direction: a learner reads a question with one picture
/// short, and never a picture that states a wrong fact. The caller reports the
/// missing figure through the readiness audit.
///
/// Each figure takes the prefix `<prefix>-<position>`, so two figures on one page
/// never share the id of a `<title>`.
#[must_use]
pub fn render_all(specs: &[VisualSpec], prefix: &str) -> Vec<RenderedVisual> {
    let mut out = Vec::new();
    for (at, spec) in specs.iter().enumerate() {
        let options = RenderOptions::with_prefix(&format!("{prefix}-{at}"));
        if let Ok(svg) = render(spec, &options) {
            out.push(RenderedVisual {
                kind: spec.kind().to_owned(),
                svg,
                text: spec.text_equivalent(),
            });
        }
    }
    out
}

/// The title of one figure: the caption, or the family name.
fn title_of(spec: &VisualSpec) -> String {
    spec.caption().map(str::trim).map_or_else(
        || match spec {
            VisualSpec::NumberLine(_) => "Number line".to_owned(),
            VisualSpec::Fraction(_) => "Fraction figure".to_owned(),
            VisualSpec::Coordinate(_) => "Coordinate plane".to_owned(),
            VisualSpec::Geometry(_) => "Geometry diagram".to_owned(),
            VisualSpec::Curve(_) => "Curve".to_owned(),
            VisualSpec::SpecialTriangle(_) => "Special right triangle".to_owned(),
        },
        ToOwned::to_owned,
    )
}

/// The SVG element around one body.
fn frame(spec: &VisualSpec, options: &RenderOptions, body: &str) -> String {
    let prefix = &options.id_prefix;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" \
         class=\"cadus-visual cadus-visual-{kind}\" role=\"img\" \
         aria-labelledby=\"{prefix}-title {prefix}-desc\">\
         <title id=\"{prefix}-title\">{title}</title>\
         <desc id=\"{prefix}-desc\">{desc}</desc>{body}</svg>",
        width = options.width,
        height = options.height,
        kind = spec.kind(),
        title = escape(&title_of(spec)),
        desc = escape(&spec.text_equivalent()),
    )
}

/// The XML escape of one text.
pub(super) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// One pixel coordinate, rounded to two places.
///
/// The round keeps the output stable across platforms, and it turns `-0` into
/// `0`, so the bytes never depend on the sign of a zero.
pub(super) fn px(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let settled = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{settled:.2}")
}

/// One tick label: the value with no trailing zero.
pub(super) fn num_text(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    let settled = if rounded == 0.0 { 0.0 } else { rounded };
    let mut text = format!("{settled:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// One `<text>` element.
pub(super) fn text_at(out: &mut String, x: f64, y: f64, anchor: &str, label: &str) {
    let _ = write!(
        out,
        "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" class=\"cadus-visual-label\">{}</text>",
        px(x),
        px(y),
        escape(label)
    );
}

/// One `<line>` element.
pub(super) fn line_at(out: &mut String, from: (f64, f64), to: (f64, f64), class: &str) {
    let _ = write!(
        out,
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"{class}\"/>",
        px(from.0),
        px(from.1),
        px(to.0),
        px(to.1)
    );
}

/// One `<circle>` element.
pub(super) fn dot_at(out: &mut String, at: (f64, f64), radius: f64, class: &str) {
    let _ = write!(
        out,
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" class=\"{class}\"/>",
        px(at.0),
        px(at.1),
        px(radius)
    );
}

/// The label of one point, or an empty text.
pub(super) fn point_label(point: &LabeledPoint) -> &str {
    point
        .label
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or_default()
}

/// The four figure bodies share this margin, in pixels.
pub(super) const MARGIN: f64 = 36.0;

/// The drawn width of one figure.
pub(super) fn inner_width(options: &RenderOptions) -> f64 {
    (options.width as f64 - 2.0 * MARGIN).max(1.0)
}

/// The drawn height of one figure.
pub(super) fn inner_height(options: &RenderOptions) -> f64 {
    (options.height as f64 - 2.0 * MARGIN).max(1.0)
}
