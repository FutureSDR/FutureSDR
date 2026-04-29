use std::collections::HashMap;

use crate::runtime::dev::prelude::*;

/// Add fixed fields to messages.
///
/// With a `payload_field_name`, any incoming message is wrapped in a
/// `Pmt::MapStrPmt` and stored under that field. Without one, incoming messages
/// must already be `Pmt::MapStrPmt`; the annotation fields are merged into the
/// map.
///
/// # Message Inputs
///
/// `in`: Message to annotate. `Pmt::Finished` terminates the block.
///
/// # Message Outputs
///
/// `out`: Annotated `Pmt::MapStrPmt` messages.
///
/// # Usage
/// ```
/// use std::collections::HashMap;
///
/// use futuresdr::blocks::MessageAnnotator;
/// use futuresdr::runtime::Pmt;
///
/// let mut fields = HashMap::new();
/// fields.insert("source".to_string(), Pmt::String("rx0".to_string()));
/// let annotator = MessageAnnotator::new(fields, Some("payload"));
/// ```
#[derive(Block)]
#[message_inputs(r#in)]
#[message_outputs(out)]
#[null_kernel]
pub struct MessageAnnotator {
    annotation_prototype: HashMap<String, Pmt>,
    payload_field_name: Option<String>,
}

impl MessageAnnotator {
    /// Create [`MessageAnnotator`] block.
    pub fn new(annotation: HashMap<String, Pmt>, payload_field_name: Option<&str>) -> Self {
        Self {
            annotation_prototype: annotation,
            payload_field_name: payload_field_name.map(String::from),
        }
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
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
                    mo.post("out", Pmt::MapStrPmt(annotated_message)).await?;
                }
            }
        } else {
            match p {
                Pmt::Finished => {
                    io.finished = true;
                }
                Pmt::MapStrPmt(mut annotated_message) => {
                    annotated_message.extend(self.annotation_prototype.clone());
                    mo.post("out", Pmt::MapStrPmt(annotated_message)).await?;
                }
                _ => return Ok(Pmt::InvalidValue),
            }
        }
        Ok(Pmt::Ok)
    }
}
