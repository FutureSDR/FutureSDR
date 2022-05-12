
use futuresdr::num_complex::Complex32;
use futuresdr::blocks::ApplyIntoIter;

const dsss : [[Complex32; 16]; 16] = [
    [Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1),],
    [Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j),],
    [Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j),],
    [Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1),],
    [Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1),],
    [Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j),],
    [Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j),],
    [Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1),],
    [Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1),],
    [Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j),],
    [Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1),],
    [Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j),],
    [Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j),],
    [Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1),],
    [Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new(-1+1j),],
    [Complex32::new( 1+1j), Complex32::new(-1, -1), Complex32::new( 1, -1), Complex32::new(-1+1j), Complex32::new(-1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1), Complex32::new(-1, -1), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new(-1+1j), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new( 1+1j), Complex32::new( 1, -1), Complex32::new(-1, -1),]
];



fn make_iter(&i: u8) -> Box<dyn Iterator<Item = Complex32>> {

    


}


pub fn modulator() -> Block {


    ApplyIntoIter::new(make_iter)


}
