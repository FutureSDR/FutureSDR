use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BlockInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferRequirements;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::dev::ItemTag;

struct CustomReader<T: CpuSample> {
    core: PortCore,
    data: Vec<T>,
    tags: Vec<ItemTag>,
    finished: bool,
}

impl<T: CpuSample> Default for CustomReader<T> {
    fn default() -> Self {
        Self {
            core: PortCore::new_unbound(),
            data: Vec::new(),
            tags: Vec::new(),
            finished: false,
        }
    }
}

impl<T: CpuSample> BufferReader for CustomReader<T> {
    type Inbox = BlockInbox;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn notify_finished(&mut self) {}

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<T: CpuSample> CpuBufferReader for CustomReader<T> {
    type Item = T;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        (self.data.as_slice(), &self.tags)
    }

    fn consume(&mut self, n: usize) {
        self.data.drain(..n);
    }

    fn set_min_items(&mut self, n: usize) {
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.core.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}

struct CustomWriter<T: CpuSample> {
    core: PortCore,
    data: Vec<T>,
    tags: Vec<ItemTag>,
}

impl<T: CpuSample> Default for CustomWriter<T> {
    fn default() -> Self {
        Self {
            core: PortCore::new_unbound(),
            data: Vec::new(),
            tags: Vec::new(),
        }
    }
}

impl<T: CpuSample> BufferWriter for CustomWriter<T> {
    type Inbox = BlockInbox;
    type Reader = CustomReader<T>;

    fn max_readers(&self) -> usize {
        usize::MAX
    }

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }

    fn connect(&mut self, _dest: &mut Self::Reader) {}

    async fn notify_finished(&mut self) {}

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<T: CpuSample> CpuBufferWriter for CustomWriter<T> {
    type Item = T;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        let Self { data, tags, .. } = self;
        (data.as_mut_slice(), Tags::new(tags, 0))
    }

    fn produce(&mut self, _n: usize) {}

    fn set_min_items(&mut self, n: usize) {
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.core.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}

#[test]
fn custom_cpu_buffer_can_use_public_requirement_api() {
    let mut initial = BufferRequirements::with_min_items(2);
    initial.set_min_buffer_size_in_items(16);
    let mut core = PortCore::<BlockInbox>::with_requirements(initial);

    let mut requirements = core.requirements();
    requirements.raise_min_items(4);
    requirements.raise_min_buffer_size_in_items(32);

    assert_eq!(requirements.min_items(), Some(4));
    assert_eq!(requirements.min_buffer_size_in_items(), Some(32));

    core.raise_requirements(requirements);
    assert_eq!(core.min_items(), Some(4));
    assert_eq!(core.min_buffer_size_in_items(), Some(32));
}

#[test]
fn custom_cpu_buffer_can_publish_and_absorb_requirements() {
    let mut writer = CustomWriter::<u8>::default();
    CpuBufferWriter::set_min_items(&mut writer, 4);
    CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 32);

    let mut reader = CustomReader::<u8>::default();
    CpuBufferReader::set_min_items(&mut reader, 8);
    CpuBufferReader::set_min_buffer_size_in_items(&mut reader, 64);

    let mut writer_requirements = BufferWriter::buffer_requirements(&writer);
    let reader_requirements = BufferReader::buffer_requirements(&reader);
    writer_requirements.merge(reader_requirements);

    assert_eq!(writer_requirements.min_items(), Some(8));
    assert_eq!(writer_requirements.min_buffer_size_in_items(), Some(64));
    assert_eq!(BufferWriter::max_readers(&writer), usize::MAX);

    BufferWriter::raise_buffer_requirements(&mut writer, writer_requirements);
    let raised = BufferWriter::buffer_requirements(&writer);
    assert_eq!(raised.min_items(), Some(8));
    assert_eq!(raised.min_buffer_size_in_items(), Some(64));
}
