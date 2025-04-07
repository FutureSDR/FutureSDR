use futuresdr::blocks::Apply;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;
use rand::distr::Uniform;
use rand::Rng;

fn main() {
    let input: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(64)
        .collect();

    let mut block = Apply::<_, _, _, Reader<u32>, Writer<u32>>::new(|x: &u32| x + 1);
    block.input().set(input.clone());
    block.output().reserve(64);

    let mut mocker = Mocker::new(block);
    mocker.run();
    let (output, _) = mocker.output().get();

    assert_eq!(input.len(), output.len());
    for (a, b) in input.iter().zip(output.iter()) {
        assert_eq!(a + 1, *b);
    }
}
