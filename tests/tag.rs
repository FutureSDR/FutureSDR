use futuresdr::anyhow::Result;
use futuresdr::runtime::Tag;

#[test]
fn tag_any_is() -> Result<()> {
    let tag = Tag::NamedAny("test".to_string(), Box::new(42_i32));
    let Tag::NamedAny(_, value) = tag else {
        unreachable!()
    };
    assert!(value.is::<i32>());
    assert!(!value.is::<f32>());
    Ok(())
}
