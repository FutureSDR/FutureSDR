use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessageCopy;
use futuresdr::runtime::copy_tag_propagation;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Mocker;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;
use futuresdr::runtime::Tag;
use rand::distr::Uniform;
use rand::Rng;

#[test]
fn multi_input_mock() {
    let input: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(128)
        .collect();

    let block = Apply::new(|x: &u32| x + 1);

    let mut mocker = Mocker::new(block);
    mocker.input(0, input[..64].to_vec());
    mocker.init_output::<u32>(0, 128);
    mocker.run();
    mocker.input(0, input[64..].to_vec());
    mocker.run();
    let (output, _) = mocker.output::<u32>(0);

    assert_eq!(input.len(), output.len());
    for (a, b) in input.iter().zip(output.iter()) {
        assert_eq!(a + 1, *b);
    }
}

#[test]
fn tags_through_mock() -> Result<()> {
    let mut noop = Apply::<_, f32, f32>::new(|x| *x);
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
    mock.init();
    mock.init_output::<f32>(0, input.len() * 2);
    mock.input(0, input.clone());
    mock.run();

    let (out_buffer, out_tags) = mock.output::<f32>(0);
    assert_eq!(out_buffer.len(), 1024);
    assert_eq!(out_tags.len(), 0);

    mock.input_with_tags(0, input, tags.clone());
    mock.run();
    mock.deinit();

    let (out_buffer, out_tags) = mock.output::<f32>(0);
    assert_eq!(out_buffer.len(), 2048);
    assert_eq!(out_tags.len(), 3);

    for (i, tag) in tags.iter().enumerate() {
        assert_eq!(out_tags[i].index, tag.index + 1024);
        let Tag::Id(tag_id) = tag.tag else {
            unreachable!()
        };
        assert!(matches!(out_tags[i].tag, Tag::Id(t) if t == tag_id));
    }

    let (out_buffer, out_tags) = mock.take_output::<f32>(0);
    assert_eq!(out_buffer.len(), 2048);
    assert_eq!(out_tags.len(), 3);

    let (out_buffer, out_tags) = mock.output::<f32>(0);
    assert_eq!(out_buffer.len(), 0);
    assert_eq!(out_tags.len(), 0);

    Ok(())
}

#[test]
fn mock_pmts() -> Result<()> {
    let copy = MessageCopy::new();

    let mut mock = Mocker::new(copy);
    mock.init();

    let ret = mock.post(PortId::Index(0), Pmt::Usize(123));
    assert_eq!(ret, Ok(Pmt::Ok));
    mock.run();

    let pmts = mock.take_messages();
    assert_eq!(pmts, vec![vec![Pmt::Usize(123)]]);

    Ok(())
}
