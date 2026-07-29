#[test]
fn buffer_compile_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/local_buffer_cross_domain.rs");
    t.compile_fail("tests/compile_fail/zero_sized_sample.rs");
}
