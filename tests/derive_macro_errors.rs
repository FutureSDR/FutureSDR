#[test]
fn normal_add_rejects_non_send_blocks() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/non_send_normal_add.rs");
}
