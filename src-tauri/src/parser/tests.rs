//! Tests for content parser module

use super::*;
use tempfile::TempDir;
use std::fs;

/// Helper to create a temp file with content
async fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).expect("Failed to write temp file");
    path
}

mod text_parser_tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_plain_text() {
        let dir = TempDir::new().unwrap();
        let content = "Hello, world!\nThis is a test file.\nWith multiple lines.";
        let path = create_temp_file(&dir, "test.txt", content).await;

        let parser = TextParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert_eq!(result.text, content);
        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.word_count, 10);
    }

    #[tokio::test]
    async fn test_parse_markdown() {
        let dir = TempDir::new().unwrap();
        let content = r#"# Title

This is a paragraph.

## Section 1

Some content here.

```rust
fn main() {
    println!("Hello");
}
```

## Section 2

More content.
"#;
        let path = create_temp_file(&dir, "test.md", content).await;

        let parser = TextParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert!(result.text.contains("# Title"));
        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.title, Some("Title".to_string()));
    }

    #[tokio::test]
    async fn test_parse_json() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"name": "test", "value": 42, "nested": {"key": "value"}}"#;
        let path = create_temp_file(&dir, "test.json", content).await;

        let parser = TextParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert_eq!(result.text, content);
        assert!(!result.chunks.is_empty());
    }

    #[tokio::test]
    async fn test_parse_invalid_json() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"name": "test", invalid}"#;
        let path = create_temp_file(&dir, "test.json", content).await;

        let parser = TextParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await;

        assert!(result.is_err());
        if let Err(ParseError::ParseFailed { reason }) = result {
            assert!(reason.contains("Invalid JSON"));
        } else {
            panic!("Expected ParseFailed error");
        }
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let parser = TextParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(Path::new("/nonexistent/file.txt"), &config).await;

        assert!(matches!(result, Err(ParseError::FileNotFound { .. })));
    }

    #[tokio::test]
    async fn test_large_text_chunking() {
        let dir = TempDir::new().unwrap();
        // Create a large text file
        let paragraph = "This is a test paragraph with some content. ";
        let content = paragraph.repeat(100);
        let path = create_temp_file(&dir, "large.txt", &content).await;

        let parser = TextParser::new();
        let config = ParseConfig {
            max_chunk_size: 500,
            min_chunk_size: 100,
            chunk_overlap: 50,
            ..Default::default()
        };
        let result = parser.parse(&path, &config).await.unwrap();

        // Should have multiple chunks
        assert!(result.chunks.len() > 1);
        
        // Each chunk should be within size limits
        for chunk in &result.chunks {
            assert!(chunk.content.len() <= config.max_chunk_size + 100); // Allow some tolerance
        }
    }

    #[tokio::test]
    async fn test_supported_extensions() {
        let parser = TextParser::new();
        
        assert!(parser.supported_extensions().contains(&"txt"));
        assert!(parser.supported_extensions().contains(&"md"));
        assert!(parser.supported_extensions().contains(&"json"));
        assert!(parser.supported_extensions().contains(&"yaml"));
    }
}

mod code_parser_tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_rust_code() {
        let dir = TempDir::new().unwrap();
        let content = r#"
/// A simple function
pub fn hello_world() {
    println!("Hello, world!");
}

/// A struct
pub struct MyStruct {
    field: i32,
}

impl MyStruct {
    pub fn new(value: i32) -> Self {
        Self { field: value }
    }
}
"#;
        let path = create_temp_file(&dir, "test.rs", content).await;

        let parser = CodeParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.language, Some("Rust".to_string()));
        
        // Should detect function and struct
        let has_code_block = result.chunks.iter().any(|c| c.chunk_type == ChunkType::CodeBlock);
        assert!(has_code_block);
    }

    #[tokio::test]
    async fn test_parse_python_code() {
        let dir = TempDir::new().unwrap();
        let content = r#"
def hello_world():
    """A simple function"""
    print("Hello, world!")

class MyClass:
    """A simple class"""
    
    def __init__(self, value):
        self.value = value
    
    def get_value(self):
        return self.value
"#;
        let path = create_temp_file(&dir, "test.py", content).await;

        let parser = CodeParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.language, Some("Python".to_string()));
    }

    #[tokio::test]
    async fn test_parse_javascript_code() {
        let dir = TempDir::new().unwrap();
        let content = r#"
function helloWorld() {
    console.log("Hello, world!");
}

class MyClass {
    constructor(value) {
        this.value = value;
    }
    
    getValue() {
        return this.value;
    }
}

export async function fetchData(url) {
    const response = await fetch(url);
    return response.json();
}
"#;
        let path = create_temp_file(&dir, "test.js", content).await;

        let parser = CodeParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.language, Some("JavaScript".to_string()));
    }

    #[tokio::test]
    async fn test_parse_typescript_code() {
        let dir = TempDir::new().unwrap();
        let content = r#"
interface User {
    name: string;
    age: number;
}

function greet(user: User): string {
    return `Hello, ${user.name}!`;
}

class UserService {
    private users: User[] = [];
    
    addUser(user: User): void {
        this.users.push(user);
    }
}
"#;
        let path = create_temp_file(&dir, "test.ts", content).await;

        let parser = CodeParser::new();
        let config = ParseConfig::default();
        let result = parser.parse(&path, &config).await.unwrap();

        assert!(!result.chunks.is_empty());
        assert_eq!(result.metadata.language, Some("TypeScript".to_string()));
    }

    #[tokio::test]
    async fn test_language_detection() {
        let parser = CodeParser::new();
        
        assert_eq!(parser.detect_language("rs"), "Rust");
        assert_eq!(parser.detect_language("py"), "Python");
        assert_eq!(parser.detect_language("js"), "JavaScript");
        assert_eq!(parser.detect_language("ts"), "TypeScript");
        assert_eq!(parser.detect_language("java"), "Java");
        assert_eq!(parser.detect_language("go"), "Go");
        assert_eq!(parser.detect_language("cpp"), "C++");
    }

    #[tokio::test]
    async fn test_supported_extensions() {
        let parser = CodeParser::new();
        
        assert!(parser.supported_extensions().contains(&"rs"));
        assert!(parser.supported_extensions().contains(&"py"));
        assert!(parser.supported_extensions().contains(&"js"));
        assert!(parser.supported_extensions().contains(&"ts"));
        assert!(parser.supported_extensions().contains(&"java"));
    }
}

mod content_parser_service_tests {
    use super::*;

    #[tokio::test]
    async fn test_service_routes_to_correct_parser() {
        let dir = TempDir::new().unwrap();
        let service = ContentParserService::new();

        // Test text file
        let txt_content = "Hello, world!";
        let txt_path = create_temp_file(&dir, "test.txt", txt_content).await;
        let result = service.parse(&txt_path).await.unwrap();
        assert_eq!(result.text, txt_content);

        // Test code file
        let rs_content = "fn main() { println!(\"Hello\"); }";
        let rs_path = create_temp_file(&dir, "test.rs", rs_content).await;
        let result = service.parse(&rs_path).await.unwrap();
        assert_eq!(result.metadata.language, Some("Rust".to_string()));
    }

    #[tokio::test]
    async fn test_is_supported() {
        let service = ContentParserService::new();

        assert!(service.is_supported(Path::new("test.txt")));
        assert!(service.is_supported(Path::new("test.md")));
        assert!(service.is_supported(Path::new("test.json")));
        assert!(service.is_supported(Path::new("test.rs")));
        assert!(service.is_supported(Path::new("test.py")));
        assert!(service.is_supported(Path::new("test.pdf")));
        
        // Unsupported
        assert!(!service.is_supported(Path::new("test.xyz")));
        assert!(!service.is_supported(Path::new("test.bin")));
    }

    #[tokio::test]
    async fn test_unsupported_file_type() {
        let dir = TempDir::new().unwrap();
        let service = ContentParserService::new();

        let path = create_temp_file(&dir, "test.xyz", "some content").await;
        let result = service.parse(&path).await;

        assert!(matches!(result, Err(ParseError::UnsupportedFileType { .. })));
    }

    #[tokio::test]
    async fn test_custom_config() {
        let dir = TempDir::new().unwrap();
        let service = ContentParserService::new();

        let content = "A ".repeat(1000);
        let path = create_temp_file(&dir, "test.txt", &content).await;

        let config = ParseConfig {
            max_chunk_size: 100,
            min_chunk_size: 50,
            chunk_overlap: 10,
            ..Default::default()
        };

        let result = service.parse_with_config(&path, &config).await.unwrap();
        
        // Should have many small chunks
        assert!(result.chunks.len() > 10);
    }
}

mod chunk_creation_tests {
    use super::*;

    #[test]
    fn test_create_chunks_small_text() {
        let file_id = Uuid::now_v7();
        let text = "Small text content.";
        let config = ParseConfig::default();

        let chunks = create_chunks_from_text(file_id, text, &config, ChunkType::Paragraph);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, text);
        assert_eq!(chunks[0].chunk_index, 0);
    }

    #[test]
    fn test_create_chunks_empty_text() {
        let file_id = Uuid::now_v7();
        let text = "";
        let config = ParseConfig::default();

        let chunks = create_chunks_from_text(file_id, text, &config, ChunkType::Paragraph);

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_create_chunks_large_text() {
        let file_id = Uuid::now_v7();
        let text = "This is a sentence. ".repeat(100);
        let config = ParseConfig {
            max_chunk_size: 200,
            min_chunk_size: 50,
            chunk_overlap: 20,
            ..Default::default()
        };

        let chunks = create_chunks_from_text(file_id, &text, &config, ChunkType::Paragraph);

        assert!(chunks.len() > 1);
        
        // Verify chunk indices are sequential
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as u32);
        }
    }

    #[test]
    fn test_chunk_location_tracking() {
        let file_id = Uuid::now_v7();
        let text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let config = ParseConfig::default();

        let chunks = create_chunks_from_text(file_id, text, &config, ChunkType::Paragraph);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].location.start_offset, 0);
        assert_eq!(chunks[0].location.end_offset, text.len() as u64);
        assert_eq!(chunks[0].location.start_line, Some(1));
        assert_eq!(chunks[0].location.end_line, Some(5));
    }
}

mod parse_error_tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ParseError::FileNotFound {
            path: "/test/file.txt".to_string(),
        };
        assert!(err.to_string().contains("/test/file.txt"));

        let err = ParseError::UnsupportedFileType {
            extension: "xyz".to_string(),
        };
        assert!(err.to_string().contains("xyz"));

        let err = ParseError::ParseFailed {
            reason: "Invalid format".to_string(),
        };
        assert!(err.to_string().contains("Invalid format"));
    }
}
