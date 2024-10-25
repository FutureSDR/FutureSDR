use futuresdr::blocks::MovingAvg;
use futuresdr::runtime::Mocker;

#[test]
fn moving_avg_correct_output() {
    let block = MovingAvg::<3>::new_typed(0.1, 3);
    let mut mocker = Mocker::new(block);

    mocker.input::<f32>(0, vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
    mocker.init_output::<f32>(0, 3);
    mocker.run();

    assert_eq!(mocker.output::<f32>(0), vec![0.271, 0.542, 0.813]);
}

#[test]
fn moving_avg_handles_non_finite_values() {
    let block = MovingAvg::<3>::new_typed(0.1, 3);
    let mut mocker = Mocker::new(block);
    mocker.input::<f32>(
        0,
        vec![1.0, f32::NAN, 3.0, 1.0, f32::INFINITY, 3.0, 1.0, 2.0, 3.0],
    );
    mocker.init_output::<f32>(0, 3);
    mocker.run();

    assert_eq!(mocker.output::<f32>(0), vec![0.271, 0.2, 0.813]);
}
