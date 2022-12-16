use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

use crate::BiMap;
use crate::CWAlphabet;
use crate::CWAlphabet::{LetterSpace, WordSpace};
use crate::get_alphabet;

pub struct CWToChar {
    symbol_vec: Vec<CWAlphabet>,
    // Required to keep the state of already received pulses
    alphabet: BiMap<char, Vec<CWAlphabet>>,
}

impl CWToChar {
    pub fn new(alphabet: BiMap<char, Vec<CWAlphabet>>) -> Block {
        Block::new(
            BlockMetaBuilder::new("CWToChar").build(),
            StreamIoBuilder::new()
                .add_input::<CWAlphabet>("in")
                .add_output::<char>("out")
                .build(),
            MessageIoBuilder::new().build(),
            CWToChar {
                symbol_vec: vec![],
                alphabet,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for CWToChar {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<CWAlphabet>();
        let o = sio.output(0).slice::<char>();

        let mut consumed = 0;
        let mut produced = 0;

        // Not doing any checks on the output buffer length here.
        // Assuming, that i and o are of the same length.
        // Assuming, that one input sample generates at max one output sample.
        for v in i.iter() {
            if (*v != LetterSpace) && (*v != WordSpace) {
                self.symbol_vec.push(*v);
            } else {
                if let Some(character) = self.alphabet.get_by_right(&self.symbol_vec) {
                    o[produced] = *character;
                    produced += 1;
                }
                self.symbol_vec.clear();

                if *v == WordSpace { // Special case if sequency of pulse codes is not followed by a LetterSpace but a WordSpace
                    self.symbol_vec.push(*v);
                    if let Some(character) = self.alphabet.get_by_right(&self.symbol_vec) {
                        o[produced] = *character;
                        produced += 1;
                    }
                    self.symbol_vec.clear();
                }
            }
            consumed += 1;
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);


        if sio.input(0).finished() && consumed == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}


pub struct CWToCharBuilder {
    alphabet: BiMap<char, Vec<CWAlphabet>>,
}

impl Default for CWToCharBuilder {
    fn default() -> Self {
        CWToCharBuilder {
            alphabet: get_alphabet(),
        }
    }
}

impl CWToCharBuilder {
    pub fn new() -> CWToCharBuilder {
        CWToCharBuilder::default()
    }

    pub fn alphabet(mut self, alphabet: BiMap<char, Vec<CWAlphabet>>) -> CWToCharBuilder {
        self.alphabet = alphabet;
        self
    }

    pub fn build(self) -> Block {
        CWToChar::new(
            self.alphabet,
        )
    }
}
