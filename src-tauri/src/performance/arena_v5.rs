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
            let fd = libc::memfd_create(
                b"neural_forge_arena\0".as_ptr() as *const libc::c_char,
                libc::MFD_ALLOW_SEALING,
            );
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
impl MemoryMapper for NativeMapper {
    fn map(&self, capacity: usize) -> Result<*mut u8, ArenaError> {
        extern "system" {
            fn CreateFileMappingW(
                hFile: isize,
                lpAttributes: *const std::ffi::c_void,
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

        const INVALID_HANDLE_VALUE: isize = -1;
        const PAGE_READWRITE: u32 = 0x04;
        const FILE_MAP_ALL_ACCESS: u32 = 0xF001F;

        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                (capacity >> 32) as u32,
                (capacity & 0xFFFF_FFFF) as u32,
                ptr::null(),
            );

            if handle == 0 {
                return Err(ArenaError::MappingError("CreateFileMappingW failed".to_string()));
            }

            let view_ptr = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, capacity);

            if view_ptr.is_null() {
                CloseHandle(handle);
                return Err(ArenaError::MappingError("MapViewOfFile failed".to_string()));
            }

            Ok(view_ptr as *mut u8)
        }
    }

    fn unmap(&self, ptr: *mut u8, _capacity: usize) -> Result<(), ArenaError> {
        extern "system" {
            fn UnmapViewOfFile(lpBaseAddress: *const std::ffi::c_void) -> i32;
        }
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
            (*control_ptr).write_head.store(
                std::mem::size_of::<ArenaControl>() as u64,
                Ordering::SeqCst,
            );
            (*control_ptr).read_head.store(
                std::mem::size_of::<ArenaControl>() as u64,
                Ordering::SeqCst,
            );
            (*control_ptr).capacity = capacity as u64;
            (*control_ptr).epoch.store(1, Ordering::SeqCst);
        }

        Ok(Self {
            storage_ptr,
            capacity,
            mapper,
        })
    }

    pub fn get_control(&self) -> &ArenaControl {
        unsafe { &*(self.storage_ptr as *const ArenaControl) }
    }
}

impl Drop for SharedRingBufferArena {
    fn drop(&mut self) {
        let _ = self.mapper.unmap(self.storage_ptr, self.capacity);
    }
}