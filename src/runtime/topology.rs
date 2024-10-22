use futures::channel::mpsc::Sender;
use std::collections::HashMap;

use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::{Block, ConnectCtx};
use slab::Slab;
use std::any::{Any, TypeId};
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

pub trait BufferBuilderKey: Debug + Send + Sync {
    fn eq(&self, other: &dyn BufferBuilderKey) -> bool;
    fn hash(&self) -> u64;
    fn as_any(&self) -> &dyn Any;
    fn builder(&self) -> &dyn BufferBuilder;
}

impl<T: BufferBuilder + Debug + Eq + Hash + 'static> BufferBuilderKey for T {
    fn eq(&self, other: &dyn BufferBuilderKey) -> bool {
        if let Some(other) = other.as_any().downcast_ref::<T>() {
            return self == other;
        }
        false
    }

    fn hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        Hash::hash(&(TypeId::of::<T>(), self), &mut h);
        h.finish()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn builder(&self) -> &dyn BufferBuilder {
        self
    }
}

#[derive(Debug)]
pub struct BufferBuilderEntry {
    item_size: usize,
    builder: Box<dyn BufferBuilderKey>,
}

impl BufferBuilderEntry {
    pub(crate) fn build(
        &self,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        self.builder
            .builder()
            .build(self.item_size, writer_inbox, writer_output_id)
    }
}

impl PartialEq for BufferBuilderEntry {
    fn eq(&self, other: &Self) -> bool {
        BufferBuilderKey::eq(self.builder.as_ref(), other.builder.as_ref())
    }
}

impl Eq for BufferBuilderEntry {}

impl Hash for BufferBuilderEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let key_hash = BufferBuilderKey::hash(self.builder.as_ref());
        state.write_u64(key_hash);
    }
}

/// The actual graph that backs a [Flowgraph](crate::runtime::Flowgraph).
#[derive(Debug)]
pub struct Topology {
    pub(crate) blocks: Slab<Option<Block>>,
    pub(crate) stream_edges: HashMap<(usize, usize, BufferBuilderEntry), Vec<(usize, usize)>>,
    // src blk, src port, dst blk, dst port
    pub(crate) message_edges: Vec<(usize, usize, usize, usize)>,
}

impl Topology {
    /// Constructs a blank [Topology]
    pub fn new() -> Self {
        Topology {
            blocks: Slab::new(),
            stream_edges: HashMap::new(),
            message_edges: Vec::new(),
        }
    }

    /// Get Id of a block, given its name
    pub fn block_id(&self, name: &str) -> Option<usize> {
        for (i, b) in self.blocks.iter() {
            if b.as_ref()?.instance_name()? == name {
                return Some(i);
            }
        }

        None
    }

    /// Get name of a block, given its Id
    pub fn block_name(&self, id: usize) -> Option<&str> {
        if let Some(Some(b)) = &self.blocks.get(id) {
            b.instance_name()
        } else {
            None
        }
    }

    /// Adds a [Block] to the [Topology] returning the `id` of the [Block] in the [Topology].
    pub fn add_block(&mut self, mut block: Block) -> usize {
        let block_name = block.type_name();
        let block_id = self.blocks.vacant_key();
        block.set_instance_name(format!("{}-{}", block_name, block_id));
        self.blocks.insert(Some(block))
    }

    /// Removes a [Block] and all edges connected to the [Block] from the [Topology].
    pub fn delete_block(&mut self, id: usize) {
        // remove from registry
        self.blocks.remove(id);

        // delete associated stream edges
        self.stream_edges.retain(|k, _| k.0 != id);
        for (_, vec) in self.stream_edges.iter_mut() {
            *vec = vec.iter().filter(|x| x.0 != id).copied().collect();
        }
        self.stream_edges = self
            .stream_edges
            .drain()
            .filter(|(_, v)| !v.is_empty())
            .collect();

        // delete associated message edges
        self.message_edges.retain(|x| x.0 != id && x.2 != id);
    }

    /// Connect stream ports
    pub fn connect_stream<B: BufferBuilder + Debug + Eq + Hash>(
        &mut self,
        src_block: usize,
        src_port: PortId,
        dst_block: usize,
        dst_port: PortId,
        buffer_builder: B,
    ) -> Result<(), Error> {
        let src = self
            .blocks
            .get(src_block)
            .ok_or(Error::InvalidBlock(src_block))?
            .as_ref()
            .ok_or(Error::InvalidBlock(src_block))?;
        let dst = self
            .blocks
            .get(dst_block)
            .ok_or(Error::InvalidBlock(dst_block))?
            .as_ref()
            .ok_or(Error::InvalidBlock(dst_block))?;

        let src_port_id = match src_port {
            PortId::Name(ref s) => src
                .stream_output_name_to_id(s)
                .ok_or(Error::InvalidStreamPort(src_block, src_port.clone()))?,
            PortId::Index(i) => {
                if i < src.stream_outputs().len() {
                    i
                } else {
                    return Err(Error::InvalidStreamPort(src_block, src_port));
                }
            }
        };
        let sp = src.stream_output(src_port_id);

        let dst_port_id = match dst_port {
            PortId::Name(ref s) => dst
                .stream_input_name_to_id(s)
                .ok_or(Error::InvalidStreamPort(dst_block, dst_port.clone()))?,
            PortId::Index(i) => {
                if i < dst.stream_inputs().len() {
                    i
                } else {
                    return Err(Error::InvalidStreamPort(dst_block, dst_port));
                }
            }
        };
        let dp = dst.stream_input(dst_port_id);

        if sp.type_id() != dp.type_id() {
            return Err(Error::ConnectError(Box::new(ConnectCtx::new(
                src, &src_port, sp, dst, &dst_port, dp,
            ))));
        }

        let buffer_entry = BufferBuilderEntry {
            item_size: sp.item_size(),
            builder: Box::new(buffer_builder),
        };
        let id = (src_block, src_port_id, buffer_entry);
        if let Some(v) = self.stream_edges.get_mut(&id) {
            v.push((dst_block, dst_port_id));
        } else {
            self.stream_edges.insert(id, vec![(dst_block, dst_port_id)]);
        }
        Ok(())
    }

    /// Connect message ports
    pub fn connect_message(
        &mut self,
        src_block: usize,
        src_port: PortId,
        dst_block: usize,
        dst_port: PortId,
    ) -> Result<(), Error> {
        let src = self
            .blocks
            .get(src_block)
            .ok_or(Error::InvalidBlock(src_block))?
            .as_ref()
            .ok_or(Error::InvalidBlock(src_block))?;
        let dst = self
            .blocks
            .get(dst_block)
            .ok_or(Error::InvalidBlock(dst_block))?
            .as_ref()
            .ok_or(Error::InvalidBlock(dst_block))?;

        let src_port_id = match src_port {
            PortId::Name(ref s) => src
                .message_output_name_to_id(s)
                .ok_or(Error::InvalidMessagePort(Some(src_block), src_port.clone()))?,
            PortId::Index(i) => {
                if i < src.message_outputs().len() {
                    i
                } else {
                    return Err(Error::InvalidMessagePort(Some(src_block), src_port.clone()));
                }
            }
        };
        let dst_port_id = match dst_port {
            PortId::Name(ref s) => dst
                .message_input_name_to_id(s)
                .ok_or(Error::InvalidMessagePort(Some(dst_block), dst_port.clone()))?,
            PortId::Index(i) => {
                if i < dst.message_outputs().len() {
                    i
                } else {
                    return Err(Error::InvalidMessagePort(Some(dst_block), dst_port));
                }
            }
        };

        self.message_edges
            .push((src_block, src_port_id, dst_block, dst_port_id));

        Ok(())
    }

    /// Validate flowgraph topology
    ///
    /// Make sure that all stream ports are connected. Check if connections are valid, e.g., every
    /// stream input has exactly one connection.
    pub fn validate(&self) -> Result<(), Error> {
        // check if all stream ports are connected (neither message inputs nor outputs have to be connected)
        for (block_id, e) in self.blocks.iter() {
            if let Some(block) = e {
                for (out_id, out_port) in block.stream_outputs().iter().enumerate() {
                    if self
                        .stream_edges
                        .iter()
                        .filter(|(k, v)| k.0 == block_id && k.1 == out_id && !v.is_empty())
                        .count()
                        == 0
                    {
                        return Err(Error::ValidationError(format!(
                            "unconnected stream output port {:?} of block {:?}",
                            out_port,
                            block.instance_name()
                        )));
                    }
                }

                for (input_id, _) in block.stream_inputs().iter().enumerate() {
                    // there should be exactly one buffer, with exactly one connection to the input
                    if self
                        .stream_edges
                        .values()
                        .map(|v| v.iter().filter(|x| **x == (block_id, input_id)).count() == 1)
                        .filter(|b| *b)
                        .count()
                        != 1
                    {
                        return Err(Error::ValidationError(format!(
                            "Block {} stream input {} does not have exactly one input",
                            block_id, input_id
                        )));
                    }
                }
            } else {
                return Err(Error::ValidationError(format!(
                    "Block {} not owned by topology",
                    block_id
                )));
            }
        }

        // check if all stream edges are valid
        for ((src, src_port, _), v) in self.stream_edges.iter() {
            let src_block = self.block_ref(*src).ok_or(Error::ValidationError(format!(
                "Source block {} not found",
                src
            )))?;
            let output = src_block.stream_output(*src_port);

            for (dst, dst_port) in v.iter() {
                let dst_block = self.block_ref(*dst).ok_or(Error::ValidationError(format!(
                    "Destination block {} not found",
                    dst
                )))?;
                let input = dst_block.stream_input(*dst_port);
                if output.type_id() != input.type_id() {
                    return Err(Error::ValidationError(format!(
                        "Item size of stream connection does not match ({}, {:?} -> {}, {:?})",
                        src, src_port, dst, dst_port
                    )));
                }
            }
        }

        // all blocks are Some
        // all instance names are Some
        // all instance names are unique
        let mut v = Vec::new();
        for (i, b) in self.blocks.iter() {
            let c = b.as_ref().ok_or(Error::ValidationError(format!(
                "Block {} not present/not owned by topology",
                i
            )))?;
            let name = c.instance_name().ok_or(Error::ValidationError(format!(
                "Block {}, {:?} has no instance name",
                i, c
            )))?;
            v.push(name.to_string());
        }
        v.sort();
        let len = v.len();
        v.dedup();
        if len != v.len() {
            return Err(Error::ValidationError(
                "Duplicate block instance names".to_string(),
            ));
        }

        Ok(())
    }

    /// Get reference to a block
    pub fn block_ref(&self, id: usize) -> Option<&Block> {
        self.blocks.get(id).and_then(|v| v.as_ref())
    }

    /// Get mutable reference to a block
    pub fn block_mut(&mut self, id: usize) -> Option<&mut Block> {
        self.blocks.get_mut(id).and_then(|v| v.as_mut())
    }
}

impl Default for Topology {
    fn default() -> Self {
        Topology::new()
    }
}
