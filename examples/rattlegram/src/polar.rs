use crate::{get_le_bit, set_le_bit};

struct Assert<const V: bool>;
trait True {}
impl True for Assert<true> {}

struct Crc32 {
    poly: u32,
    crc: u32,
}

impl Crc32 {
    fn new(poly: u32) -> Self {
        Self { crc: 0, poly}
    }

    fn reset(&mut self) {
        self.crc = 0;
    }

    fn update(prev: u32, data: bool, poly: u32) -> u32 {
        let tmp = prev ^ data as u32;
        (prev >> 1) ^ ((tmp & 1) * poly)
    }

    fn put(&mut self, data:bool) -> u32 {
        self.crc = Self::update(self.crc, data, self.poly);
        self.crc
    }

    fn get(&self) -> u32 {
        self.crc
    }
}

pub struct PolarEncoder;

impl PolarEncoder {
    const CODE_ORDER: usize = 11;
    const MAX_BITS: usize = 1360 + 32;
    const CRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::Algorithm {
        width: 32,
        poly: 0x05EC76F1,
        init: 0x0,
        refin: true,
        refout: true,
        xorout: 0x000000,
        check: 0x0000,
        residue: 0x0000,
    });

    pub fn encode(code: &mut [i8], message: &[u8], frozen_bits: &[u32], data_bits: usize) {
        fn nrz(bit: bool) -> i8 {
            if bit {
                -1
            } else {
                1
            }
        }

        let mut mesg = [0; Self::MAX_BITS];

        for (i, m) in mesg.iter_mut().enumerate().take(data_bits) {
            *m = nrz(get_le_bit(message, i));
        }

        let crc = Self::CRC.checksum(&message[0..data_bits / 8]);

        for i in 0..32 {
            mesg[i + data_bits] = nrz(((crc >> i) & 1) == 1);
        }

        PolarSysEnc::encode(code, mesg.as_slice(), frozen_bits, Self::CODE_ORDER);
    }
}

struct PolarSysEnc;

impl PolarSysEnc {
    fn get(bits: &[u32], idx: usize) -> bool {
        ((bits[idx / 32] >> (idx % 32)) & 1) == 1
    }

    fn encode(codeword: &mut [i8], message: &[i8], frozen: &[u32], level: usize) {
        let length = 1 << level;
        let mut mi = 0;
        for i in (0..length as usize).step_by(2) {
            let msg0 = if Self::get(frozen, i) {
                1
            } else {
                let v = message[mi];
                mi += 1;
                v
            };
            let msg1 = if Self::get(frozen, i + 1) {
                1
            } else {
                let v = message[mi];
                mi += 1;
                v
            };
            codeword[i] = msg0 * msg1;
            codeword[i + 1] = msg1;
        }

        let mut h = 2usize;
        while h < length as usize {
            let mut i = 0usize;
            while i < length as usize {
                for j in i..(i + h) {
                    codeword[j] *= codeword[j + h];
                }
                i += 2 * h;
            }
            h *= 2;
        }

        for i in (0..length as usize).step_by(2) {
            let msg0 = if Self::get(frozen, i) { 1 } else { codeword[i] };
            let msg1 = if Self::get(frozen, i + 1) {
                1
            } else {
                codeword[i + 1]
            };
            codeword[i] = msg0 * msg1;
            codeword[i + 1] = msg1;
        }

        let mut h = 2usize;
        while h < length as usize {
            let mut i = 0usize;
            while i < length as usize {
                for j in i..(i + h) {
                    codeword[j] *= codeword[j + h];
                }
                i += 2 * h;
            }
            h *= 2;
        }
    }
}

type Type = [i8; 16];
type MesgType = [i8; 16];
type Path = i64;
type Map = [u8; 16];

pub struct PolarDecoder {
    mesg: [MesgType; Self::MAX_BITS],
    mess: [MesgType; Self::CODE_LEN],
    decode: PolarListDecoder,
    crc: Crc32,
}

impl PolarDecoder {
    const CODE_ORDER: usize = 11;
    const CODE_LEN: usize = 1 << Self::CODE_ORDER;
    const MAX_BITS: usize = 1360 + 32;

    pub fn new() -> Self {
        Self {
            mesg: [[1; 16]; Self::MAX_BITS],
            mess: [[1; 16]; Self::CODE_LEN],
            decode: PolarListDecoder::new(),
            crc: Crc32::new(0x8F6E37A0),
        }
    }

    fn systematic(&mut self, frozen_bits: &[u32], crc_bits: usize) {
        PolarEnc::encode(&mut self.mess, &self.mesg, frozen_bits, Self::CODE_ORDER);
        let mut i = 0;
        let mut j = 0;
        while i < Self::CODE_LEN && j < crc_bits {
			if ((frozen_bits[i / 32] >> (i % 32)) & 1) == 0 {
				self.mesg[j] = self.mess[i];
                j += 1;
            }
            i += 1;
        }
    }

    pub fn decode(&mut self, message: &mut [u8], code: &[i8], frozen_bits: &[u32], data_bits: usize) -> i32 {
        let crc_bits = data_bits + 32;
        let mut metric = [0i64; 16];
        self.decode.decode(&mut metric, &mut self.mesg, code, frozen_bits, Self::CODE_ORDER);
        self.systematic(frozen_bits, crc_bits);
        let mut order = [0; 16];
        for k in 0..16 {
            order[k] = k;
        }
        order.sort_by(|a, b| metric[*a].cmp(&metric[*b]));
        
        let mut best = -1isize;
        for k in 0..16 {
            self.crc.reset();
            for i in 0..crc_bits {
                self.crc.put(self.mesg[i][order[k]] < 0);
            }
            if self.crc.get() == 0 {
                best = order[k] as isize;
                break;
            }
        }

        if best < 0 {
            return -1;
        }

        let mut flips = 0;
        let mut i = 0;
        let mut j = 0;
        while i < data_bits {
            while ((frozen_bits[j / 32] >> (j % 32)) & 1) == 1 {
                j += 1;
            }
            let received = code[j] < 0;
            let decoded = self.mesg[i][best as usize] < 0;
            if received != decoded {
                flips += 1;
            }
            set_le_bit(message, i, decoded);

            i += 1;
            j += 1;
        }

        flips
    }
}


struct PolarEnc;

impl PolarEnc {
    fn get(bits: &[u32], idx: usize) -> bool {
		((bits[idx/32] >> (idx%32)) & 1) != 0
    }
    fn encode(codeword: &mut [MesgType], message: &[MesgType], frozen: &[u32], level: usize) {
        let length = 1 << level;
        let mut mi = 0;
        for i in (0..length).step_by(2) {
            let msg0 = if Self::get(frozen, i) { [1; 16] } else { let v = message[mi]; mi += 1; v };
            let msg1 = if Self::get(frozen, i+1) { [1; 16] } else { let v = message[mi]; mi += 1; v };
            let mut tmp = [0; 16];
            for k in 0..16 {
                tmp[k] = msg0[k] * msg1[k];
            }
            codeword[i] = tmp;
            codeword[i+1] = msg1;
        }

        let mut h = 2;
        while h < length {
            let mut i = 0;
            while i < length {
                for j in i..(i+h) {
                    let mut tmp = [0; 16];
                    for k in 0..16 {
                        tmp[k] = codeword[j][k] * codeword[j+h][k];
                    }
                    codeword[j] = tmp;                    
                }
                i+= 2* h;
            }
            h *= 2;
        }
    }
}

struct PolarListDecoder {
    soft: [MesgType; 2 * Self::MAX_N],
    hard: [MesgType; Self::MAX_N],
    maps: [[u8; 16]; Self::MAX_N],
}

impl PolarListDecoder {
    const MAX_M: usize = 11;
    const MAX_N: usize = 1 << Self::MAX_M;

    fn new() -> Self {
        Self {
            soft: [[0; 16]; 2 * Self::MAX_N],
            hard: [[0; 16]; Self::MAX_N],
            maps: [[0; 16]; Self::MAX_N],
        }
    }

    fn decode(&mut self, metric: &mut [i64], message: &mut [MesgType], codeword: &[i8], frozen: &[u32], level: usize) {
        assert!(level <= Self::MAX_M);
        let mut count = 0;
        metric[0] = 0;
        for k in 1..16 {
            metric[k] = 1000;
        }
        let length = 1 << level;
        for i in 0..length {
            self.soft[length + i] = [codeword[i]; 16];
        }

        assert_eq!(level, 11);
		    PolarListTree::<11>::decode(metric, message, &self.maps, &mut count, &mut self.hard, &mut self.soft, frozen);

        let mut acc = self.maps[count-1];
        let mut i = count as isize -2;
        while i >= 0 {
            for k in 0..16 {
                message[i as usize][k] = acc[message[i as usize][k] as usize] as i8;
            }
            for k in 0..16 {
                acc[k] = acc[self.maps[i as usize][k] as usize];
            }
            i -= 1;
        }
    }
}

struct PolarListNode<const M: usize>;

impl<const M: usize> PolarListNode<M> 
    where Assert<{M > 0}>: True
{
    const N: usize = 1 << M;

    fn rate0(metric: &mut[Path], hard: &[Type], soft: &[Type]) -> Map {
        hard.fill([1; 16]);
        for i in 0..Self::N {
            for k in 0..16 {
                if soft[i+Self::N][k] < 0 {
                    metric[k] -= soft[i+Self::N][k] as i64;
                }
            }
        }
        let mut map = [0u8; 16];
        for k in 0..16u8 {
            map[k as usize] = k; 
        }
        map
    }
}

impl PolarListNode<0> {
    fn rate0(metric: &mut [Path], hard: &mut [Type], soft: &[Type]) -> Map {
        hard[0] = [1i8; 16];
        for k in 0..16 {
            if soft[1][k] < 0 {
                metric[k] -= soft[1][k] as i64;
            }
        }
        let mut map = [0u8; 16];
        for k in 0..16u8 {
            map[k as usize] = k; 
        }
        map
    }

    fn rate1(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut Type, soft: &[Type]) -> Map {
        let sft = soft[1];
        let mut fork = [0i64; 2 * 16];
        for k in 0..16 {
            fork[k + 16] = metric[k];
            fork[k] = metric[k];
        }
        for k in 0..16 {
            if sft[k] < 0 {
                fork[k] -= sft[k] as i64;
            } else {
                fork[k+16] += sft[k] as i64;
            }
        }
        let mut perm = [0usize; 2*16];
        for k in 0..2*16 {
            perm[k] = k;
        }
        perm.sort_by(|a, b| fork[*a].cmp(&fork[*b]));
        for k in 0..16 {
            metric[k] = fork[perm[k]];
        }
        let mut map = [0u8; 16];
        for k in 0..16usize {
            map[k] = (perm[k] % 16) as u8; 
        }
        let mut hrd = [0i8; 16];
        for k in 0..16 {
            hrd[k] = if perm[k] < 16 { 1 } else { -1 };
        }
        message[*count] = hrd;
        maps[*count] = map;
        *count += 1;
        *hard = hrd;
        map
    }
}

fn vmin(mut a: Type, b: Type) -> Type {
    for i in 0..16 {
        if b[i] < a[i] {
            a[i] = b[i];
        }
    }
    a
}

fn vmax(mut a: Type, b: Type) -> Type {
    for i in 0..16 {
        if b[i] > a[i] {
            a[i] = b[i];
        }
    }
    a
}

fn vqabs(mut a: Type) -> Type {
    for i in 0..16 {
        a[i] = a[i].saturating_abs();
    }
    a
}

fn vsignum(mut a: Type) -> Type {
    for i in 0..16 {
        a[i] = if a[i] > 0 { 1 } else if a[i] == 0 { 0 } else { -1 };
    }
    a
}

fn vsign(mut a: Type, b: Type) -> Type {
    for i in 0..16 {
        a[i] = if b[i] > 0 { a[i] } else if b[i] == 0 { 0 } else { -1 * std::cmp::max(a[i], - std::i8::MAX) };
    }
    a
}

fn prod(mut a: Type, b: Type) -> Type {
		vsign(vmin(vqabs(a), vqabs(b)), vsign(vsignum(a), b))
}

fn vshuf(mut a: Type, b: Map) -> Type {
    for i in 0..16 {
        a[i] = if b[i] < 16 { a[b[i] as usize] } else { 0 };
    }
    a
}

fn vqadd(mut a: Type, b: Type) -> Type {
    for i in 0..16 {
        a[i] = a[i].saturating_add(b[i]);
    }
    a
}
fn qmul(mut a: Type, b: Type) -> Type {
    for i in 0..16 {
        a[i] = a[i].saturating_mul(b[i]);
    }
    a
}

fn madd(a: Type, b: Type, c: Type) -> Type {
		vqadd(vsign(vmax(b, [-127; 16]), a), c)
}

fn to_map(mut a: Type) -> Map {
    let mut tmp = [0u8; 16];
    for i in 0..16 {
        assert!(a[i] > 0);
        tmp[i] = a[i] as u8;
    }
    tmp
}

fn to_type(mut a: Map) -> Type {
    let mut tmp = [0i8; 16];
    for i in 0..16 {
        assert!(a[i] <= std::i8::MAX as u8);
        tmp[i] = a[i] as i8;
    }
    tmp
}

struct PolarListTree<const M: usize>;

impl<const M: usize> PolarListTree<M> 
    where Assert<{M > 6}>: True,
        [(); M-1]:,
{
    const N: usize = 1 << M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: &[u32]) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let lmap = PolarListTree::<{M-1}>::decode(metric, message, maps, count, hard, soft, frozen);
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        let rmap = PolarListTree::<{M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, &frozen[Self::N/2/32..]);
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))
    }
}

impl PolarListTree<6> {
    const M: usize = 6;
    const N: usize = 1 << Self::M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & ((1<<(1<<(Self::M-1)))-1)) == ((1<<(1<<(Self::M-1)))-1) {
            lmap = PolarListNode::<{Self::M-1}>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, hard, soft, frozen & ((1<<(1<<(Self::M-1)))-1));
        }
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        if frozen >> (Self::N/2) == ((1<<(1<<(Self::M-1)))-1) {
            rmap = PolarListNode::<{Self::M-1}>::rate0(metric, &hard[Self::N/2..], soft);
        } else {
            rmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, frozen >> (Self::N/2));
        }
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))

    }
}

impl PolarListTree<5> {
    const M: usize = 5;
    const N: usize = 1 << Self::M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & ((1<<(1<<(Self::M-1)))-1)) == ((1<<(1<<(Self::M-1)))-1) {
            lmap = PolarListNode::<{Self::M-1}>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, hard, soft, frozen & ((1<<(1<<(Self::M-1)))-1));
        }
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        if frozen >> (Self::N/2) == ((1<<(1<<(Self::M-1)))-1) {
            rmap = PolarListNode::<{Self::M-1}>::rate0(metric, &hard[Self::N/2..], soft);
        } else {
            rmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, frozen >> (Self::N/2));
        }
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))
    }
}

impl PolarListTree<4> {
    const M: usize = 4;
    const N: usize = 1 << Self::M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & ((1<<(1<<(Self::M-1)))-1)) == ((1<<(1<<(Self::M-1)))-1) {
            lmap = PolarListNode::<{Self::M-1}>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, hard, soft, frozen & ((1<<(1<<(Self::M-1)))-1));
        }
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        if frozen >> (Self::N/2) == ((1<<(1<<(Self::M-1)))-1) {
            rmap = PolarListNode::<{Self::M-1}>::rate0(metric, &hard[Self::N/2..], soft);
        } else {
            rmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, frozen >> (Self::N/2));
        }
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))
    }
}

impl PolarListTree<3> {
    const M: usize = 3;
    const N: usize = 1 << Self::M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & ((1<<(1<<(Self::M-1)))-1)) == ((1<<(1<<(Self::M-1)))-1) {
            lmap = PolarListNode::<{Self::M-1}>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, hard, soft, frozen & ((1<<(1<<(Self::M-1)))-1));
        }
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        if frozen >> (Self::N/2) == ((1<<(1<<(Self::M-1)))-1) {
            rmap = PolarListNode::<{Self::M-1}>::rate0(metric, &hard[Self::N/2..], soft);
        } else {
            rmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, frozen >> (Self::N/2));
        }
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))
    }
}

impl PolarListTree<2> {
    const M: usize = 2;
    const N: usize = 1 << Self::M;

    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = prod(soft[i+Self::N], soft[i+Self::N/2+Self::N]);
        }
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & ((1<<(1<<(Self::M-1)))-1)) == ((1<<(1<<(Self::M-1)))-1) {
            lmap = PolarListNode::<{Self::M-1}>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, hard, soft, frozen & ((1<<(1<<(Self::M-1)))-1));
        }
        for i in 0..Self::N/2 {
            soft[i+Self::N/2] = madd(hard[i], vshuf(soft[i+Self::N], lmap), vshuf(soft[i+Self::N/2+Self::N], lmap));
        }
        if frozen >> (Self::N/2) == ((1<<(1<<(Self::M-1)))-1) {
            rmap = PolarListNode::<{Self::M-1}>::rate0(metric, &hard[Self::N/2..], soft);
        } else {
            rmap = PolarListTree::<{Self::M-1}>::decode(metric, message, maps, count, &mut hard[Self::N/2..], soft, frozen >> (Self::N/2));
        }
        for i in 0..Self::N/2 {
            hard[i] = qmul(vshuf(hard[i], rmap), hard[i+Self::N/2]);
        }
        to_map(vshuf(to_type(lmap), rmap))
    }
}

impl PolarListTree<1> {
    fn decode(metric: &mut [Path], message: &mut [Type], maps: &[Map], count: &mut usize, hard: &mut [Type], soft: &mut [Type], frozen: u32) -> Map {
        soft[1] = prod(soft[2], soft[3]);
        let mut lmap = [0u8; 16];
        let mut rmap = [0u8; 16];
        if (frozen & 1) == 1 {
            lmap = PolarListNode::<0>::rate0(metric, hard, soft);
        } else {
            lmap = PolarListNode::<0>::rate1(metric, message, maps, count, &mut hard[0], soft);
        }
        soft[1] = madd(hard[0], vshuf(soft[2], lmap), vshuf(soft[3], lmap));
        if (frozen >> 1) != 0 {
            rmap = PolarListNode::<0>::rate0(metric, &mut hard[1..], soft);
        } else {
            rmap = PolarListNode::<0>::rate1(metric, message, maps, count, &mut hard[1], soft);
        }
        hard[0] = qmul(vshuf(hard[0], rmap), hard[1]);
        to_map(vshuf(to_type(lmap), rmap))
    }
}
