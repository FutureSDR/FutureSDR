#[test]
fn local_only_buffer_is_rejected_by_cross_domain_stream_api() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/local_buffer_cross_domain.rs");
}

#[test]
fn zero_sized_samples_are_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/zero_sized_sample.rs");
}
