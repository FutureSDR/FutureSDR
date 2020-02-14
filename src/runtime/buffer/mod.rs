#[allow(clippy::module_inception)]
mod buffer;
pub use buffer::BufferBuilder;
pub use buffer::BufferReader;
pub use buffer::BufferReaderCustom;
pub use buffer::BufferReaderHost;
pub use buffer::BufferWriter;
pub use buffer::BufferWriterCustom;
pub use buffer::BufferWriterHost;

// ==================== CIRCULAR =======================
#[cfg(windows)]
mod double_create_file_mapping;
#[cfg(windows)]
use double_create_file_mapping::DoubleCreateFileMapping as DoubleMapped;

#[cfg(unix)]
mod double_mapped_temp_file;
#[cfg(unix)]
use double_mapped_temp_file::DoubleMappedTempFile as DoubleMapped;

#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
pub mod slab;

// ==================== VULKAN =======================
#[cfg(feature = "vulkan")]
pub mod vulkan;

// // -==================== ZYNQ ========================
#[cfg(feature = "zynq")]
pub mod zynq;

// =================== PAGESIZE ======================
#[cfg(unix)]
pub fn pagesize() -> usize {
    unsafe {
        let ps = libc::sysconf(libc::_SC_PAGESIZE);
        if ps < 0 {
            panic!("could not determince page size");
        }
        ps as usize
    }
}

#[cfg(windows)]
use winapi::um::sysinfoapi::GetSystemInfo;
#[cfg(windows)]
use winapi::um::sysinfoapi::SYSTEM_INFO;

#[cfg(windows)]
pub fn pagesize() -> usize {
    unsafe {
        let mut info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut info);
        info.dwAllocationGranularity as usize
    }
}
