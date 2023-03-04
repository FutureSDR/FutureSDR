use rand::Rng;

use futuresdr::blocks::Apply;
use futuresdr::runtime::Mocker;

fn main() {
    let input: Vec<u32> = rand::thread_rng()
        .sample_iter(rand::distributions::Uniform::<u32>::new(0, 1024))
        .take(64)
        .collect();

    let block = Apply::new_typed(|x: &u32| x + 1);

    let mut mocker = Mocker::new(block);
    mocker.input(0, input.clone());
    mocker.init_output::<u32>(0, 64);
    mocker.run();
    let output = mocker.output::<u32>(0);

    assert_eq!(input.len(), output.len());
    for (a, b) in input.iter().zip(output.iter()) {
        assert_eq!(a + 1, *b);
    }
}
