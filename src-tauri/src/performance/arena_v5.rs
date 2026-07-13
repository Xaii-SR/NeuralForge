use std::sync::atomic::{AtomicU64, Ordering};
use std::ptr;
use thiserror::Error;

pub const WRAP_SENTINEL: u32 = u32::MAX;
pub const FLAG_COMMITTED: u16 = 0b0000_0001;

#[repr(C, align(64))]
pub struct SlotHeader {
    pub seq: AtomicU64,
    pub len: u32,
    pub kind: u16,
    pub flags: u16,
}

#[repr(C, align(64))]
pub struct ArenaControl {
    pub write_head: AtomicU64,
    pub _pad0: [u8; 56],
    pub read_head: AtomicU64,
    pub _pad1: [u8; 56],
    pub capacity: u64,
    pub epoch: AtomicU64,
}

#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("Arena capacity exceeded")]
    CapacityExceeded,
    #[error("Arena backpressure: reader is too slow")]
    Backpressure,
    #[error("OS Mapping Error: {0}")]
    MappingError(String),
}

pub trait MemoryMapper: Send + Sync {
    fn map(&self, capacity: usize) -> Result<*mut u8, ArenaError>;
    fn unmap(&self, ptr: *mut u8, capacity: usize) -> Result<(), ArenaError>;
}

#[cfg(target_os = "linux")]
pub struct NativeMapper;

#[cfg(target_os = "linux")]
impl MemoryMapper for NativeMapper {
    fn map(&self, capacity: usize) -> Result<*mut u8, ArenaError> {
        unsafe {
            let fd = libc::memfd_create(b"neural_forge_arena\0".as_ptr() as *const libc::c_char, libc::MFD_ALLOW_SEALING);
            if fd < 0 {
                return Err(ArenaError::MappingError("Failed to create memfd".to_string()));
            }
            if libc::ftruncate(fd, capacity as libc::off_t) < 0 {
                libc::close(fd);
                return Err(ArenaError::MappingError("Failed to truncate memfd".to_string()));
            }
            libc::fcntl(fd, libc::F_ADD_SEALS, libc::F_SEAL_SHRINK | libc::F_SEAL_GROW);

            let ptr = libc::mmap(
                ptr::null_mut(),
                capacity,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if ptr == libc::MAP_FAILED {
                libc::close(fd);
                return Err(ArenaError::MappingError("mmap failed".to_string()));
            }
            Ok(ptr as *mut u8)
        }
    }

    fn unmap(&self, ptr: *mut u8, capacity: usize) -> Result<(), ArenaError> {
        unsafe {
            if libc::munmap(ptr as *mut libc::c_void, capacity) < 0 {
                return Err(ArenaError::MappingError("munmap failed".to_string()));
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub struct NativeMapper;

#[cfg(target_os = "macos")]
impl MemoryMapper for NativeMapper {
    fn map(&self, capacity: usize) -> Result<*mut u8, ArenaError> {
        unsafe {
            let ptr = libc::mmap(
                ptr::null_mut(),
                capacity,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANON | libc::MAP_SHARED,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return Err(ArenaError::MappingError("mmap failed".to_string()));
            }
            Ok(ptr as *mut u8)
        }
    }

    fn unmap(&self, ptr: *mut u8, capacity: usize) -> Result<(), ArenaError> {
        unsafe {
            if libc::munmap(ptr as *mut libc::c_void, capacity) < 0 {
                return Err(ArenaError::MappingError("munmap failed".to_string()));
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
pub struct NativeMapper;

#[cfg(target_os = "windows")]
extern "system" {
    fn CreateFileMappingW(
        hFile: isize,
        lpFileMappingAttributes: *const std::ffi::c_void,
        flProtect: u32,
        dwMaximumSizeHigh: u32,
        dwMaximumSizeLow: u32,
        lpName: *const u16,
    ) -> isize;

    fn MapViewOfFile(
        hFileMappingObject: isize,
        dwDesiredAccess: u32,
        dwFileOffsetHigh: u32,
        dwFileOffsetLow: u32,
        dwNumberOfBytesToMap: usize,
    ) -> *mut std::ffi::c_void;

    fn UnmapViewOfFile(lpBaseAddress: *const std::ffi::c_void) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
}

#[cfg(target_os = "windows")]
impl MemoryMapper for NativeMapper {
    fn map(&self, capacity: usize) -> Result<*mut u8, ArenaError> {
        unsafe {
            let handle = CreateFileMappingW(-1, ptr::null(), 0x04, (capacity >> 32) as u32, (capacity & 0xFFFFFFFF) as u32, ptr::null());
            if handle == 0 {
                return Err(ArenaError::MappingError("CreateFileMappingW failed".to_string()));
            }
            let ptr = MapViewOfFile(handle, 0x000F001F, 0, 0, capacity);
            CloseHandle(handle);
            if ptr.is_null() {
                return Err(ArenaError::MappingError("MapViewOfFile failed".to_string()));
            }
            Ok(ptr as *mut u8)
        }
    }

    fn unmap(&self, ptr: *mut u8, _capacity: usize) -> Result<(), ArenaError> {
        unsafe {
            if UnmapViewOfFile(ptr as *const std::ffi::c_void) == 0 {
                return Err(ArenaError::MappingError("UnmapViewOfFile failed".to_string()));
            }
            Ok(())
        }
    }
}

pub struct SharedRingBufferArena {
    storage_ptr: *mut u8,
    capacity: usize,
    mapper: NativeMapper,
}

unsafe impl Send for SharedRingBufferArena {}
unsafe impl Sync for SharedRingBufferArena {}

impl SharedRingBufferArena {
    pub fn new(capacity: usize) -> Result<Self, ArenaError> {
        let mapper = NativeMapper;
        let storage_ptr = mapper.map(capacity)?;

        unsafe {
            let control_ptr = storage_ptr as *mut ArenaControl;
            let ctrl_size = std::mem::size_of::<ArenaControl>() as u64;
            (*control_ptr).write_head.store(ctrl_size, Ordering::SeqCst);
            (*control_ptr).read_head.store(ctrl_size, Ordering::SeqCst);
            (*control_ptr).capacity = capacity as u64;
            (*control_ptr).epoch.store(1, Ordering::SeqCst);
        }

        Ok(Self { storage_ptr, capacity, mapper })
    }

    pub fn get_control(&self) -> &ArenaControl {
        unsafe { &*(self.storage_ptr as *const ArenaControl) }
    }

    #[inline]
    fn align_size(size: usize) -> usize {
        (size + 7) & !7
    }

    pub fn reserve_slot(&self, len: u32, kind: u16) -> Result<u64, ArenaError> {
        let control = self.get_control();
        let header_size = std::mem::size_of::<SlotHeader>();
        let payload_size = Self::align_size(len as usize);
        let total_size = header_size + payload_size;
        let ctrl_size = std::mem::size_of::<ArenaControl>() as u64;

        loop {
            let current_write = control.write_head.load(Ordering::Relaxed);
            let current_read = control.read_head.load(Ordering::Acquire);

            let mut target_write = current_write;
            let mut wrap_needed = false;

            if target_write + total_size as u64 > control.capacity {
                if current_read > ctrl_size && current_read <= ctrl_size + total_size as u64 {
                    return Err(ArenaError::Backpressure);
                }
                if current_read <= ctrl_size || current_read > target_write {
                    target_write = ctrl_size;
                    wrap_needed = true;
                } else {
                    return Err(ArenaError::Backpressure);
                }
            } else {
                if current_write < current_read && current_write + total_size as u64 >= current_read {
                    return Err(ArenaError::Backpressure);
                }
            }

            let next_write = target_write + total_size as u64;

            if wrap_needed && target_write == ctrl_size {
                let old_space_left = control.capacity - current_write;
                if old_space_left >= header_size as u64 {
                    unsafe {
                        let header_ptr = self.storage_ptr.add(current_write as usize) as *mut SlotHeader;
                        (*header_ptr).len = WRAP_SENTINEL;
                        (*header_ptr).kind = 0;
                        (*header_ptr).flags = FLAG_COMMITTED;
                        let epoch = control.epoch.load(Ordering::Relaxed);
                        (*header_ptr).seq.store(epoch, Ordering::Release);
                    }
                }
            }

            if control.write_head.compare_exchange_weak(
                current_write,
                next_write,
                Ordering::Release,
                Ordering::Relaxed,
            ).is_ok() {
                unsafe {
                    let header_ptr = self.storage_ptr.add(target_write as usize) as *mut SlotHeader;
                    (*header_ptr).len = len;
                    (*header_ptr).kind = kind;
                    (*header_ptr).flags = 0;
                }
                return Ok(target_write);
            }
        }
    }

    pub unsafe fn write_payload(&self, offset: u64, data: &[u8]) {
        let header_size = std::mem::size_of::<SlotHeader>();
        let dest_ptr = self.storage_ptr.add((offset as usize) + header_size);
        ptr::copy_nonoverlapping(data.as_ptr(), dest_ptr, data.len());
    }

    pub fn commit_slot(&self, offset: u64, seq_num: u64) {
        unsafe {
            let header_ptr = self.storage_ptr.add(offset as usize) as *mut SlotHeader;
            (*header_ptr).flags |= FLAG_COMMITTED;
            (*header_ptr).seq.store(seq_num, Ordering::Release);
        }
    }

    pub fn read_next_slot(&self, expected_seq: u64) -> Result<Option<(u64, u32, u16, &[u8])>, ArenaError> {
        let control = self.get_control();
        let current_read = control.read_head.load(Ordering::Acquire);
        let current_write = control.write_head.load(Ordering::Acquire);

        if current_read == current_write {
            return Ok(None);
        }

        let header_size = std::mem::size_of::<SlotHeader>();
        unsafe {
            let header_ptr = self.storage_ptr.add(current_read as usize) as *mut SlotHeader;
            let flags = (*header_ptr).flags;

            if (flags & FLAG_COMMITTED) == 0 {
                return Ok(None);
            }

            let seq = (*header_ptr).seq.load(Ordering::Acquire);
            if seq != expected_seq {
                return Ok(None);
            }

            let len = (*header_ptr).len;
            let kind = (*header_ptr).kind;

            if len == WRAP_SENTINEL {
                let ctrl_size = std::mem::size_of::<ArenaControl>() as u64;
                control.read_head.store(ctrl_size, Ordering::Release);
                return self.read_next_slot(expected_seq);
            }

            let payload_size = Self::align_size(len as usize);
            let next_read = current_read + (header_size + payload_size) as u64;

            let data_ptr = self.storage_ptr.add(current_read as usize + header_size);
            let data_slice = std::slice::from_raw_parts(data_ptr, len as usize);

            control.read_head.store(next_read, Ordering::Release);

            Ok(Some((current_read, len, kind, data_slice)))
        }
    }
}

impl Drop for SharedRingBufferArena {
    fn drop(&mut self) {
        let _ = self.mapper.unmap(self.storage_ptr, self.capacity);
    }
}