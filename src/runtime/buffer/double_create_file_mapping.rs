use anyhow::{bail, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use winapi::shared::minwindef::LPVOID;
use winapi::um::handleapi::CloseHandle;
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::memoryapi::MapViewOfFileEx;
use winapi::um::memoryapi::VirtualAlloc;
use winapi::um::memoryapi::VirtualFree;
use winapi::um::winnt::MEM_RELEASE;
use winapi::um::winnt::MEM_RESERVE;
use winapi::um::winnt::PAGE_NOACCESS;
use winapi::um::winnt::PAGE_READWRITE;
use winapi::um::{
    memoryapi::{UnmapViewOfFile, FILE_MAP_WRITE},
    winbase::CreateFileMappingA,
};

use crate::runtime::buffer::pagesize;

static SEGMENTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct DoubleCreateFileMapping {
    addr: *mut libc::c_void,
    handle: *mut libc::c_void,
    size: usize,
}

impl DoubleCreateFileMapping {
    pub fn new(size: usize) -> Result<DoubleCreateFileMapping> {
        for _ in 0..5 {
            let ret = Self::new_try(size);
            if ret.is_ok() {
                return ret;
            }
        }
        Self::new_try(size)
    }

    fn new_try(size: usize) -> Result<DoubleCreateFileMapping> {
        let page_size = pagesize();
        if size % page_size != 0 {
            bail!(
                "size ({}) not a multiple of page size ({})",
                size,
                page_size
            );
        }

        let s = SEGMENTS.fetch_add(1, Ordering::SeqCst);
        let seg_name = format!("futuresdr-{}-{}", std::process::id(), s);

        unsafe {
            let handle = CreateFileMappingA(
                INVALID_HANDLE_VALUE,
                std::mem::zeroed(),
                PAGE_READWRITE,
                0,
                size as u32,
                seg_name.as_ptr() as *const i8,
            );

            if handle == INVALID_HANDLE_VALUE || handle == 0 as LPVOID {
                bail!("Failed to create file mapping.");
            }

            let first_tmp = VirtualAlloc(0 as LPVOID, 2 * size, MEM_RESERVE, PAGE_NOACCESS);
            if first_tmp == 0 as LPVOID {
                CloseHandle(handle);
                bail!("Failed to map first segment.");
            }

            let res = VirtualFree(first_tmp, 0, MEM_RELEASE);
            if res == 0 {
                CloseHandle(handle);
                bail!("Failed to free double-sized segment.")
            }

            let first_cpy = MapViewOfFileEx(handle, FILE_MAP_WRITE, 0, 0, size, first_tmp);
            if first_tmp != first_cpy {
                CloseHandle(handle);
                bail!("Failed to map first segement at correct address.")
            }

            let second_cpy =
                MapViewOfFileEx(handle, FILE_MAP_WRITE, 0, 0, size, first_tmp.add(size));
            if second_cpy != first_tmp.add(size) {
                UnmapViewOfFile(first_cpy);
                CloseHandle(handle);
                bail!("Failed to map second segement at correct address.")
            }

            Ok(DoubleCreateFileMapping {
                addr: first_tmp as *mut libc::c_void,
                handle,
                size,
            })
        }
    }

    pub fn addr(&self) -> *mut libc::c_void {
        self.addr
    }
}

impl Drop for DoubleCreateFileMapping {
    fn drop(&mut self) {
        unsafe {
            if UnmapViewOfFile(self.addr) == 0 {
                info!("failed to unmap first copy of buffer");
            }
            if UnmapViewOfFile(self.addr.add(self.size)) == 0 {
                info!("failed to unmap second copy of buffer");
            }
            if CloseHandle(self.handle) == 0 {
                info!("failed to close buffer handle");
            }
        }
    }
}
