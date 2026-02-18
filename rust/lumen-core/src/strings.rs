//! String interning table for fast comparisons.

use std::collections::HashMap;

/// Intern table mapping strings to unique IDs.
#[derive(Debug, Default)]
pub struct StringTable {
    strings: Vec<String>,
    lookup: HashMap<String, u32>,
}

impl StringTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.lookup.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.lookup.insert(s.to_string(), id);
        id
    }

    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern() {
        let mut table = StringTable::new();
        let id1 = table.intern("hello");
        let id2 = table.intern("world");
        let id3 = table.intern("hello");
        assert_eq!(id1, id3); // same string = same ID
        assert_ne!(id1, id2);
        assert_eq!(table.resolve(id1), Some("hello"));
    }
}
