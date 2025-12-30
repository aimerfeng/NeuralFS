//! Diluted Attention Processor for Long Documents
//!
//! This module implements the diluted attention mechanism for processing
//! documents that exceed the maximum sequence length of the embedding model.
//! 
//! Key features:
//! - Sliding window processing with configurable overlap
//! - Global context sampling via dilution factor
//! - Memory-efficient processing for large files
//!
//! **Validates: Requirements 4.2** - WHEN processing large files, THE Embedding_Engine
//! SHALL use Diluted_Attention mechanism to reduce memory footprint

use chrono::Utc;
use uuid::Uuid;

use crate::core::types::chunk::{ChunkLocation, ChunkType, ContentChunk};

/// Configuration for the diluted attention processor
#[derive(Debug, Clone)]
pub struct DilutedAttentionConfig {
    /// Size of the sliding window in tokens
    pub window_size: usize,
    
    /// Dilution factor for global context sampling
    /// Every nth token is sampled for global context
    pub dilution_factor: usize,
    
    /// Maximum sequence length the model can handle
    pub max_seq_length: usize,
    
    /// Overlap ratio between consecutive windows (0.0 to 1.0)
    /// 0.5 means 50% overlap
    pub overlap_ratio: f32,
    
    /// Maximum tokens for global context
    pub max_global_context: usize,
}

impl Default for DilutedAttentionConfig {
    fn default() -> Self {
        Self {
            window_size: 256,
            dilution_factor: 8,
            max_seq_length: 512,
            overlap_ratio: 0.5,
            max_global_context: 128,
        }
    }
}

/// Token representation for processing
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Start of sequence token
    CLS,
    /// Separator token
    SEP,
    /// Regular word token
    Word(String),
    /// Padding token
    PAD,
}

impl Token {
    /// Get the string representation of the token
    pub fn as_str(&self) -> &str {
        match self {
            Token::CLS => "[CLS]",
            Token::SEP => "[SEP]",
            Token::Word(s) => s,
            Token::PAD => "[PAD]",
        }
    }
}

/// Result of processing a document window
#[derive(Debug, Clone)]
pub struct ProcessedWindow {
    /// The tokens in this window (local context)
    pub local_tokens: Vec<Token>,
    
    /// The global context tokens (diluted sampling)
    pub global_tokens: Vec<Token>,
    
    /// Combined tokens ready for embedding
    pub combined_tokens: Vec<Token>,
    
    /// Start position in original document (character offset)
    pub start_offset: usize,
    
    /// End position in original document (character offset)
    pub end_offset: usize,
    
    /// Window index
    pub window_index: usize,
}

/// Diluted attention processor for handling long documents
/// 
/// Uses sliding window with global context sampling to process
/// documents that exceed the model's maximum sequence length.
pub struct DilutedAttentionProcessor {
    config: DilutedAttentionConfig,
}

impl DilutedAttentionProcessor {
    /// Create a new processor with the given configuration
    pub fn new(config: DilutedAttentionConfig) -> Self {
        Self { config }
    }
    
    /// Create a processor with default configuration
    pub fn with_defaults() -> Self {
        Self::new(DilutedAttentionConfig::default())
    }
    
    /// Get the configuration
    pub fn config(&self) -> &DilutedAttentionConfig {
        &self.config
    }
    
    /// Tokenize content into tokens
    /// 
    /// This is a simple whitespace-based tokenizer.
    /// In production, this should use a proper BPE/WordPiece tokenizer.
    pub fn tokenize(&self, content: &str) -> Vec<Token> {
        content
            .split_whitespace()
            .map(|s| Token::Word(s.to_string()))
            .collect()
    }
    
    /// Detokenize tokens back to string
    pub fn detokenize(&self, tokens: &[Token]) -> String {
        tokens
            .iter()
            .filter_map(|t| match t {
                Token::Word(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    
    /// Check if a document needs diluted attention processing
    /// 
    /// Returns true if the document exceeds the maximum sequence length
    pub fn needs_diluted_processing(&self, content: &str) -> bool {
        let tokens = self.tokenize(content);
        tokens.len() > self.config.max_seq_length
    }
    
    /// Calculate the step size for sliding window based on overlap ratio
    fn calculate_step_size(&self) -> usize {
        let overlap = (self.config.window_size as f32 * self.config.overlap_ratio) as usize;
        self.config.window_size.saturating_sub(overlap).max(1)
    }
    
    /// Sample global context tokens using dilution factor
    /// 
    /// Samples every nth token from the entire document to provide
    /// global context while keeping memory usage bounded.
    fn sample_global_context(&self, tokens: &[Token]) -> Vec<Token> {
        tokens
            .iter()
            .enumerate()
            .filter(|(i, _)| i % self.config.dilution_factor == 0)
            .take(self.config.max_global_context)
            .map(|(_, t)| t.clone())
            .collect()
    }
    
    /// Combine local window tokens with global context
    /// 
    /// Format: [CLS] + global_context + [SEP] + local_window + [SEP]
    pub fn combine_context(&self, local: &[Token], global: &[Token]) -> Vec<Token> {
        let mut combined = Vec::with_capacity(
            1 + global.len() + 1 + local.len() + 1
        );
        
        // [CLS] token
        combined.push(Token::CLS);
        
        // Global context (limited)
        combined.extend(global.iter().take(self.config.max_global_context).cloned());
        
        // [SEP] separator
        combined.push(Token::SEP);
        
        // Local window
        combined.extend(local.iter().cloned());
        
        // [SEP] end
        combined.push(Token::SEP);
        
        combined
    }
    
    /// Process a long document into windows with global context
    /// 
    /// Returns a vector of processed windows, each containing:
    /// - Local tokens from the sliding window
    /// - Global context tokens from diluted sampling
    /// - Combined tokens ready for embedding
    /// - Position information for chunk creation
    pub fn process_document(&self, content: &str) -> Vec<ProcessedWindow> {
        let tokens = self.tokenize(content);
        
        // If document is short enough, return single window
        if tokens.len() <= self.config.max_seq_length {
            let global = self.sample_global_context(&tokens);
            let combined = self.combine_context(&tokens, &global);
            
            return vec![ProcessedWindow {
                local_tokens: tokens,
                global_tokens: global,
                combined_tokens: combined,
                start_offset: 0,
                end_offset: content.len(),
                window_index: 0,
            }];
        }
        
        // Process with sliding window
        let step_size = self.calculate_step_size();
        let global_context = self.sample_global_context(&tokens);
        
        let mut windows = Vec::new();
        let mut position = 0;
        let mut window_index = 0;
        
        // Track character offsets
        let word_offsets = self.calculate_word_offsets(content);
        
        while position < tokens.len() {
            let window_end = (position + self.config.window_size).min(tokens.len());
            let window_tokens: Vec<Token> = tokens[position..window_end].to_vec();
            
            let combined = self.combine_context(&window_tokens, &global_context);
            
            // Calculate character offsets
            let start_offset = word_offsets.get(position).map(|&(s, _)| s).unwrap_or(0);
            let end_offset = word_offsets
                .get(window_end.saturating_sub(1))
                .map(|&(_, e)| e)
                .unwrap_or(content.len());
            
            windows.push(ProcessedWindow {
                local_tokens: window_tokens,
                global_tokens: global_context.clone(),
                combined_tokens: combined,
                start_offset,
                end_offset,
                window_index,
            });
            
            // Move to next window
            position += step_size;
            window_index += 1;
            
            // Ensure we don't create too many windows
            if window_index > 1000 {
                tracing::warn!("Document too large, truncating at 1000 windows");
                break;
            }
        }
        
        windows
    }
    
    /// Calculate word offsets (start, end) for each token in the content
    fn calculate_word_offsets(&self, content: &str) -> Vec<(usize, usize)> {
        let mut offsets = Vec::new();
        let mut current_pos = 0;
        
        for word in content.split_whitespace() {
            // Find the word in the remaining content
            if let Some(start) = content[current_pos..].find(word) {
                let absolute_start = current_pos + start;
                let absolute_end = absolute_start + word.len();
                offsets.push((absolute_start, absolute_end));
                current_pos = absolute_end;
            }
        }
        
        offsets
    }

    
    /// Convert processed windows to content chunks
    /// 
    /// Creates ContentChunk structures from processed windows,
    /// ready for vector storage.
    pub fn windows_to_chunks(
        &self,
        windows: &[ProcessedWindow],
        file_id: Uuid,
    ) -> Vec<ContentChunk> {
        windows
            .iter()
            .map(|window| {
                ContentChunk {
                    id: Uuid::now_v7(),
                    file_id,
                    chunk_index: window.window_index as u32,
                    chunk_type: ChunkType::Paragraph,
                    content: self.detokenize(&window.local_tokens),
                    location: ChunkLocation {
                        start_offset: window.start_offset as u64,
                        end_offset: window.end_offset as u64,
                        start_line: None,
                        end_line: None,
                        page_number: None,
                        bounding_box: None,
                    },
                    vector_id: 0, // To be assigned by vector store
                    created_at: Utc::now(),
                }
            })
            .collect()
    }
    
    /// Process a document and return content chunks directly
    /// 
    /// Convenience method that combines process_document and windows_to_chunks
    pub fn process_to_chunks(&self, content: &str, file_id: Uuid) -> Vec<ContentChunk> {
        let windows = self.process_document(content);
        self.windows_to_chunks(&windows, file_id)
    }
    
    /// Get the combined token strings for embedding
    /// 
    /// Returns the text representation of combined tokens for each window,
    /// ready to be passed to the embedding engine.
    pub fn get_embedding_texts(&self, windows: &[ProcessedWindow]) -> Vec<String> {
        windows
            .iter()
            .map(|w| {
                w.combined_tokens
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }
    
    /// Calculate coverage statistics for processed windows
    /// 
    /// Returns information about how well the windows cover the original content.
    pub fn calculate_coverage(&self, content: &str, windows: &[ProcessedWindow]) -> CoverageStats {
        if windows.is_empty() {
            return CoverageStats {
                total_chars: content.len(),
                covered_chars: 0,
                coverage_ratio: 0.0,
                window_count: 0,
                overlap_ratio: 0.0,
            };
        }
        
        let total_chars = content.len();
        
        // Calculate covered characters (accounting for overlaps)
        let mut covered = vec![false; total_chars];
        for window in windows {
            for i in window.start_offset..window.end_offset.min(total_chars) {
                covered[i] = true;
            }
        }
        let covered_chars = covered.iter().filter(|&&c| c).count();
        
        // Calculate overlap
        let total_window_chars: usize = windows
            .iter()
            .map(|w| w.end_offset.saturating_sub(w.start_offset))
            .sum();
        
        let overlap_chars = total_window_chars.saturating_sub(covered_chars);
        let overlap_ratio = if total_window_chars > 0 {
            overlap_chars as f32 / total_window_chars as f32
        } else {
            0.0
        };
        
        CoverageStats {
            total_chars,
            covered_chars,
            coverage_ratio: covered_chars as f32 / total_chars.max(1) as f32,
            window_count: windows.len(),
            overlap_ratio,
        }
    }
}

/// Statistics about content coverage
#[derive(Debug, Clone)]
pub struct CoverageStats {
    /// Total characters in the original content
    pub total_chars: usize,
    
    /// Number of characters covered by at least one window
    pub covered_chars: usize,
    
    /// Ratio of covered characters (0.0 to 1.0)
    pub coverage_ratio: f32,
    
    /// Number of windows created
    pub window_count: usize,
    
    /// Ratio of overlapping content between windows
    pub overlap_ratio: f32,
}

impl CoverageStats {
    /// Check if coverage is complete (all characters covered)
    pub fn is_complete(&self) -> bool {
        self.covered_chars >= self.total_chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    
    #[test]
    fn test_tokenize_simple() {
        let processor = DilutedAttentionProcessor::with_defaults();
        let tokens = processor.tokenize("hello world test");
        
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Word("hello".to_string()));
        assert_eq!(tokens[1], Token::Word("world".to_string()));
        assert_eq!(tokens[2], Token::Word("test".to_string()));
    }
    
    #[test]
    fn test_detokenize() {
        let processor = DilutedAttentionProcessor::with_defaults();
        let tokens = vec![
            Token::Word("hello".to_string()),
            Token::Word("world".to_string()),
        ];
        
        let text = processor.detokenize(&tokens);
        assert_eq!(text, "hello world");
    }
    
    #[test]
    fn test_short_document_single_window() {
        let config = DilutedAttentionConfig {
            window_size: 100,
            max_seq_length: 200,
            ..Default::default()
        };
        let processor = DilutedAttentionProcessor::new(config);
        
        let content = "This is a short document with few words";
        let windows = processor.process_document(content);
        
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].window_index, 0);
    }
    
    #[test]
    fn test_long_document_multiple_windows() {
        let config = DilutedAttentionConfig {
            window_size: 5,
            max_seq_length: 10,
            overlap_ratio: 0.5,
            dilution_factor: 2,
            max_global_context: 10,
        };
        let processor = DilutedAttentionProcessor::new(config);
        
        // Create a document with 20 words
        let words: Vec<&str> = (0..20).map(|i| match i % 4 {
            0 => "alpha",
            1 => "beta",
            2 => "gamma",
            _ => "delta",
        }).collect();
        let content = words.join(" ");
        
        let windows = processor.process_document(&content);
        
        // Should have multiple windows
        assert!(windows.len() > 1);
        
        // Each window should have global context
        for window in &windows {
            assert!(!window.global_tokens.is_empty());
        }
    }
    
    #[test]
    fn test_combine_context_format() {
        let processor = DilutedAttentionProcessor::with_defaults();
        
        let local = vec![
            Token::Word("local".to_string()),
            Token::Word("content".to_string()),
        ];
        let global = vec![
            Token::Word("global".to_string()),
        ];
        
        let combined = processor.combine_context(&local, &global);
        
        // Should be: [CLS] global [SEP] local content [SEP]
        assert_eq!(combined[0], Token::CLS);
        assert_eq!(combined[1], Token::Word("global".to_string()));
        assert_eq!(combined[2], Token::SEP);
        assert_eq!(combined[3], Token::Word("local".to_string()));
        assert_eq!(combined[4], Token::Word("content".to_string()));
        assert_eq!(combined[5], Token::SEP);
    }
    
    #[test]
    fn test_coverage_complete() {
        let config = DilutedAttentionConfig {
            window_size: 10,
            max_seq_length: 20,
            overlap_ratio: 0.5,
            ..Default::default()
        };
        let processor = DilutedAttentionProcessor::new(config);
        
        let content = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let windows = processor.process_document(content);
        let stats = processor.calculate_coverage(content, &windows);
        
        // Coverage should be complete
        assert!(stats.is_complete(), "Coverage ratio: {}", stats.coverage_ratio);
    }
    
    #[test]
    fn test_windows_to_chunks() {
        let processor = DilutedAttentionProcessor::with_defaults();
        let content = "This is test content for chunking";
        let file_id = Uuid::new_v4();
        
        let chunks = processor.process_to_chunks(content, file_id);
        
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert_eq!(chunk.file_id, file_id);
            assert_eq!(chunk.chunk_type, ChunkType::Paragraph);
        }
    }

    
    // Property-based tests
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// **Feature: neural-fs-core, Property 5: Chunk Coverage Invariant**
        /// *For any* valid document content, the diluted attention processor SHALL
        /// produce chunks that cover 100% of the original content.
        /// **Validates: Requirements 3.2**
        #[test]
        fn prop_chunk_coverage_complete(
            // Generate random content with 1-500 words
            word_count in 1usize..500usize,
            window_size in 10usize..100usize,
            overlap_ratio in 0.1f32..0.9f32,
        ) {
            // Generate content
            let words: Vec<String> = (0..word_count)
                .map(|i| format!("word{}", i))
                .collect();
            let content = words.join(" ");
            
            // Create processor with random config
            let config = DilutedAttentionConfig {
                window_size,
                max_seq_length: window_size * 2,
                overlap_ratio,
                dilution_factor: 4,
                max_global_context: 64,
            };
            let processor = DilutedAttentionProcessor::new(config);
            
            // Process document
            let windows = processor.process_document(&content);
            
            // Calculate coverage
            let stats = processor.calculate_coverage(&content, &windows);
            
            // Property: Coverage must be complete (100%)
            prop_assert!(
                stats.is_complete(),
                "Coverage incomplete: {:.2}% ({} of {} chars covered) with {} windows",
                stats.coverage_ratio * 100.0,
                stats.covered_chars,
                stats.total_chars,
                stats.window_count
            );
        }
        
        /// **Feature: neural-fs-core, Property 5: Chunk Coverage - No Gaps**
        /// *For any* document, there SHALL be no gaps between consecutive chunk locations.
        /// **Validates: Requirements 3.2**
        #[test]
        fn prop_chunk_no_gaps(
            word_count in 10usize..200usize,
        ) {
            let words: Vec<String> = (0..word_count)
                .map(|i| format!("w{}", i))
                .collect();
            let content = words.join(" ");
            
            let config = DilutedAttentionConfig {
                window_size: 20,
                max_seq_length: 40,
                overlap_ratio: 0.5,
                dilution_factor: 4,
                max_global_context: 32,
            };
            let processor = DilutedAttentionProcessor::new(config);
            
            let windows = processor.process_document(&content);
            
            // For documents that produce multiple windows, check for gaps
            if windows.len() > 1 {
                for i in 0..windows.len() - 1 {
                    let current_end = windows[i].end_offset;
                    let next_start = windows[i + 1].start_offset;
                    
                    // Next window should start at or before current window ends
                    // (allowing for overlap)
                    prop_assert!(
                        next_start <= current_end,
                        "Gap detected between window {} (end: {}) and window {} (start: {})",
                        i, current_end, i + 1, next_start
                    );
                }
            }
        }
        
        /// **Feature: neural-fs-core, Property 5: Chunk Index Monotonicity**
        /// *For any* document, chunk indices SHALL be monotonically increasing.
        /// **Validates: Requirements 3.2**
        #[test]
        fn prop_chunk_index_monotonic(
            word_count in 5usize..300usize,
        ) {
            let words: Vec<String> = (0..word_count)
                .map(|i| format!("token{}", i))
                .collect();
            let content = words.join(" ");
            
            let processor = DilutedAttentionProcessor::with_defaults();
            let file_id = Uuid::new_v4();
            
            let chunks = processor.process_to_chunks(&content, file_id);
            
            // Verify monotonically increasing indices
            for (i, chunk) in chunks.iter().enumerate() {
                prop_assert_eq!(
                    chunk.chunk_index as usize, i,
                    "Chunk index {} does not match position {}",
                    chunk.chunk_index, i
                );
            }
        }
        
        /// **Feature: neural-fs-core, Property 5: Global Context Bounded**
        /// *For any* document, global context size SHALL not exceed max_global_context.
        /// **Validates: Requirements 3.2, 4.2**
        #[test]
        fn prop_global_context_bounded(
            word_count in 1usize..1000usize,
            max_global in 16usize..256usize,
        ) {
            let words: Vec<String> = (0..word_count)
                .map(|i| format!("g{}", i))
                .collect();
            let content = words.join(" ");
            
            let config = DilutedAttentionConfig {
                window_size: 50,
                max_seq_length: 100,
                overlap_ratio: 0.5,
                dilution_factor: 4,
                max_global_context: max_global,
            };
            let processor = DilutedAttentionProcessor::new(config);
            
            let windows = processor.process_document(&content);
            
            for window in &windows {
                prop_assert!(
                    window.global_tokens.len() <= max_global,
                    "Global context {} exceeds max {}",
                    window.global_tokens.len(),
                    max_global
                );
            }
        }
        
        /// **Feature: neural-fs-core, Property 5: Window Size Bounded**
        /// *For any* document, local window size SHALL not exceed configured window_size.
        /// **Validates: Requirements 3.2, 4.2**
        #[test]
        fn prop_window_size_bounded(
            word_count in 1usize..500usize,
            window_size in 10usize..200usize,
        ) {
            let words: Vec<String> = (0..word_count)
                .map(|i| format!("x{}", i))
                .collect();
            let content = words.join(" ");
            
            let config = DilutedAttentionConfig {
                window_size,
                max_seq_length: window_size * 2,
                overlap_ratio: 0.5,
                dilution_factor: 4,
                max_global_context: 64,
            };
            let processor = DilutedAttentionProcessor::new(config);
            
            let windows = processor.process_document(&content);
            
            for window in &windows {
                prop_assert!(
                    window.local_tokens.len() <= window_size,
                    "Window size {} exceeds max {}",
                    window.local_tokens.len(),
                    window_size
                );
            }
        }
    }
}
