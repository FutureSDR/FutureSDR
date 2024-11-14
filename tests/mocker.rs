use futuresdr::blocks::Apply;
use futuresdr::runtime::Mocker;
use rand::Rng;

#[test]
fn multi_input_mock() {
    let input: Vec<u32> = rand::thread_rng()
        .sample_iter(rand::distributions::Uniform::<u32>::new(0, 1024))
        .take(128)
        .collect();

    let block = Apply::new_typed(|x: &u32| x + 1);

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
