use bimap::{BiMap};
use crate::blocks::cw::{self, CWAlphabet};

use crate::anyhow::Result;
use crate::blocks::cw::CWAlphabet::LetterSpace;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;


pub struct CharToCW {
    alphabet: BiMap<char, Vec<CWAlphabet>>,
}

impl CharToCW {
    pub fn new(
        alphabet: BiMap<char, Vec<CWAlphabet>>,
    ) -> Block {
        Block::new(
            BlockMetaBuilder::new("CharToCW").build(),
            StreamIoBuilder::new()
                .add_input::<char>("in")
                .add_output::<CWAlphabet>("out")
                .build(),
            MessageIoBuilder::new().build(),
            CharToCW {
                alphabet: alphabet,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for CharToCW {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<char>();
        let o = sio.output(0).slice::<CWAlphabet>();

        let mut consumed = 0;
        let mut produced = 0;

        for v in i.iter() { // Iterate over all the chars in the input buffer
            if let Some(symbols) = self.alphabet.get_by_left(v) { // Check if the char can be converted to morse code pulses
                let symbols_len = symbols.len() + 1; // Length of the morse code pulse vector and +1 for final LetterSpace
                if (produced + symbols_len) < o.len() { // If there is still enough space left in the output buffer for the pulse representation of the char...
                    for (index, symbol) in symbols.iter().enumerate() { // ...get all pulses and...
                        o[produced+index] = *symbol; // ...add the pulse symbols to the output buffer
                    }
                    o[produced+symbols.len()] = LetterSpace; // Add a final LetterSpace after every character and...
                    produced += symbols_len; // Adjust the amount of consumed symbols for the runtime
                } else { // As the output buffer is already full...
                    break; // ...exit the loop over the chars to send the produced symbols to the runtime
                }
            }
            consumed += 1;
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && consumed == i.len(){
            io.finished = true;
        }

        Ok(())
    }
}


pub struct CharToCWBuilder {
    alphabet: BiMap<char, Vec<CWAlphabet>>,
}

impl Default for CharToCWBuilder {
    fn default() -> Self {
        CharToCWBuilder {
            alphabet: cw::get_alphabet(),
        }
    }
}

impl CharToCWBuilder {
    pub fn new() -> CharToCWBuilder {
        CharToCWBuilder::default()
    }

    pub fn alphabet(mut self, alphabet: BiMap<char, Vec<CWAlphabet>>) -> CharToCWBuilder {
        self.alphabet = alphabet;
        self
    }

    pub fn build(self) -> Block {
        CharToCW::new(
            self.alphabet,
        )
    }
}
