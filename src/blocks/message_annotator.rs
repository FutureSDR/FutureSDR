use std::collections::HashMap;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Forward messages.
pub struct MessageAnnotator {
    annotation_prototype: HashMap<String, Pmt>,
    payload_field_name: Option<String>,
}

impl MessageAnnotator {
    /// Create MessageCopy block
    pub fn new(
        annotation: HashMap<String, Pmt>,
        payload_field_name: Option<&str>,
    ) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessageCopy").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_output("out")
                .add_input("in", Self::handler)
                .build(),
            MessageAnnotator {
                annotation_prototype: annotation,
                payload_field_name: payload_field_name.map(String::from),
            },
        )
    }

    #[message_handler]
    async fn handler(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Some(payload_field_name) = self.payload_field_name.clone() {
            match p {
                Pmt::Finished => {
                    io.finished = true;
                }
                p => {
                    let mut annotated_message = self.annotation_prototype.clone();
                    annotated_message.insert(payload_field_name, p);
                    mio.post(0, Pmt::MapStrPmt(annotated_message)).await;
                }
            }
        } else {
            match p {
                Pmt::Finished => {
                    io.finished = true;
                }
                Pmt::MapStrPmt(mut annotated_message) => {
                    annotated_message.extend(self.annotation_prototype.clone());
                    mio.post(0, Pmt::MapStrPmt(annotated_message)).await;
                }
                _ => return Ok(Pmt::InvalidValue),
            }
        }
        Ok(Pmt::Ok)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageAnnotator {}
