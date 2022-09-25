use futures::channel::mpsc::Sender;
use std::collections::HashMap;

use crate::anyhow::{bail, Context, Result};
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::Block;
use crate::runtime::BlockMessage;
use slab::Slab;
use std::any::{Any, TypeId};
use std::cmp::{Eq, PartialEq};
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

    pub fn block_id(&self, name: &str) -> Option<usize> {
        for (i, b) in self.blocks.iter() {
            if b.as_ref()?.instance_name()? == name {
                return Some(i);
            }
        }

        None
    }

    pub fn block_name(&self, id: usize) -> Option<&str> {
        if let Some(Some(b)) = &self.blocks.get(id) {
            b.instance_name()
        } else {
            None
        }
    }

    /// Adds a [Block] to the [Topology] returning the `id` of the [Block] in the [Topology].
    pub fn add_block(&mut self, mut block: Block) -> usize {
        let (mut i, base_name, mut block_name) = if let Some(name) = block.instance_name() {
            (-1, name.to_string(), name.to_string())
        } else {
            (
                0,
                block.type_name().to_string(),
                format!("{}_{}", block.type_name(), 0),
            )
        };

        // find a unique name
        loop {
            if self.block_id(&block_name).is_none() {
                break;
            }
            i += 1;
            block_name = format!("{}_{}", base_name, i);
        }

        block.set_instance_name(block_name);
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

    pub fn connect_stream<B: BufferBuilder + Debug + Eq + Hash>(
        &mut self,
        src_block: usize,
        src_port: &str,
        dst_block: usize,
        dst_port: &str,
        buffer_builder: B,
    ) -> Result<()> {
        let src = self
            .blocks
            .get(src_block)
            .context("src block invalid")?
            .as_ref()
            .context("src block not present")?;
        let dst = self
            .blocks
            .get(dst_block)
            .context("dst block invalid")?
            .as_ref()
            .context("dst block not present")?;

        let sp = src
            .stream_output_name_to_id(src_port)
            .context("invalid src port name")?;
        let sp = src.stream_output(sp);

        let dp = dst
            .stream_input_name_to_id(dst_port)
            .context("invalid dst port name")?;
        let dp = dst.stream_input(dp);

        let src_port_id = src
            .stream_output_name_to_id(src_port)
            .context("invalid src port name")?;
        let dst_port_id = dst
            .stream_input_name_to_id(dst_port)
            .context("invalid dst port name")?;

        if sp.item_size() != dp.item_size() {
            bail!("item sizes do not match");
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

    pub fn connect_message(
        &mut self,
        src_block: usize,
        src_port: &str,
        dst_block: usize,
        dst_port: &str,
    ) -> Result<()> {
        let src = self
            .blocks
            .get(src_block)
            .context("invalid src block")?
            .as_ref()
            .context("src block not present")?;
        let dst = self
            .blocks
            .get(dst_block)
            .context("invalid dst block")?
            .as_ref()
            .context("dst block not present")?;

        let src_port_id = src
            .message_output_name_to_id(src_port)
            .context("invalid src port name")?;
        let dst_port_id = dst
            .message_input_name_to_id(dst_port)
            .context("invalid dst port name")?;

        self.message_edges
            .push((src_block, src_port_id, dst_block, dst_port_id));

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // check if all stream ports are connected (neither message inputs nor outputs have to be connected)
        for (block_id, e) in self.blocks.iter() {
            if let Some(block) = e {
                for (out_id, _) in block.stream_outputs().iter().enumerate() {
                    if self
                        .stream_edges
                        .iter()
                        .filter(|(k, v)| k.0 == block_id && k.1 == out_id && !v.is_empty())
                        .count()
                        == 0
                    {
                        bail!("unconnected stream output port");
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
                        bail!("stream input port does not have exactly one input");
                    }
                }
            } else {
                bail!("block not owned by topology");
            }
        }

        // check if all stream edges are valid
        for ((src, src_port, _), v) in self.stream_edges.iter() {
            let src_block = self.block_ref(*src).expect("src block not found");
            let output = src_block.stream_output(*src_port);

            for (dst, dst_port) in v.iter() {
                let dst_block = self.block_ref(*dst).expect("dst block not found");
                let input = dst_block.stream_input(*dst_port);
                if output.item_size() != input.item_size() {
                    bail!("item size of stream connection does not match");
                }
            }
        }

        // all blocks are Some
        // all instance names are Some
        // all instance names are unique
        let mut v = Vec::new();
        for (_, b) in self.blocks.iter() {
            let c = b.as_ref().expect("block is not set");
            let name = c.instance_name().expect("block instance name not set");
            v.push(name.to_string());
        }
        v.sort();
        let len = v.len();
        v.dedup();
        if len != v.len() {
            bail!("duplicate block instance names");
        }

        Ok(())
    }

    pub fn block_ref(&self, id: usize) -> Option<&Block> {
        self.blocks.get(id).and_then(|v| v.as_ref())
    }

    pub fn block_mut(&mut self, id: usize) -> Option<&mut Block> {
        self.blocks.get_mut(id).and_then(|v| v.as_mut())
    }
}

impl Default for Topology {
    fn default() -> Self {
        Topology::new()
    }
}
