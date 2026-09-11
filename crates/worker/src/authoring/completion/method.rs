//! Conservative instructional scaffolds; independent AI review determines pedagogical completeness.
use crate::authoring::prompt::AuthoringSpec;

pub(super) fn rule(spec: &AuthoringSpec) -> &'static str {
    let name = format!("{} {}", spec.topic_id, spec.kp_name).to_lowercase();
    if name.contains("slope") {
        "Compare the change in the vertical coordinate with the change in the horizontal coordinate, keeping the point order consistent."
    } else if name.contains("system") {
        "A solution must satisfy every equation in the system. Use substitution or elimination to find a candidate, then check it in each original equation."
    } else if name.contains("equation") || name.contains("inequalit") {
        "Keep the two sides equivalent while isolating the requested quantity. For an inequality, reverse its direction when multiplying or dividing both sides by a negative quantity."
    } else if name.contains("percent") {
        "A percentage is a ratio per hundred. Identify the reference whole before converting between a percentage, a fraction and a decimal."
    } else if name.contains("fraction") {
        "A fraction compares its numerator with its denominator. Equivalent forms multiply or divide both by the same nonzero factor; use compatible units when combining fractions."
    } else if name.contains("graph") || name.contains("coordinate") || name.contains("plot") {
        "Read the horizontal coordinate before the vertical coordinate. Relate the plotted points and the axis scales to the relationship stated in the question."
    } else if name.contains("trig") || name.contains("triangle") || name.contains("angle") {
        "Identify the given sides and angles and the quantity requested. Choose the relation that connects those quantities, and keep angle units and side labels consistent."
    } else if name.contains("exponent") || name.contains("power") || name.contains("logarithm") {
        "Identify the base and exponent and apply the rule for the stated operation. Check the domain restrictions before simplifying or undoing a power."
    } else {
        "Evaluate grouped expressions first, then powers, then multiplication and division, and finally addition and subtraction. Keep exact fractions until a rounding instruction requires a decimal."
    }
}

pub(super) fn hints(spec: &AuthoringSpec) -> serde_json::Value {
    serde_json::json!({"hints": [
        format!("For {}, what information is given and what must you find?", spec.kp_name.to_lowercase()),
        rule(spec),
        "Check the proposed result against the original conditions and any requested units or answer format."
    ]})
}
