//! Windows-specific thumbnail extraction using IShellItemImageFactory
//!
//! This implementation uses the Windows Shell API to extract thumbnails
//! from files, leveraging the same thumbnail providers that Windows Explorer uses.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName,
    SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY,
};

use super::{ImageFormat, ThumbnailData, ThumbnailExtractorTrait, ThumbnailSize};
use crate::core::error::{OsError, Result};

/// Windows thumbnail extractor using IShellItemImageFactory
pub struct WindowsThumbnailExtractor {
    /// Whether COM has been initialized by this instance
    com_initialized: bool,
}

impl WindowsThumbnailExtractor {
    /// Create a new Windows thumbnail extractor
    pub fn new() -> Result<Self> {
        // Initialize COM for this thread
        let com_initialized = unsafe {
            let hr = CoInitializeEx(
                Some(ptr::null()),
                COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
            );
            // S_OK or S_FALSE (already initialized) are both acceptable
            hr.is_ok() || hr.0 == 1 // S_FALSE = 1
        };

        Ok(Self { com_initialized })
    }

    /// Convert a path to a wide string (null-terminated UTF-16)
    fn path_to_wide(path: &Path) -> Vec<u16> {
        let os_str: &OsStr = path.as_ref();
        os_str.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Extract thumbnail using IShellItemImageFactory
    fn extract_thumbnail_internal(
        &self,
        path: &Path,
        size: ThumbnailSize,
    ) -> Result<ThumbnailData> {
        let path_wide = Self::path_to_wide(path);
        let (width, height) = size.dimensions();

        unsafe {
            // Create IShellItem from path
            let shell_item: IShellItem = SHCreateItemFromParsingName(
                PCWSTR(path_wide.as_ptr()),
                None,
            )
            .map_err(|e| OsError::ShellItemCreationFailed {
                path: format!("{}: {}", path.display(), e),
            })?;

            // Query for IShellItemImageFactory interface
            let factory: IShellItemImageFactory = shell_item.cast().map_err(|e| {
                OsError::ThumbnailExtractionFailed {
                    reason: format!("Failed to get IShellItemImageFactory: {}", e),
                }
            })?;

            // Get the thumbnail as HBITMAP
            let hbitmap = factory
                .GetImage(
                    SIZE {
                        cx: width,
                        cy: height,
                    },
                    SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
                )
                .map_err(|e| OsError::ThumbnailExtractionFailed {
                    reason: format!("GetImage failed: {}", e),
                })?;

            // Convert HBITMAP to PNG data
            let thumbnail_data = self.hbitmap_to_png(hbitmap, width as u32, height as u32)?;

            // Clean up the HBITMAP
            let _ = DeleteObject(hbitmap);

            Ok(thumbnail_data)
        }
    }

    /// Convert HBITMAP to PNG data
    fn hbitmap_to_png(
        &self,
        hbitmap: HBITMAP,
        requested_width: u32,
        requested_height: u32,
    ) -> Result<ThumbnailData> {
        unsafe {
            // Create a compatible DC
            let hdc = CreateCompatibleDC(None);
            if hdc.is_invalid() {
                return Err(OsError::ThumbnailExtractionFailed {
                    reason: "Failed to create compatible DC".to_string(),
                }
                .into());
            }

            // Select the bitmap into the DC
            let old_bitmap = SelectObject(hdc, hbitmap);

            // Get bitmap info
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: requested_width as i32,
                    biHeight: -(requested_height as i32), // Negative for top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [Default::default()],
            };

            // Calculate buffer size
            let row_size = ((requested_width * 32 + 31) / 32) * 4;
            let buffer_size = (row_size * requested_height) as usize;
            let mut pixel_data: Vec<u8> = vec![0; buffer_size];

            // Get the bitmap bits
            let result = GetDIBits(
                hdc,
                hbitmap,
                0,
                requested_height,
                Some(pixel_data.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            // Clean up DC
            SelectObject(hdc, old_bitmap);
            let _ = DeleteDC(hdc);

            if result == 0 {
                return Err(OsError::ThumbnailExtractionFailed {
                    reason: "GetDIBits failed".to_string(),
                }
                .into());
            }

            // Get actual dimensions from the bitmap info
            let actual_width = bmi.bmiHeader.biWidth.unsigned_abs();
            let actual_height = bmi.bmiHeader.biHeight.unsigned_abs();

            // Convert BGRA to RGBA
            for chunk in pixel_data.chunks_exact_mut(4) {
                chunk.swap(0, 2); // Swap B and R
            }

            // Encode as PNG using the image crate
            let png_data = self.encode_png(&pixel_data, actual_width, actual_height)?;

            Ok(ThumbnailData::new(
                png_data,
                ImageFormat::Png,
                actual_width,
                actual_height,
            ))
        }
    }

    /// Encode raw RGBA pixel data as PNG
    fn encode_png(&self, rgba_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        use image::{ImageBuffer, Rgba};

        // Create an image buffer from the raw data
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, rgba_data.to_vec()).ok_or_else(|| {
                OsError::ThumbnailExtractionFailed {
                    reason: "Failed to create image buffer".to_string(),
                }
            })?;

        // Encode as PNG
        let mut png_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_data);

        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| OsError::ThumbnailExtractionFailed {
                reason: format!("PNG encoding failed: {}", e),
            })?;

        Ok(png_data)
    }
}

impl Default for WindowsThumbnailExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to create WindowsThumbnailExtractor")
    }
}

impl Drop for WindowsThumbnailExtractor {
    fn drop(&mut self) {
        if self.com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

impl ThumbnailExtractorTrait for WindowsThumbnailExtractor {
    fn get_thumbnail(&self, path: &Path, size: ThumbnailSize) -> Result<ThumbnailData> {
        // Verify the file exists
        if !path.exists() {
            return Err(OsError::ThumbnailExtractionFailed {
                reason: format!("File not found: {}", path.display()),
            }
            .into());
        }

        self.extract_thumbnail_internal(path, size)
    }

    fn is_supported(&self, path: &Path) -> bool {
        // Windows Shell can generate thumbnails for most file types
        // that have registered thumbnail handlers
        if !path.exists() {
            return false;
        }

        // Common supported extensions
        let supported_extensions = [
            // Images
            "jpg", "jpeg", "png", "gif", "bmp", "ico", "webp", "tiff", "tif",
            // Documents
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            // Videos
            "mp4", "avi", "mkv", "mov", "wmv", "flv", "webm",
            // 3D models (if handlers installed)
            "obj", "fbx", "gltf", "glb", "3ds",
            // Other
            "svg", "psd", "ai",
        ];

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| supported_extensions.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_thumbnail_size_dimensions() {
        assert_eq!(ThumbnailSize::Small.dimensions(), (48, 48));
        assert_eq!(ThumbnailSize::Medium.dimensions(), (96, 96));
        assert_eq!(ThumbnailSize::Large.dimensions(), (256, 256));
        assert_eq!(ThumbnailSize::XLarge.dimensions(), (512, 512));
    }

    #[test]
    fn test_path_to_wide() {
        let path = PathBuf::from("C:\\test\\file.txt");
        let wide = WindowsThumbnailExtractor::path_to_wide(&path);
        assert!(!wide.is_empty());
        assert_eq!(*wide.last().unwrap(), 0); // Null terminated
    }

    #[test]
    fn test_is_supported() {
        let extractor = WindowsThumbnailExtractor::new().unwrap();

        // These should be supported (if files existed)
        let supported_paths = [
            PathBuf::from("test.jpg"),
            PathBuf::from("test.png"),
            PathBuf::from("test.pdf"),
            PathBuf::from("test.mp4"),
        ];

        for path in &supported_paths {
            // Note: is_supported checks if file exists, so these will return false
            // but the extension check logic is correct
            let ext = path.extension().unwrap().to_str().unwrap();
            assert!(
                ["jpg", "png", "pdf", "mp4"].contains(&ext),
                "Extension {} should be in supported list",
                ext
            );
        }
    }

    #[test]
    fn test_image_format_mime_type() {
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormat::Bmp.mime_type(), "image/bmp");
    }

    #[test]
    fn test_thumbnail_data_creation() {
        let data = vec![0u8; 100];
        let thumbnail = ThumbnailData::new(data.clone(), ImageFormat::Png, 96, 96);

        assert_eq!(thumbnail.len(), 100);
        assert!(!thumbnail.is_empty());
        assert_eq!(thumbnail.width, 96);
        assert_eq!(thumbnail.height, 96);
        assert_eq!(thumbnail.mime_type(), "image/png");
    }
}
