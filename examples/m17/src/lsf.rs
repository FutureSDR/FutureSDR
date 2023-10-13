use crate::CallSign;
use crate::Crc;

pub struct LinkSetupFrame {
    pub data: [u8; 6 + 6 + 2 + 14 + 2], // 30
}

impl LinkSetupFrame {
    pub fn new(src: CallSign, dst: CallSign) -> Self {
        assert!(matches!(src, CallSign::UnitId(_)));
        assert!(matches!(dst, CallSign::UnitId(_) | CallSign::Broadcast));

        let mut data = [0; 30];
        data[0..6].copy_from_slice(dst.encode());
        data[6..12].copy_from_slice(src.encode());
        data[13] = 0b101;

        let mut s = Self { data };
        s.set_crc();
        s
    }

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
    fn set_crc(&mut self) {
        let crc = Crc::crc(&self.data[0..28]).to_be_bytes();
        self.data[28..30].copy_from_slice(&crc);
    }
}

impl TryFrom<&[u8; 30]> for LinkSetupFrame {
    type Error = ();

    fn try_from(value: &[u8; 30]) -> Result<Self, Self::Error> {
        let mut lsf = LinkSetupFrame {
            data: value.clone(),
        };

        let mut tmp = [0u8; 2];
        tmp.copy_from_slice(&value[28..30]);
        lsf.set_crc();
        if lsf.data[28..30] == tmp {
            Ok(lsf)
        } else {
            Err(())
        }
    }
}
