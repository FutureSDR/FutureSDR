#[test]
fn removed_circuit_close_operator_is_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/connect_less_operator.rs");
}
