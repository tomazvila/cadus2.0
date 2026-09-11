use cadus_core::answer::{AnswerContract, NumericForm, Outcome, check_contract};
fn correct(expected: &str, learner: &str) -> bool {
    let outcome = check_contract(
        expected,
        learner,
        AnswerContract::RequiredForm {
            form: NumericForm::FactoredLinear,
        },
    );
    matches!(outcome, Outcome::Decided(v) if v.correct)
}
#[test]
fn requires_a_greatest_factor_and_primitive_linear_sum() {
    assert!(correct("3(2x+3)", "3*(2x+3)"));
    assert!(correct("-4(x+2)", "-4*(x+2)"));
    assert!(correct("6(2x+3y)", "6*(2x+3y)"));
    assert!(!correct("3*(2x+3)", "6x+9"));
    assert!(!correct("3*(2x+3)", "1*(6x+9)"));
    assert!(!correct("3*(2x+3)", "3*(4x+6)/2"));
    assert!(!correct("-4*(x+2)", "4*(-x-2)"));
    assert!(!correct("3*(2x+3)", "3*(x*y+3)"));
}
