use futuresdr::macros::async_trait;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use futuresdr::tracing::debug;
use futuresdr::tracing::info;
use std::cmp::min;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Frame {
    pub nibbles: Vec<u8>,
    pub implicit_header: bool,
    pub has_crc: bool,
    pub code_rate: usize,
    pub annotations: HashMap<String, Pmt>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            nibbles: Vec::new(),
            implicit_header: false,
            has_crc: false,
            code_rate: 1,
            annotations: HashMap::new(),
        }
    }
}

pub enum HeaderMode {
    Explicit,
    Implicit {
        payload_len: usize,
        has_crc: bool,
        code_rate: usize,
    },
}

const HEADER_LEN: usize = 5; // size of the header in nibbles

pub struct HeaderDecoder {
    mode: HeaderMode,
    left: usize,
    frame: Frame,
    ldro_mode: bool,
}

impl HeaderDecoder {
    pub fn new(mode: HeaderMode, ldro_mode: bool) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("HeaderDecoder").build(),
            StreamIoBuilder::new().add_input::<u8>("in").build(),
            MessageIoBuilder::new()
                .add_output("out")
                .add_output("frame_info")
                .build(),
            HeaderDecoder {
                mode,
                left: 0,
                frame: Frame::default(),
                ldro_mode,
            },
        )
    }

    async fn publish_frame_info(
        mio: &mut MessageIo<Self>,
        cr: usize,
        pay_len: usize,
        crc: bool,
        ldro_mode: bool,
        err: bool,
    ) {
        let mut header_content: HashMap<String, Pmt> = HashMap::new();

        header_content.insert("cr".to_string(), Pmt::Usize(cr));
        header_content.insert("pay_len".to_string(), Pmt::Usize(pay_len));
        header_content.insert("crc".to_string(), Pmt::Bool(crc));
        header_content.insert("ldro_mode".to_string(), Pmt::Bool(ldro_mode));
        header_content.insert("err".to_string(), Pmt::Bool(err));
        mio.output_mut(1)
            .post(Pmt::MapStrPmt(header_content.clone()))
            .await;
    }
}

#[async_trait]
impl Kernel for HeaderDecoder {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<u8>();
        let mut nitem_to_consume = input.len();
        let n_input = input.len();
        let mut is_header = false;

        let tags: Vec<(usize, &HashMap<String, Pmt>)> = sio
            .input(0)
            .tags()
            .iter()
            .filter_map(|x| match x {
                ItemTag {
                    index,
                    tag: Tag::NamedAny(n, val),
                } => {
                    if n == "frame_info" {
                        match (**val).downcast_ref().unwrap() {
                            Pmt::MapStrPmt(map) => Some((*index, map)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        let mut annotations = if !tags.is_empty() {
            if tags[0].0 != 0 {
                nitem_to_consume = tags[0].0;
                if self.left == 0 {
                    sio.input(0).consume(nitem_to_consume);
                    io.call_again = true;
                    return Ok(());
                }
                None
            } else {
                if tags.len() >= 2 {
                    nitem_to_consume = tags[1].0 - tags[0].0;
                }
                is_header = if let Pmt::Bool(tmp) = tags[0].1.get("is_header").unwrap() {
                    *tmp
                } else {
                    panic!()
                };
                if is_header {
                    let mut tmp = tags[0].1.clone();
                    tmp.remove("is_header");
                    Some(tmp)
                } else {
                    Some(tags[0].1.clone())
                }
            }
        } else {
            None
        };

        if input.is_empty()
            || (is_header && input.len() < 5 && matches!(self.mode, HeaderMode::Explicit))
        {
            if sio.input(0).finished() {
                mio.output_mut(0).post(Pmt::Finished).await;
                io.finished = true;
            }
            return Ok(());
        }

        nitem_to_consume = min(nitem_to_consume, input.len());
        // nitem_to_consume = 0;
        if is_header {
            if let HeaderMode::Implicit {
                payload_len,
                has_crc,
                code_rate,
            } = self.mode
            {
                // implicit header
                self.frame = Frame {
                    nibbles: Vec::new(),
                    implicit_header: true,
                    has_crc,
                    code_rate,
                    annotations: annotations.unwrap_or(HashMap::<String, Pmt>::new()),
                };

                Self::publish_frame_info(
                    mio,
                    code_rate,
                    payload_len,
                    has_crc,
                    self.ldro_mode,
                    false,
                )
                .await;

                self.left = payload_len * 2 + if has_crc { 4 } else { 0 };
            } else {
                // explicit header to decode
                let payload_len = ((input[0] << 4) + input[1]) as usize;
                let has_crc = input[2] & 1 != 0;
                let code_rate = (input[2] >> 1) as usize;

                // check header Checksum
                let header_chk = ((input[3] & 1) << 4) + input[4];
                let c4: u8 = (input[0] & 0b1000) >> 3
                    ^ (input[0] & 0b0100) >> 2
                    ^ (input[0] & 0b0010) >> 1
                    ^ (input[0] & 0b0001);
                let c3: u8 = (input[0] & 0b1000) >> 3
                    ^ (input[1] & 0b1000) >> 3
                    ^ (input[1] & 0b0100) >> 2
                    ^ (input[1] & 0b0010) >> 1
                    ^ (input[2] & 0b0001);
                let c2: u8 = (input[0] & 0b0100) >> 2
                    ^ (input[1] & 0b1000) >> 3
                    ^ (input[1] & 0b0001)
                    ^ (input[2] & 0b1000) >> 3
                    ^ (input[2] & 0b0010) >> 1;
                let c1: u8 = (input[0] & 0b0010) >> 1
                    ^ (input[1] & 0b0100) >> 2
                    ^ (input[1] & 0b0001)
                    ^ (input[2] & 0b0100) >> 2
                    ^ (input[2] & 0b0010) >> 1
                    ^ (input[2] & 0b0001);
                let c0: u8 = (input[0] & 0b0001)
                    ^ (input[1] & 0b0010) >> 1
                    ^ (input[2] & 0b1000) >> 3
                    ^ (input[2] & 0b0100) >> 2
                    ^ (input[2] & 0b0010) >> 1
                    ^ (input[2] & 0b0001);

                info!("..:: Header");
                info!("Payload length: {}", payload_len);
                info!("CRC presence:   {}", has_crc);
                info!("Coding rate:    {}", code_rate);

                let mut head_err = header_chk as i16
                    - ((c4 << 4) + (c3 << 3) + (c2 << 2) + (c1 << 1) + c0) as i16
                    != 0;
                if head_err || payload_len == 0 {
                    debug!("Header checksum invalid!");
                    if head_err {
                        debug!("Header checksum invalid!");
                    }
                    if payload_len == 0 {
                        debug!("Frame can not be empty!");
                        debug!("item to process= {}", nitem_to_consume);
                    }
                    head_err = true;
                } else {
                    debug!("Header checksum valid!");
                }

                Self::publish_frame_info(
                    mio,
                    code_rate,
                    payload_len,
                    has_crc,
                    self.ldro_mode,
                    head_err,
                )
                .await;

                if !head_err {
                    self.frame = Frame {
                        nibbles: Vec::new(),
                        implicit_header: false,
                        has_crc,
                        code_rate,
                        annotations: annotations.unwrap_or(HashMap::<String, Pmt>::new()),
                    };

                    self.left = HEADER_LEN + payload_len * 2 + if has_crc { 4 } else { 0 };
                } else {
                    sio.input(0).consume(HEADER_LEN);
                    io.call_again = true;
                    return Ok(());
                }
            }
        } else if let Some(a) = annotations.take() {
            self.frame.annotations.extend(a);
        }

        if self.left > 0 {
            nitem_to_consume = nitem_to_consume.min(self.left);
            self.frame
                .nibbles
                .extend_from_slice(&input[0..nitem_to_consume]);
            self.left -= nitem_to_consume;

            if self.left == 0 {
                mio.output_mut(0)
                    .post(Pmt::Any(Box::new(std::mem::take(&mut self.frame))))
                    .await;
            }
            io.call_again = true;
            sio.input(0).consume(nitem_to_consume);
        }

        if sio.input(0).finished() && nitem_to_consume == n_input {
            mio.output_mut(0).post(Pmt::Finished).await;
            io.finished = true;
        }

        Ok(())
    }
}
