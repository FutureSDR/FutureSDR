use rand::Rng;
use rand::distr::Uniform;

use futuresdr::blocks::Apply;
use futuresdr::runtime::Mocker;

fn main() {
    let input: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(64)
        .collect();

    let block = Apply::new(|x: &u32| x + 1);

    let mut mocker = Mocker::new(block);
    mocker.input(0, input.clone());
    mocker.init_output::<u32>(0, 64);
    mocker.run();
    let (output, _) = mocker.output::<u32>(0);

    assert_eq!(input.len(), output.len());
    for (a, b) in input.iter().zip(output.iter()) {
        assert_eq!(a + 1, *b);
    }
}
