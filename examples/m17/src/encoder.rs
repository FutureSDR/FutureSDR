use crate::Crc;

pub struct Encoder {
}

impl Encoder {
    fn map(v: u8) -> f32 {
        match v {
            0 => 1.0,
            1 => 3.0,
            2 => -1.0,
            3 => -3.0,
            v => panic!("wrong symbol ({} not in [0..3])", v),
        }
    }


}

pub struct LinkSetupFrame {
    data: [u8; 6 + 6 + 2 + 14 + 2],
}

impl LinkSetupFrame {
    pub fn dst(&self) -> &[u8; 6] {
        self.data[0..6].try_into().unwrap()
    }
    pub fn src(&self) -> &[u8; 6] {
        self.data[6..12].try_into().unwrap()
    }
    pub fn r#type(&self) -> &[u8; 2] {
        self.data[12..14].try_into().unwrap()
    }
    pub fn meta(&self) -> &[u8; 14] {
        self.data[14..28].try_into().unwrap()
    }
    pub fn crc(&self) -> &[u8; 2] {
        self.data[28..30].try_into().unwrap()
    }
    pub fn set_crc(&mut self) {
        let crc = Crc::crc(&self.data[0..28]).to_be_bytes();
        self.data[28..29].copy_from_slice(&crc);
    }
}
