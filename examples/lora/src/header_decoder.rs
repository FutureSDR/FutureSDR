use crate::utils::CodeRate;
use crate::utils::HeaderMode;
use futuresdr::dev_prelude::*;
use std::cmp::min;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Frame {
    pub nibbles: Vec<u8>,
    pub implicit_header: bool,
    pub has_crc: bool,
    pub code_rate: CodeRate,
    pub annotations: HashMap<String, Pmt>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            nibbles: Vec::new(),
            implicit_header: false,
            has_crc: false,
            code_rate: CodeRate::CR_4_5,
            annotations: HashMap::new(),
        }
    }
}

const HEADER_LEN: usize = 5; // size of the header in nibbles

#[derive(Block)]
#[message_outputs(out, frame_info)]
pub struct HeaderDecoder<I = DefaultCpuReader<u8>>
where
    I: CpuBufferReader<Item = u8>,
{
    #[input]
    input: I,
    mode: HeaderMode,
    left: usize,
    frame: Frame,
    ldro_enabled: bool,
}

impl<I> HeaderDecoder<I>
where
    I: CpuBufferReader<Item = u8>,
{
    pub fn new(mode: HeaderMode, ldro_enabled: bool) -> Self {
        Self {
            input: I::default(),
            mode,
            left: 0,
            frame: Frame::default(),
            ldro_enabled,
        }
    }

    async fn publish_frame_info(
        mo: &mut MessageOutputs,
        cr: usize,
        pay_len: usize,
        crc: bool,
        ldro_enabled: bool,
        err: bool,
    ) -> Result<()> {
        let mut header_content: HashMap<String, Pmt> = HashMap::new();

        header_content.insert("cr".to_string(), Pmt::Usize(cr));
        header_content.insert("pay_len".to_string(), Pmt::Usize(pay_len));
        header_content.insert("crc".to_string(), Pmt::Bool(crc));
        header_content.insert("ldro_enabled".to_string(), Pmt::Bool(ldro_enabled));
        header_content.insert("err".to_string(), Pmt::Bool(err));
        mo.post("frame_info", Pmt::MapStrPmt(header_content.clone()))
            .await?;
        Ok(())
    }
}

impl<I> Kernel for HeaderDecoder<I>
where
    I: CpuBufferReader<Item = u8>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (input, in_tags) = self.input.slice_with_tags();
        let mut nitem_to_consume = input.len();
        let n_input = input.len();
        let mut is_header = false;

        let tags: Vec<(usize, &HashMap<String, Pmt>)> = in_tags
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
                    self.input.consume(nitem_to_consume);
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
            if self.input.finished() {
                mo.post("out", Pmt::Finished).await?;
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
                    mo,
                    code_rate.into(),
                    payload_len,
                    has_crc,
                    self.ldro_enabled,
                    false,
                )
                .await?;

                self.left = payload_len * 2 + if has_crc { 4 } else { 0 };
            } else {
                // explicit header to decode
                let payload_len = ((input[0] << 4) + input[1]) as usize;
                let has_crc = input[2] & 1 != 0;
                let code_rate_raw = input[2] >> 1;
                let code_rate = CodeRate::try_from(code_rate_raw);

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
                info!("Coding rate:    {}", code_rate_raw);

                let mut head_err = header_chk as i16
                    - ((c4 << 4) + (c3 << 3) + (c2 << 2) + (c1 << 1) + c0) as i16
                    != 0;
                if head_err || payload_len == 0 {
                    info!("Header checksum invalid!");
                    if head_err {
                        debug!("Header checksum invalid!");
                    }
                    if payload_len == 0 {
                        debug!("Frame can not be empty!");
                        debug!("item to process= {}", nitem_to_consume);
                    }
                    head_err = true;
                } else if code_rate.is_err() {
                    info!("Header invalid!");
                    debug!("Code rate must be within [0, 3]!");
                    head_err = true;
                } else {
                    debug!("Header checksum valid!");
                }

                Self::publish_frame_info(
                    mo,
                    code_rate_raw as usize,
                    payload_len,
                    has_crc,
                    self.ldro_enabled,
                    head_err,
                )
                .await?;

                if !head_err {
                    self.frame = Frame {
                        nibbles: Vec::new(),
                        implicit_header: false,
                        has_crc,
                        code_rate: code_rate.unwrap(),
                        annotations: annotations.unwrap_or(HashMap::<String, Pmt>::new()),
                    };

                    self.left = HEADER_LEN + payload_len * 2 + if has_crc { 4 } else { 0 };
                } else {
                    self.input.consume(HEADER_LEN);
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
                mo.post("out", Pmt::Any(Box::new(std::mem::take(&mut self.frame))))
                    .await?;
            }
            io.call_again = true;
            self.input.consume(nitem_to_consume);
        }

        if self.input.finished() && nitem_to_consume == n_input {
            mo.post("out", Pmt::Finished).await?;
            io.finished = true;
        }

        Ok(())
    }
}
