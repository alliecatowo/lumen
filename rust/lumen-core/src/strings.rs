//! String interning table for fast comparisons.

use std::collections::HashMap;
use std::sync::Arc;

/// Intern table mapping strings to unique IDs.
#[repr(C)]
#[derive(Debug, Default)]
pub struct StringTable {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
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
        let arc = Arc::from(s);
        self.strings.push(Arc::clone(&arc));
        self.lookup.insert(arc, id);
        id
    }

    /// Return an interned Arc<str> for the given string.
    ///
    /// If the string is already interned, this reuses the existing allocation
    /// and returns a cloned Arc handle. Otherwise, it interns the string and
    /// returns the newly allocated Arc.
    #[inline]
    pub fn get_or_intern_arc(&mut self, s: &str) -> Arc<str> {
        if let Some(&id) = self.lookup.get(s) {
            return Arc::clone(
                self.strings
                    .get(id as usize)
                    .expect("string table ID must be valid"),
            );
        }
        let id = self.strings.len() as u32;
        let arc = Arc::from(s);
        self.strings.push(Arc::clone(&arc));
        self.lookup.insert(Arc::clone(&arc), id);
        arc
    }

    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_ref())
    }

    /// Return an owned Arc<str> for the given interned ID.
    #[inline]
    pub fn get_arc(&self, id: u32) -> Option<Arc<str>> {
        self.strings.get(id as usize).map(Arc::clone)
    }

    /// Look up the interned ID for a string without creating a new entry.
    /// Returns `None` if the string has never been interned.
    #[inline]
    pub fn get_id(&self, s: &str) -> Option<u32> {
        self.lookup.get(s).copied()
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

    #[test]
    fn test_get_or_intern_arc_reuses_allocation() {
        let mut table = StringTable::new();
        let first = table.get_or_intern_arc("leaf");
        let second = table.get_or_intern_arc("leaf");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(table.resolve(0), Some("leaf"));
    }
}
