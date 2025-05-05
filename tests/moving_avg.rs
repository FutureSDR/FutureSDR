use futuresdr::blocks::MovingAvg;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

#[test]
fn moving_avg_correct_output() {
    let block = MovingAvg::<3, Reader<f32>, Writer<f32>>::new(0.1, 3);
    let mut mocker = Mocker::new(block);

    mocker
        .input()
        .set(vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
    mocker.output().reserve(3);
    mocker.run();

    assert_eq!(mocker.output().get().0, vec![0.271, 0.542, 0.813]);
}

#[test]
fn moving_avg_handles_non_finite_values() {
    let block = MovingAvg::<3, Reader<f32>, Writer<f32>>::new(0.1, 3);
    let mut mocker = Mocker::new(block);
    mocker.input().set(vec![
        1.0,
        f32::NAN,
        3.0,
        1.0,
        f32::INFINITY,
        3.0,
        1.0,
        2.0,
        3.0,
    ]);
    mocker.output().reserve(3);
    mocker.run();

    assert_eq!(mocker.output().get().0, vec![0.271, 0.2, 0.813]);
}
