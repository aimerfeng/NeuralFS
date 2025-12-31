//! Tests for the preview module

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_preview_service_text_file() {
    let mut file = NamedTempFile::with_suffix(".txt").unwrap();
    writeln!(file, "Hello World").unwrap();
    writeln!(file, "This is a test file").unwrap();
    writeln!(file, "With multiple lines").unwrap();

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let preview = service
        .generate_preview(file.path(), file_id)
        .await
        .unwrap();

    match preview {
        GeneratedPreview::Text(text_preview) => {
            assert_eq!(text_preview.file_id, file_id);
            assert!(text_preview.snippet.contains("Hello World"));
            assert_eq!(text_preview.total_lines, 3);
        }
        _ => panic!("Expected text preview"),
    }
}

#[tokio::test]
async fn test_preview_service_markdown_file() {
    let mut file = NamedTempFile::with_suffix(".md").unwrap();
    writeln!(file, "# Heading").unwrap();
    writeln!(file, "").unwrap();
    writeln!(file, "Some paragraph text.").unwrap();

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let preview = service
        .generate_preview(file.path(), file_id)
        .await
        .unwrap();

    match preview {
        GeneratedPreview::Text(text_preview) => {
            assert!(text_preview.snippet.contains("# Heading"));
        }
        _ => panic!("Expected text preview"),
    }
}

#[tokio::test]
async fn test_preview_service_code_file() {
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    writeln!(file, "fn main() {{").unwrap();
    writeln!(file, "    println!(\"Hello\");").unwrap();
    writeln!(file, "}}").unwrap();

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let preview = service
        .generate_preview(file.path(), file_id)
        .await
        .unwrap();

    match preview {
        GeneratedPreview::Text(text_preview) => {
            assert!(text_preview.snippet.contains("fn main()"));
        }
        _ => panic!("Expected text preview"),
    }
}

#[tokio::test]
async fn test_preview_service_image_file() {
    use image::{ImageBuffer, Rgb};

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(100, 100, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });

    let file = NamedTempFile::with_suffix(".png").unwrap();
    img.save(file.path()).unwrap();

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let preview = service
        .generate_preview(file.path(), file_id)
        .await
        .unwrap();

    match preview {
        GeneratedPreview::Image(img_preview) => {
            assert_eq!(img_preview.file_id, file_id);
            assert_eq!(img_preview.original_width, 100);
            assert_eq!(img_preview.original_height, 100);
            assert!(!img_preview.data.is_empty());
        }
        _ => panic!("Expected image preview"),
    }
}

#[tokio::test]
async fn test_preview_service_unsupported_file() {
    let file = NamedTempFile::with_suffix(".xyz").unwrap();

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let result = service.generate_preview(file.path(), file_id).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        PreviewError::UnsupportedFileType { extension } => {
            assert_eq!(extension, "xyz");
        }
        _ => panic!("Expected UnsupportedFileType error"),
    }
}

#[tokio::test]
async fn test_preview_service_file_not_found() {
    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let result = service
        .generate_preview(std::path::Path::new("/nonexistent/file.txt"), file_id)
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        PreviewError::FileNotFound { .. } => {}
        _ => panic!("Expected FileNotFound error"),
    }
}

#[tokio::test]
async fn test_preview_with_highlight() {
    let mut file = NamedTempFile::with_suffix(".txt").unwrap();
    for i in 1..=50 {
        writeln!(file, "Line {}: This is content for testing search functionality", i).unwrap();
    }

    let service = PreviewService::new();
    let file_id = uuid::Uuid::new_v4();

    let location = crate::core::types::chunk::ChunkLocation {
        start_offset: 0,
        end_offset: 100,
        start_line: Some(25),
        end_line: Some(27),
        page_number: None,
        bounding_box: None,
    };

    let preview = service
        .generate_preview_with_highlight(file.path(), file_id, &location, Some("content"))
        .await
        .unwrap();

    match preview {
        GeneratedPreview::Text(text_preview) => {
            // Should have context around line 25
            assert!(text_preview.start_line <= 25);
            assert!(text_preview.end_line >= 27);
            // Should have highlights for "content"
            assert!(!text_preview.highlights.is_empty());
        }
        _ => panic!("Expected text preview"),
    }
}

#[tokio::test]
async fn test_generated_preview_to_bytes() {
    let text_preview = TextPreview {
        file_id: uuid::Uuid::new_v4(),
        snippet: "Test content".to_string(),
        highlights: vec![],
        total_lines: 1,
        total_chars: 12,
        start_line: 1,
        end_line: 1,
        has_more: false,
    };

    let preview = GeneratedPreview::Text(text_preview);
    let bytes = preview.to_bytes().unwrap();

    assert!(!bytes.is_empty());
    // Should be valid JSON
    let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
}

#[test]
fn test_preview_config_default() {
    let config = PreviewConfig::default();

    assert_eq!(config.max_snippet_length, 500);
    assert_eq!(config.context_lines, 3);
    assert_eq!(config.max_image_width, 800);
    assert_eq!(config.max_image_height, 600);
    assert_eq!(config.jpeg_quality, 85);
    assert_eq!(config.max_pdf_pages, 5);
}

#[test]
fn test_generated_preview_content_type() {
    let text_preview = GeneratedPreview::Text(TextPreview {
        file_id: uuid::Uuid::new_v4(),
        snippet: "".to_string(),
        highlights: vec![],
        total_lines: 0,
        total_chars: 0,
        start_line: 1,
        end_line: 1,
        has_more: false,
    });
    assert_eq!(text_preview.content_type(), "application/json");

    let img_preview = GeneratedPreview::Image(ImagePreview {
        file_id: uuid::Uuid::new_v4(),
        data: vec![],
        content_type: "image/jpeg".to_string(),
        original_width: 100,
        original_height: 100,
        preview_width: 100,
        preview_height: 100,
        region_marker: None,
    });
    assert_eq!(img_preview.content_type(), "image/jpeg");

    let doc_preview = GeneratedPreview::Document(DocumentPreview {
        file_id: uuid::Uuid::new_v4(),
        document_type: DocumentType::Pdf,
        total_pages: 1,
        pages: vec![],
        target_page: None,
        word_count: 0,
        has_more_pages: false,
    });
    assert_eq!(doc_preview.content_type(), "application/json");
}
