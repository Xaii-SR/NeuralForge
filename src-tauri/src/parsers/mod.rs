use std::collections::HashMap;

pub struct ParsedDocument {
    pub normalized_text: String,
    pub sections: Vec<DocumentSection>,
    pub metadata: HashMap<String, String>,
}

pub struct DocumentSection {
    pub heading: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub trait FileParser: Send + Sync {
    fn supports(&self, extension: &str) -> bool;
    fn parse(&self, content: &str) -> ParsedDocument;
    fn version(&self) -> u32;
}

pub struct ParserRegistry {
    parsers: Vec<Box<dyn FileParser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self { parsers: Vec::new() }
    }

    pub fn register(&mut self, parser: Box<dyn FileParser>) {
        self.parsers.push(parser);
    }

    pub fn find(&self, extension: &str) -> Option<&dyn FileParser> {
        self.parsers.iter().find(|p| p.supports(extension)).map(|p| p.as_ref())
    }
}