use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessageCopy;
use futuresdr::prelude::*;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;
use rand::Rng;
use rand::distr::Uniform;

#[test]
fn multi_input_mock() {
    let input: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(128)
        .collect();

    let block: Apply<_, _, _, Reader<_>, Writer<_>> = Apply::new(|x: &u32| x + 1);

    let mut mocker = Mocker::new(block);
    mocker.input().set(input[..64].to_vec());
    mocker.output().reserve(128);
    mocker.run();
    mocker.input().set(input[64..].to_vec());
    mocker.run();
    let (output, _) = mocker.output().get();

    assert_eq!(input.len(), output.len());
    for (a, b) in input.iter().zip(output.iter()) {
        assert_eq!(a + 1, *b);
    }
}

#[test]
fn tags_through_mock() -> Result<()> {
    let noop: Apply<_, _, _, Reader<_>, Writer<_>> = Apply::new(|x: &f32| *x);

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
    mock.output().reserve(input.len() * 2);
    mock.input().set(input.clone());
    mock.run();

    let (out_buffer, out_tags) = mock.output().get();
    assert_eq!(out_buffer.len(), 1024);
    assert_eq!(out_tags.len(), 0);

    mock.input().set_with_tags(input, tags.clone());
    mock.run();
    mock.deinit();

    let (out_buffer, out_tags) = mock.output().get();
    assert_eq!(out_buffer.len(), 2048);
    assert_eq!(out_tags.len(), 3);

    for (i, tag) in tags.iter().enumerate() {
        assert_eq!(out_tags[i].index, tag.index + 1024);
        let Tag::Id(tag_id) = tag.tag else {
            unreachable!()
        };
        assert!(matches!(out_tags[i].tag, Tag::Id(t) if t == tag_id));
    }

    let (out_buffer, out_tags) = mock.output().take();
    assert_eq!(out_buffer.len(), 2048);
    assert_eq!(out_tags.len(), 3);

    let (out_buffer, out_tags) = mock.output().get();
    assert_eq!(out_buffer.len(), 0);
    assert_eq!(out_tags.len(), 0);

    Ok(())
}

#[test]
fn mock_pmts() -> Result<()> {
    let copy = MessageCopy;

    let mut mock = Mocker::new(copy);
    mock.init();

    let ret = mock.post("in", Pmt::Usize(123));
    assert_eq!(ret, Ok(Pmt::Ok));
    mock.run();

    let pmts = mock.take_messages();
    assert_eq!(pmts, vec![vec![Pmt::Usize(123)]]);

    Ok(())
}
