use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::runtime::copy_tag_propagation;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Mocker;
use futuresdr::runtime::Tag;

#[test]
fn tags_through_mock() -> Result<()> {
    let mut noop = Apply::<_, f32, f32>::new_typed(|x| *x);
    noop.sio.set_tag_propagation(Box::new(copy_tag_propagation));

    let mut mock = Mocker::new(noop);
    let input = vec![0.0_f32; 1024];
    let tags = vec![
        ItemTag {
            index: 0,
            tag: Tag::Id(0),
        },
        ItemTag {
            index: 256,
            tag: Tag::Id(256),
        },
        ItemTag {
            index: 555,
            tag: Tag::Id(555),
        },
    ];

    mock.input_with_tags(0, input, tags.clone());
    mock.init_output::<f32>(0, 1024);
    mock.run();

    let out_tags = mock.output_tags::<f32>(0);

    assert_eq!(out_tags.len(), 3);

    for (i, tag) in tags.iter().enumerate() {
        assert_eq!(out_tags[i].index, tag.index);
        let Tag::Id(t) = &tag.tag else { unreachable!() };
        assert!(matches!(out_tags[i].tag, Tag::Id(t)));
    }

    Ok(())
}
