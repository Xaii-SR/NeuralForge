use sha2::{Sha256, Digest};

pub struct HashService;

impl HashService {
    pub fn file_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    pub fn content_hash(normalized_text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(normalized_text.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn chunk_hash(chunk_text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(chunk_text.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}