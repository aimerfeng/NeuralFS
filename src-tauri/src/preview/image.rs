//! Image Preview Generator
//!
//! Generates image previews with scaling and region marking.
//! Supports common image formats: PNG, JPEG, GIF, WebP, BMP, etc.

use super::{PreviewConfig, PreviewError};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;
use uuid::Uuid;

/// Image preview generator
pub struct ImagePreviewGenerator {
    config: PreviewConfig,
}

impl ImagePreviewGenerator {
    /// Create a new image preview generator
    pub fn new(config: PreviewConfig) -> Self {
        Self { config }
    }

    /// Generate an image preview from a file
    pub async fn generate(
        &self,
        path: &Path,
        file_id: Uuid,
        region: Option<(f32, f32, f32, f32)>,
    ) -> Result<ImagePreview, PreviewError> {
        if !path.exists() {
            return Err(PreviewError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        // Read and decode image (blocking operation)
        let path_owned = path.to_path_buf();
        let config = self.config.clone();

        let result = tokio::task::spawn_blocking(move || {
            Self::process_image(&path_owned, file_id, region, &config)
        })
        .await
        .map_err(|e| PreviewError::GenerationFailed {
            reason: format!("Task join error: {}", e),
        })??;

        Ok(result)
    }

    /// Process image synchronously
    fn process_image(
        path: &Path,
        file_id: Uuid,
        region: Option<(f32, f32, f32, f32)>,
        config: &PreviewConfig,
    ) -> Result<ImagePreview, PreviewError> {
        let img = image::open(path).map_err(|e| PreviewError::ImageError {
            reason: format!("Failed to open image: {}", e),
        })?;

        let (original_width, original_height) = img.dimensions();

        // Scale image to fit within max dimensions
        let scaled = Self::scale_image(&img, config.max_image_width, config.max_image_height);
        let (preview_width, preview_height) = scaled.dimensions();

        // Apply region marker if specified
        let (final_image, region_marker) = if let Some((x, y, w, h)) = region {
            let marked = Self::draw_region_marker(&scaled, x, y, w, h);
            let marker = RegionMarker {
                x,
                y,
                width: w,
                height: h,
                color: "#FF0000".to_string(),
                stroke_width: 2,
            };
            (marked, Some(marker))
        } else {
            (scaled, None)
        };

        // Encode to JPEG
        let mut buffer = Cursor::new(Vec::new());
        final_image
            .write_to(&mut buffer, ImageFormat::Jpeg)
            .map_err(|e| PreviewError::ImageError {
                reason: format!("Failed to encode image: {}", e),
            })?;

        let data = buffer.into_inner();
        let content_type = "image/jpeg".to_string();

        Ok(ImagePreview {
            file_id,
            data,
            content_type,
            original_width,
            original_height,
            preview_width,
            preview_height,
            region_marker,
        })
    }

    /// Scale image to fit within max dimensions while preserving aspect ratio
    fn scale_image(img: &DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
        let (width, height) = img.dimensions();

        // Check if scaling is needed
        if width <= max_width && height <= max_height {
            return img.clone();
        }

        // Calculate scale factor
        let width_ratio = max_width as f64 / width as f64;
        let height_ratio = max_height as f64 / height as f64;
        let scale = width_ratio.min(height_ratio);

        let new_width = (width as f64 * scale) as u32;
        let new_height = (height as f64 * scale) as u32;

        img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    }

    /// Draw a region marker (rectangle) on the image
    fn draw_region_marker(
        img: &DynamicImage,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> DynamicImage {
        let mut rgba_img = img.to_rgba8();
        let (img_width, img_height) = rgba_img.dimensions();

        // Convert normalized coordinates to pixel coordinates
        let px_x = (x * img_width as f32) as i32;
        let px_y = (y * img_height as f32) as i32;
        let px_w = (width * img_width as f32) as i32;
        let px_h = (height * img_height as f32) as i32;

        // Red color for the marker
        let color = Rgba([255, 0, 0, 255]);
        let stroke_width = 2;

        // Draw rectangle outline
        Self::draw_rect_outline(&mut rgba_img, px_x, px_y, px_w, px_h, color, stroke_width);

        DynamicImage::ImageRgba8(rgba_img)
    }

    /// Draw a rectangle outline on an RGBA image
    fn draw_rect_outline(
        img: &mut image::RgbaImage,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: Rgba<u8>,
        stroke_width: i32,
    ) {
        let (img_width, img_height) = img.dimensions();
        let img_width = img_width as i32;
        let img_height = img_height as i32;

        // Draw top and bottom edges
        for dx in 0..width {
            for s in 0..stroke_width {
                // Top edge
                let px = x + dx;
                let py = y + s;
                if px >= 0 && px < img_width && py >= 0 && py < img_height {
                    img.put_pixel(px as u32, py as u32, color);
                }
                // Bottom edge
                let py = y + height - 1 - s;
                if px >= 0 && px < img_width && py >= 0 && py < img_height {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }

        // Draw left and right edges
        for dy in 0..height {
            for s in 0..stroke_width {
                // Left edge
                let px = x + s;
                let py = y + dy;
                if px >= 0 && px < img_width && py >= 0 && py < img_height {
                    img.put_pixel(px as u32, py as u32, color);
                }
                // Right edge
                let px = x + width - 1 - s;
                if px >= 0 && px < img_width && py >= 0 && py < img_height {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

/// Image preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePreview {
    /// File UUID
    pub file_id: Uuid,
    /// Encoded image data (JPEG)
    #[serde(with = "base64_serde")]
    pub data: Vec<u8>,
    /// Content type (MIME type)
    pub content_type: String,
    /// Original image width
    pub original_width: u32,
    /// Original image height
    pub original_height: u32,
    /// Preview image width
    pub preview_width: u32,
    /// Preview image height
    pub preview_height: u32,
    /// Region marker if specified
    pub region_marker: Option<RegionMarker>,
}

/// Region marker for highlighting areas in images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionMarker {
    /// X coordinate (normalized 0-1)
    pub x: f32,
    /// Y coordinate (normalized 0-1)
    pub y: f32,
    /// Width (normalized 0-1)
    pub width: f32,
    /// Height (normalized 0-1)
    pub height: f32,
    /// Marker color (hex)
    pub color: String,
    /// Stroke width in pixels
    pub stroke_width: u32,
}

/// Base64 serialization for binary data
mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use tempfile::NamedTempFile;

    fn create_test_image(width: u32, height: u32) -> NamedTempFile {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });

        let file = NamedTempFile::with_suffix(".png").unwrap();
        img.save(file.path()).unwrap();
        file
    }

    #[tokio::test]
    async fn test_generate_image_preview() {
        let file = create_test_image(100, 100);
        let config = PreviewConfig::default();
        let generator = ImagePreviewGenerator::new(config);
        let file_id = Uuid::new_v4();

        let preview = generator
            .generate(file.path(), file_id, None)
            .await
            .unwrap();

        assert_eq!(preview.file_id, file_id);
        assert_eq!(preview.original_width, 100);
        assert_eq!(preview.original_height, 100);
        assert!(!preview.data.is_empty());
        assert_eq!(preview.content_type, "image/jpeg");
        assert!(preview.region_marker.is_none());
    }

    #[tokio::test]
    async fn test_generate_with_region_marker() {
        let file = create_test_image(200, 200);
        let config = PreviewConfig::default();
        let generator = ImagePreviewGenerator::new(config);
        let file_id = Uuid::new_v4();

        let region = Some((0.25, 0.25, 0.5, 0.5));
        let preview = generator
            .generate(file.path(), file_id, region)
            .await
            .unwrap();

        assert!(preview.region_marker.is_some());
        let marker = preview.region_marker.unwrap();
        assert_eq!(marker.x, 0.25);
        assert_eq!(marker.y, 0.25);
        assert_eq!(marker.width, 0.5);
        assert_eq!(marker.height, 0.5);
    }

    #[tokio::test]
    async fn test_scale_large_image() {
        let file = create_test_image(2000, 1500);
        let config = PreviewConfig {
            max_image_width: 800,
            max_image_height: 600,
            ..Default::default()
        };
        let generator = ImagePreviewGenerator::new(config);
        let file_id = Uuid::new_v4();

        let preview = generator
            .generate(file.path(), file_id, None)
            .await
            .unwrap();

        assert!(preview.preview_width <= 800);
        assert!(preview.preview_height <= 600);
        assert_eq!(preview.original_width, 2000);
        assert_eq!(preview.original_height, 1500);
    }

    #[test]
    fn test_scale_image_preserves_aspect_ratio() {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(1000, 500, |_, _| {
            Rgb([128, 128, 128])
        });
        let dynamic = DynamicImage::ImageRgb8(img);

        let scaled = ImagePreviewGenerator::scale_image(&dynamic, 400, 300);
        let (w, h) = scaled.dimensions();

        // Should scale to fit width (400) while preserving aspect ratio
        assert_eq!(w, 400);
        assert_eq!(h, 200); // 500 * (400/1000) = 200
    }
}
