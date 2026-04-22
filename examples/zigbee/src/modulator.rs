use futuresdr::blocks::ApplyIntoIter;
use futuresdr::prelude::*;

const DSSS: [[Complex32; 16]; 16] = [
    //  0
    [
        // 0
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
    ],
    //  1
    [
        // 8
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
    ],
    //  2
    [
        // 4
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
    ],
    //  3
    [
        // 12
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
    ],
    //  4
    [
        // 2
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
    ],
    //  5
    [
        // 10
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
    ],
    //  6
    [
        // 6
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
    ],
    //  7
    [
        // 14
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
    ],
    //  8
    [
        // 1
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
    ],
    //  9
    [
        // 9
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
    ],
    // 10
    [
        // 5
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
    ],
    // 11
    [
        // 13
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
    ],
    // 12
    [
        // 3
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
    ],
    // 13
    [
        // 11
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
    ],
    // 14
    [
        // 7
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
    ],
    // 15
    [
        // 15
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, -1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(-1.0, 1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(1.0, 1.0),
        Complex32::new(1.0, -1.0),
        Complex32::new(-1.0, -1.0),
    ],
];

const SHAPE: [f32; 4] = [0.0, 0.707_106_77, 1.0, 0.707_106_77];

fn make_nibble(i: u8) -> impl Iterator<Item = Complex32> + Send {
    DSSS[i as usize]
        .iter()
        .flat_map(|x| [x; 4])
        .zip(SHAPE.iter().cycle())
        .map(|(x, y)| x * y)
}

pub fn modulator(fg: &mut Flowgraph) -> BlockId {
    fg.add_block(ApplyIntoIter::<_, _, _>::new(|i: &u8| {
        make_nibble(i & 0x0F).chain(make_nibble(i >> 4))
    }))
    .into()
}
