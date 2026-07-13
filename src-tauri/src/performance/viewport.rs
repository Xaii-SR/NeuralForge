use std::ffi::c_void;
use thiserror::Error;

#[derive(Debug, Error, serde::Serialize, serde::Deserialize, Clone)]
pub enum ViewportError {
    #[error("Failed to initialize system graphics context")]
    ContextInitializationFailed,
    #[error("Texture allocation failed of size {0}x{1}")]
    AllocationFailed(u32, u32),
    #[error("Platform capability not supported: {0}")]
    UnsupportedPlatform(String),
    #[error("Internal FFI execution failure: {0}")]
    FfiError(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TextureHandleDescriptor {
    pub handle_id: u64,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

pub trait SharedTextureEngine: Send + Sync {
    fn create_shared_texture(&self, width: u32, height: u32) -> Result<TextureHandleDescriptor, ViewportError>;
    fn release_texture(&self, handle_id: u64) -> Result<(), ViewportError>;
}

#[cfg(target_os = "windows")]
pub struct DxgiTextureEngine;

#[cfg(target_os = "windows")]
extern "system" {
    fn CloseHandle(hObject: isize) -> i32;
}

#[cfg(target_os = "windows")]
impl SharedTextureEngine for DxgiTextureEngine {
    fn create_shared_texture(&self, width: u32, height: u32) -> Result<TextureHandleDescriptor, ViewportError> {
        let mock_handle: isize = 0x4242_isize;
        if mock_handle == 0 {
            return Err(ViewportError::AllocationFailed(width, height));
        }
        Ok(TextureHandleDescriptor {
            handle_id: mock_handle as u64,
            width,
            height,
            format: "RGBA8_UNORM".to_string(),
        })
    }

    fn release_texture(&self, handle_id: u64) -> Result<(), ViewportError> {
        unsafe {
            if handle_id != 0 && CloseHandle(handle_id as isize) == 0 {
                // Handle already managed or cleared safely
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub struct IoSurfaceTextureEngine;

#[cfg(target_os = "macos")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[cfg(target_os = "macos")]
impl SharedTextureEngine for IoSurfaceTextureEngine {
    fn create_shared_texture(&self, width: u32, height: u32) -> Result<TextureHandleDescriptor, ViewportError> {
        let mock_surface_id: u64 = 0x9999_u64;
        Ok(TextureHandleDescriptor {
            handle_id: mock_surface_id,
            width,
            height,
            format: "BGRA8_UNORM".to_string(),
        })
    }

    fn release_texture(&self, handle_id: u64) -> Result<(), ViewportError> {
        unsafe {
            if handle_id == 0 {
                return Err(ViewportError::FfiError("Invalid surface identifier".to_string()));
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
pub struct DmaBufTextureEngine;

#[cfg(target_os = "linux")]
impl SharedTextureEngine for DmaBufTextureEngine {
    fn create_shared_texture(&self, width: u32, height: u32) -> Result<TextureHandleDescriptor, ViewportError> {
        let mock_fd: i32 = 101;
        Ok(TextureHandleDescriptor {
            handle_id: mock_fd as u64,
            width,
            height,
            format: "RGBA8".to_string(),
        })
    }

    fn release_texture(&self, handle_id: u64) -> Result<(), ViewportError> {
        unsafe {
            if handle_id != 0 {
                libc::close(handle_id as libc::c_int);
            }
            Ok(())
        }
    }
}

pub struct ViewportManager {
    engine: Box<dyn SharedTextureEngine>,
}

unsafe impl Send for ViewportManager {}
unsafe impl Sync for ViewportManager {}

impl ViewportManager {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        let engine = Box::new(DxgiTextureEngine);

        #[cfg(target_os = "macos")]
        let engine = Box::new(IoSurfaceTextureEngine);

        #[cfg(target_os = "linux")]
        let engine = Box::new(DmaBufTextureEngine);

        Self { engine }
    }

    pub fn allocate_frame(&self, width: u32, height: u32) -> Result<TextureHandleDescriptor, ViewportError> {
        self.engine.create_shared_texture(width, height)
    }

    pub fn free_frame(&self, handle_id: u64) -> Result<(), ViewportError> {
        self.engine.release_texture(handle_id)
    }
}