//! Code file parser
//!
//! Handles parsing of source code files with syntax-aware chunking.
//! Extracts functions, classes, and other code structures.

use super::{ContentParser, ParseConfig, ParseError, ParseMetadata, ParseResult};
use crate::core::types::chunk::{ChunkLocation, ChunkType, ContentChunk};
use crate::core::types::file::FileType;
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

/// Parser for source code files
pub struct CodeParser {
    supported_extensions: Vec<&'static str>,
}

impl CodeParser {
    /// Create a new code parser
    pub fn new() -> Self {
        Self {
            supported_extensions: vec![
                "rs", "py", "js", "ts", "jsx", "tsx", "java", "c", "cpp", "h", "hpp", "cs", "go",
                "rb", "php", "swift", "kt", "scala", "html", "css", "scss", "sql", "sh", "bash",
                "zsh", "ps1", "bat", "cmd",
            ],
        }
    }

    /// Parse code content with structure awareness
    fn parse_code(
        &self,
        content: &str,
        file_id: Uuid,
        extension: &str,
        config: &ParseConfig,
    ) -> ParseResult {
        let language = self.detect_language(extension);
        let structures = self.extract_code_structures(content, &language);

        let mut chunks = Vec::new();
        let mut chunk_index = 0u32;

        // Create chunks from extracted structures
        for structure in &structures {
            let chunk_type = match structure.kind {
                CodeStructureKind::Function | CodeStructureKind::Method => ChunkType::CodeBlock,
                CodeStructureKind::Class | CodeStructureKind::Struct => ChunkType::CodeBlock,
                CodeStructureKind::Import => ChunkType::Paragraph,
                CodeStructureKind::Comment => ChunkType::Paragraph,
                CodeStructureKind::Other => ChunkType::CodeBlock,
            };

            // If structure is too large, split it
            if structure.content.len() > config.max_chunk_size {
                let sub_chunks = self.split_large_structure(
                    file_id,
                    structure,
                    chunk_index,
                    config,
                );
                chunk_index += sub_chunks.len() as u32;
                chunks.extend(sub_chunks);
            } else {
                chunks.push(ContentChunk {
                    id: Uuid::now_v7(),
                    file_id,
                    chunk_index,
                    chunk_type,
                    content: structure.content.clone(),
                    location: ChunkLocation {
                        start_offset: structure.start_offset as u64,
                        end_offset: structure.end_offset as u64,
                        start_line: Some(structure.start_line),
                        end_line: Some(structure.end_line),
                        page_number: None,
                        bounding_box: None,
                    },
                    vector_id: 0,
                    created_at: Utc::now(),
                });
                chunk_index += 1;
            }
        }

        // If no structures found, fall back to line-based chunking
        if chunks.is_empty() {
            chunks = self.fallback_line_chunking(file_id, content, config);
        }

        let metadata = ParseMetadata {
            language: Some(language),
            word_count: content.split_whitespace().count(),
            char_count: content.chars().count(),
            ..Default::default()
        };

        ParseResult {
            text: content.to_string(),
            chunks,
            metadata,
        }
    }

    /// Detect programming language from extension
    fn detect_language(&self, extension: &str) -> String {
        match extension.to_lowercase().as_str() {
            "rs" => "Rust".to_string(),
            "py" => "Python".to_string(),
            "js" => "JavaScript".to_string(),
            "ts" => "TypeScript".to_string(),
            "jsx" => "JavaScript (JSX)".to_string(),
            "tsx" => "TypeScript (TSX)".to_string(),
            "java" => "Java".to_string(),
            "c" => "C".to_string(),
            "cpp" | "cc" | "cxx" => "C++".to_string(),
            "h" | "hpp" => "C/C++ Header".to_string(),
            "cs" => "C#".to_string(),
            "go" => "Go".to_string(),
            "rb" => "Ruby".to_string(),
            "php" => "PHP".to_string(),
            "swift" => "Swift".to_string(),
            "kt" => "Kotlin".to_string(),
            "scala" => "Scala".to_string(),
            "html" => "HTML".to_string(),
            "css" => "CSS".to_string(),
            "scss" => "SCSS".to_string(),
            "sql" => "SQL".to_string(),
            "sh" | "bash" | "zsh" => "Shell".to_string(),
            "ps1" => "PowerShell".to_string(),
            "bat" | "cmd" => "Batch".to_string(),
            _ => "Unknown".to_string(),
        }
    }

    /// Extract code structures (functions, classes, etc.)
    fn extract_code_structures(&self, content: &str, language: &str) -> Vec<CodeStructure> {
        let mut structures = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Language-specific patterns
        let patterns = self.get_language_patterns(language);

        let mut current_structure: Option<CodeStructureBuilder> = None;
        let mut brace_depth = 0i32;
        let mut current_offset = 0usize;

        for (line_num, line) in lines.iter().enumerate() {
            let line_start = current_offset;
            let line_end = current_offset + line.len();

            // Check for structure start
            if current_structure.is_none() {
                for pattern in &patterns {
                    if self.matches_pattern(line, pattern) {
                        current_structure = Some(CodeStructureBuilder {
                            kind: pattern.kind.clone(),
                            start_line: (line_num + 1) as u32,
                            start_offset: line_start,
                            content: String::new(),
                        });
                        break;
                    }
                }
            }

            // Track brace depth for block-based languages
            if let Some(ref mut builder) = current_structure {
                builder.content.push_str(line);
                builder.content.push('\n');

                brace_depth += line.matches('{').count() as i32;
                brace_depth -= line.matches('}').count() as i32;

                // Check for structure end
                let is_end = match language {
                    "Python" => {
                        // Python uses indentation - check for dedent
                        line_num + 1 < lines.len()
                            && !lines[line_num + 1].is_empty()
                            && !lines[line_num + 1].starts_with(' ')
                            && !lines[line_num + 1].starts_with('\t')
                            && builder.content.lines().count() > 1
                    }
                    _ => brace_depth == 0 && builder.content.contains('{'),
                };

                if is_end || line_num == lines.len() - 1 {
                    structures.push(CodeStructure {
                        kind: builder.kind.clone(),
                        content: builder.content.trim_end().to_string(),
                        start_line: builder.start_line,
                        end_line: (line_num + 1) as u32,
                        start_offset: builder.start_offset,
                        end_offset: line_end,
                        name: self.extract_name(&builder.content, &builder.kind),
                    });
                    current_structure = None;
                    brace_depth = 0;
                }
            }

            current_offset = line_end + 1; // +1 for newline
        }

        // Handle any remaining structure
        if let Some(builder) = current_structure {
            structures.push(CodeStructure {
                kind: builder.kind,
                content: builder.content.trim_end().to_string(),
                start_line: builder.start_line,
                end_line: lines.len() as u32,
                start_offset: builder.start_offset,
                end_offset: content.len(),
                name: None,
            });
        }

        // If no structures found, create one for the whole file
        if structures.is_empty() && !content.trim().is_empty() {
            structures.push(CodeStructure {
                kind: CodeStructureKind::Other,
                content: content.to_string(),
                start_line: 1,
                end_line: lines.len() as u32,
                start_offset: 0,
                end_offset: content.len(),
                name: None,
            });
        }

        structures
    }

    /// Get language-specific patterns for structure detection
    fn get_language_patterns(&self, language: &str) -> Vec<StructurePattern> {
        match language {
            "Rust" => vec![
                StructurePattern {
                    pattern: "fn ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "pub fn ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "async fn ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "pub async fn ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "struct ",
                    kind: CodeStructureKind::Struct,
                },
                StructurePattern {
                    pattern: "pub struct ",
                    kind: CodeStructureKind::Struct,
                },
                StructurePattern {
                    pattern: "enum ",
                    kind: CodeStructureKind::Struct,
                },
                StructurePattern {
                    pattern: "pub enum ",
                    kind: CodeStructureKind::Struct,
                },
                StructurePattern {
                    pattern: "impl ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "trait ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "pub trait ",
                    kind: CodeStructureKind::Class,
                },
            ],
            "Python" => vec![
                StructurePattern {
                    pattern: "def ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "async def ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "class ",
                    kind: CodeStructureKind::Class,
                },
            ],
            "JavaScript" | "TypeScript" | "JavaScript (JSX)" | "TypeScript (TSX)" => vec![
                StructurePattern {
                    pattern: "function ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "async function ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "class ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "const ",
                    kind: CodeStructureKind::Other,
                },
                StructurePattern {
                    pattern: "export function ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "export async function ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "export class ",
                    kind: CodeStructureKind::Class,
                },
            ],
            "Java" | "C#" | "Kotlin" => vec![
                StructurePattern {
                    pattern: "public class ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "class ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "public void ",
                    kind: CodeStructureKind::Method,
                },
                StructurePattern {
                    pattern: "private void ",
                    kind: CodeStructureKind::Method,
                },
                StructurePattern {
                    pattern: "public static ",
                    kind: CodeStructureKind::Method,
                },
                StructurePattern {
                    pattern: "interface ",
                    kind: CodeStructureKind::Class,
                },
            ],
            "Go" => vec![
                StructurePattern {
                    pattern: "func ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "type ",
                    kind: CodeStructureKind::Struct,
                },
            ],
            "C" | "C++" | "C/C++ Header" => vec![
                StructurePattern {
                    pattern: "void ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "int ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "class ",
                    kind: CodeStructureKind::Class,
                },
                StructurePattern {
                    pattern: "struct ",
                    kind: CodeStructureKind::Struct,
                },
            ],
            _ => vec![
                StructurePattern {
                    pattern: "function ",
                    kind: CodeStructureKind::Function,
                },
                StructurePattern {
                    pattern: "class ",
                    kind: CodeStructureKind::Class,
                },
            ],
        }
    }

    /// Check if a line matches a pattern
    fn matches_pattern(&self, line: &str, pattern: &StructurePattern) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with(pattern.pattern)
    }

    /// Extract name from code structure
    fn extract_name(&self, content: &str, kind: &CodeStructureKind) -> Option<String> {
        let first_line = content.lines().next()?;
        let trimmed = first_line.trim();

        // Try to extract name based on structure kind
        match kind {
            CodeStructureKind::Function | CodeStructureKind::Method => {
                // Look for function name pattern
                if let Some(start) = trimmed.find("fn ") {
                    let after_fn = &trimmed[start + 3..];
                    let name_end = after_fn.find(|c: char| c == '(' || c == '<' || c.is_whitespace())?;
                    return Some(after_fn[..name_end].to_string());
                }
                if let Some(start) = trimmed.find("def ") {
                    let after_def = &trimmed[start + 4..];
                    let name_end = after_def.find('(')?;
                    return Some(after_def[..name_end].to_string());
                }
                if let Some(start) = trimmed.find("function ") {
                    let after_func = &trimmed[start + 9..];
                    let name_end = after_func.find('(')?;
                    return Some(after_func[..name_end].trim().to_string());
                }
                if let Some(start) = trimmed.find("func ") {
                    let after_func = &trimmed[start + 5..];
                    let name_end = after_func.find('(')?;
                    return Some(after_func[..name_end].trim().to_string());
                }
            }
            CodeStructureKind::Class | CodeStructureKind::Struct => {
                // Look for class/struct name
                for keyword in &["class ", "struct ", "enum ", "trait ", "interface ", "impl "] {
                    if let Some(start) = trimmed.find(keyword) {
                        let after_keyword = &trimmed[start + keyword.len()..];
                        let name_end = after_keyword
                            .find(|c: char| c == '{' || c == '(' || c == '<' || c == ':' || c.is_whitespace())
                            .unwrap_or(after_keyword.len());
                        let name = after_keyword[..name_end].trim();
                        if !name.is_empty() {
                            return Some(name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }

        None
    }

    /// Split a large code structure into smaller chunks
    fn split_large_structure(
        &self,
        file_id: Uuid,
        structure: &CodeStructure,
        start_index: u32,
        config: &ParseConfig,
    ) -> Vec<ContentChunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = structure.content.lines().collect();
        let mut current_chunk = String::new();
        let mut chunk_start_line = structure.start_line;
        let mut chunk_index = start_index;
        let mut current_offset = structure.start_offset;

        for (i, line) in lines.iter().enumerate() {
            current_chunk.push_str(line);
            current_chunk.push('\n');

            // Check if we should create a new chunk
            if current_chunk.len() >= config.max_chunk_size || i == lines.len() - 1 {
                let chunk_end_line = structure.start_line + i as u32;
                let chunk_end_offset = current_offset + current_chunk.len();

                chunks.push(ContentChunk {
                    id: Uuid::now_v7(),
                    file_id,
                    chunk_index,
                    chunk_type: ChunkType::CodeBlock,
                    content: current_chunk.trim_end().to_string(),
                    location: ChunkLocation {
                        start_offset: current_offset as u64,
                        end_offset: chunk_end_offset as u64,
                        start_line: Some(chunk_start_line),
                        end_line: Some(chunk_end_line),
                        page_number: None,
                        bounding_box: None,
                    },
                    vector_id: 0,
                    created_at: Utc::now(),
                });

                current_offset = chunk_end_offset;
                chunk_start_line = chunk_end_line + 1;
                chunk_index += 1;
                current_chunk = String::new();
            }
        }

        chunks
    }

    /// Fallback to simple line-based chunking
    fn fallback_line_chunking(
        &self,
        file_id: Uuid,
        content: &str,
        config: &ParseConfig,
    ) -> Vec<ContentChunk> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut current_chunk = String::new();
        let mut chunk_start_line = 1u32;
        let mut chunk_index = 0u32;
        let mut current_offset = 0usize;

        for (i, line) in lines.iter().enumerate() {
            current_chunk.push_str(line);
            current_chunk.push('\n');

            if current_chunk.len() >= config.max_chunk_size || i == lines.len() - 1 {
                let chunk_end_line = (i + 1) as u32;
                let chunk_end_offset = current_offset + current_chunk.len();

                if !current_chunk.trim().is_empty() {
                    chunks.push(ContentChunk {
                        id: Uuid::now_v7(),
                        file_id,
                        chunk_index,
                        chunk_type: ChunkType::CodeBlock,
                        content: current_chunk.trim_end().to_string(),
                        location: ChunkLocation {
                            start_offset: current_offset as u64,
                            end_offset: chunk_end_offset as u64,
                            start_line: Some(chunk_start_line),
                            end_line: Some(chunk_end_line),
                            page_number: None,
                            bounding_box: None,
                        },
                        vector_id: 0,
                        created_at: Utc::now(),
                    });
                    chunk_index += 1;
                }

                current_offset = chunk_end_offset;
                chunk_start_line = chunk_end_line + 1;
                current_chunk = String::new();
            }
        }

        chunks
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentParser for CodeParser {
    async fn parse(&self, path: &Path, config: &ParseConfig) -> Result<ParseResult, ParseError> {
        // Check if file exists
        if !path.exists() {
            return Err(ParseError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        // Read file content
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                ParseError::EncodingError {
                    reason: "File is not valid UTF-8".to_string(),
                }
            } else {
                ParseError::Io(e)
            }
        })?;

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_id = Uuid::now_v7();
        Ok(self.parse_code(&content, file_id, &extension, config))
    }

    fn supports(&self, file_type: FileType) -> bool {
        matches!(file_type, FileType::Code)
    }

    fn supported_extensions(&self) -> &[&str] {
        &self.supported_extensions
    }
}

/// Kind of code structure
#[derive(Debug, Clone, PartialEq)]
enum CodeStructureKind {
    Function,
    Method,
    Class,
    Struct,
    Import,
    Comment,
    Other,
}

/// Extracted code structure
#[derive(Debug)]
struct CodeStructure {
    kind: CodeStructureKind,
    content: String,
    start_line: u32,
    end_line: u32,
    start_offset: usize,
    end_offset: usize,
    #[allow(dead_code)]
    name: Option<String>,
}

/// Builder for code structure during parsing
struct CodeStructureBuilder {
    kind: CodeStructureKind,
    start_line: u32,
    start_offset: usize,
    content: String,
}

/// Pattern for detecting code structures
struct StructurePattern {
    pattern: &'static str,
    kind: CodeStructureKind,
}
