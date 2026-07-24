use futuresdr::runtime::buffer::CpuSample;

const SIZE: usize = <() as CpuSample>::SIZE.get();

fn main() {
    let _ = SIZE;
}
