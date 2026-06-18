use anyhow::Result;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::runtime::dev::Tag;

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

#[test]
fn named_any_equals_clone() {
    let tag = Tag::NamedAny("test".to_string(), Box::new(42_i32));

    assert_eq!(tag, tag.clone());
}

#[test]
fn named_any_equal_when_name_type_and_value_match() {
    assert_eq!(
        Tag::NamedAny("test".to_string(), Box::new(42_i32)),
        Tag::NamedAny("test".to_string(), Box::new(42_i32))
    );
}

#[test]
fn named_any_not_equal_when_value_differs() {
    assert_ne!(
        Tag::NamedAny("test".to_string(), Box::new(42_i32)),
        Tag::NamedAny("test".to_string(), Box::new(43_i32))
    );
}

#[test]
fn named_any_not_equal_when_type_differs() {
    assert_ne!(
        Tag::NamedAny("test".to_string(), Box::new(42_i32)),
        Tag::NamedAny("test".to_string(), Box::new(42_i64))
    );
}

#[test]
fn named_any_not_equal_when_name_differs() {
    assert_ne!(
        Tag::NamedAny("left".to_string(), Box::new(42_i32)),
        Tag::NamedAny("right".to_string(), Box::new(42_i32))
    );
}

#[test]
fn item_tag_named_any_equality() {
    assert_eq!(
        ItemTag {
            index: 7,
            tag: Tag::NamedAny("test".to_string(), Box::new(42_i32)),
        },
        ItemTag {
            index: 7,
            tag: Tag::NamedAny("test".to_string(), Box::new(42_i32)),
        }
    );
}
