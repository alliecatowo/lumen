//! Linear (consuming) collection types for zero-copy pipelines.
//!
//! These types take `self` by value on every operation, making ownership
//! transfer explicit and enabling the compiler to avoid unnecessary clones.

use std::collections::HashMap;
use std::hash::Hash;

// ---------------------------------------------------------------------------
// LinearVec
// ---------------------------------------------------------------------------

/// A vector that consumes `self` on every operation, enabling zero-copy
/// pipelines where ownership is threaded through each transformation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearVec<T> {
    inner: Vec<T>,
}

impl<T> LinearVec<T> {
    /// Create an empty `LinearVec`.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Create a `LinearVec` from an existing `Vec`.
    pub fn from_vec(v: Vec<T>) -> Self {
        Self { inner: v }
    }

    /// Append an item, consuming self.
    pub fn push(mut self, item: T) -> Self {
        self.inner.push(item);
        self
    }

    /// Remove and return the last item, consuming self.
    pub fn pop(mut self) -> (Self, Option<T>) {
        let item = self.inner.pop();
        (self, item)
    }

    /// Transform each element, consuming self.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> LinearVec<U> {
        LinearVec {
            inner: self.inner.into_iter().map(f).collect(),
        }
    }

    /// Keep only elements satisfying the predicate, consuming self.
    pub fn filter(self, f: impl Fn(&T) -> bool) -> Self {
        Self {
            inner: self.inner.into_iter().filter(|x| f(x)).collect(),
        }
    }

    /// Concatenate two vectors, consuming both.
    pub fn concat(mut self, other: Self) -> Self {
        self.inner.extend(other.inner);
        self
    }

    /// Consume into a standard `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        self.inner
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get a reference to the element at `index`.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }

    /// Reverse the elements, consuming self.
    pub fn reverse(mut self) -> Self {
        self.inner.reverse();
        self
    }

    /// Fold/reduce over the elements, consuming self.
    pub fn fold<A>(self, init: A, f: impl FnMut(A, T) -> A) -> A {
        self.inner.into_iter().fold(init, f)
    }
}

impl<T> Default for LinearVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<Vec<T>> for LinearVec<T> {
    fn from(v: Vec<T>) -> Self {
        Self::from_vec(v)
    }
}

impl<T> IntoIterator for LinearVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

// ---------------------------------------------------------------------------
// LinearMap
// ---------------------------------------------------------------------------

/// A map that consumes `self` on every operation.
#[derive(Debug, Clone)]
pub struct LinearMap<K, V> {
    inner: HashMap<K, V>,
}

impl<K: Eq + Hash, V: PartialEq> PartialEq for LinearMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<K: Eq + Hash, V: Eq> Eq for LinearMap<K, V> {}

impl<K: Eq + Hash, V> LinearMap<K, V> {
    /// Create an empty `LinearMap`.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Create a `LinearMap` from an existing `HashMap`.
    pub fn from_map(m: HashMap<K, V>) -> Self {
        Self { inner: m }
    }

    /// Insert a key-value pair, consuming self.
    pub fn insert(mut self, key: K, value: V) -> Self {
        self.inner.insert(key, value);
        self
    }

    /// Remove a key, consuming self. Returns the map and the removed value.
    pub fn remove(mut self, key: &K) -> (Self, Option<V>) {
        let value = self.inner.remove(key);
        (self, value)
    }

    /// Merge another map into self, consuming both. On key conflict, `other`
    /// wins.
    pub fn merge(mut self, other: Self) -> Self {
        self.inner.extend(other.inner);
        self
    }

    /// Consume into a standard `HashMap`.
    pub fn into_map(self) -> HashMap<K, V> {
        self.inner
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get a reference to the value for `key`.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    /// Check if a key is present.
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Get all values.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.values()
    }
}

impl<K: Eq + Hash, V> Default for LinearMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash, V> From<HashMap<K, V>> for LinearMap<K, V> {
    fn from(m: HashMap<K, V>) -> Self {
        Self::from_map(m)
    }
}

impl<K: Eq + Hash, V> IntoIterator for LinearMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::collections::hash_map::IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// A pipeline builder that chains consuming transformations.
///
/// ```ignore
/// let result = Pipeline::new(vec![1, 2, 3, 4, 5])
///     .then(|v| v.into_iter().filter(|x| x % 2 == 0).collect::<Vec<_>>())
///     .then(|v| v.into_iter().map(|x| x * 10).collect::<Vec<_>>())
///     .finish();
/// assert_eq!(result, vec![20, 40]);
/// ```
#[derive(Debug, Clone)]
pub struct Pipeline<T> {
    value: T,
}

impl<T> Pipeline<T> {
    /// Start a pipeline with the given data.
    pub fn new(data: T) -> Self {
        Self { value: data }
    }

    /// Apply a transformation, consuming self and returning a new pipeline.
    pub fn then<U>(self, f: impl FnOnce(T) -> U) -> Pipeline<U> {
        Pipeline {
            value: f(self.value),
        }
    }

    /// Extract the final result.
    pub fn finish(self) -> T {
        self.value
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- LinearVec ----------------------------------------------------------

    #[test]
    fn linear_vec_new_is_empty() {
        let v: LinearVec<i32> = LinearVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn linear_vec_push() {
        let v = LinearVec::new().push(1).push(2).push(3);
        assert_eq!(v.len(), 3);
        assert_eq!(v.get(0), Some(&1));
        assert_eq!(v.get(2), Some(&3));
    }

    #[test]
    fn linear_vec_pop() {
        let v = LinearVec::from_vec(vec![10, 20, 30]);
        let (v, item) = v.pop();
        assert_eq!(item, Some(30));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn linear_vec_pop_empty() {
        let v: LinearVec<i32> = LinearVec::new();
        let (v, item) = v.pop();
        assert_eq!(item, None);
        assert!(v.is_empty());
    }

    #[test]
    fn linear_vec_map() {
        let v = LinearVec::from_vec(vec![1, 2, 3]);
        let v2 = v.map(|x| x * 10);
        assert_eq!(v2.into_vec(), vec![10, 20, 30]);
    }

    #[test]
    fn linear_vec_filter() {
        let v = LinearVec::from_vec(vec![1, 2, 3, 4, 5]);
        let v2 = v.filter(|x| x % 2 == 0);
        assert_eq!(v2.into_vec(), vec![2, 4]);
    }

    #[test]
    fn linear_vec_concat() {
        let a = LinearVec::from_vec(vec![1, 2]);
        let b = LinearVec::from_vec(vec![3, 4]);
        let c = a.concat(b);
        assert_eq!(c.into_vec(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn linear_vec_into_vec() {
        let v = LinearVec::from_vec(vec![42]);
        assert_eq!(v.into_vec(), vec![42]);
    }

    #[test]
    fn linear_vec_reverse() {
        let v = LinearVec::from_vec(vec![1, 2, 3]);
        let v2 = v.reverse();
        assert_eq!(v2.into_vec(), vec![3, 2, 1]);
    }

    #[test]
    fn linear_vec_fold() {
        let v = LinearVec::from_vec(vec![1, 2, 3, 4]);
        let sum = v.fold(0, |acc, x| acc + x);
        assert_eq!(sum, 10);
    }

    #[test]
    fn linear_vec_from_vec_trait() {
        let v: LinearVec<i32> = vec![1, 2, 3].into();
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn linear_vec_default() {
        let v: LinearVec<String> = LinearVec::default();
        assert!(v.is_empty());
    }

    #[test]
    fn linear_vec_into_iter() {
        let v = LinearVec::from_vec(vec![10, 20, 30]);
        let collected: Vec<i32> = v.into_iter().collect();
        assert_eq!(collected, vec![10, 20, 30]);
    }

    #[test]
    fn linear_vec_chained_operations() {
        let result = LinearVec::new()
            .push(5)
            .push(3)
            .push(8)
            .push(1)
            .filter(|x| *x > 2)
            .map(|x| x * 2)
            .reverse()
            .into_vec();
        assert_eq!(result, vec![16, 6, 10]);
    }

    #[test]
    fn linear_vec_map_type_change() {
        let v = LinearVec::from_vec(vec![1, 2, 3]);
        let v2: LinearVec<String> = v.map(|x| format!("num_{}", x));
        assert_eq!(v2.get(0), Some(&"num_1".to_string()));
    }

    // -- LinearMap ----------------------------------------------------------

    #[test]
    fn linear_map_new_is_empty() {
        let m: LinearMap<String, i32> = LinearMap::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn linear_map_insert_and_get() {
        let m = LinearMap::new().insert("a", 1).insert("b", 2);
        assert_eq!(m.get(&"a"), Some(&1));
        assert_eq!(m.get(&"b"), Some(&2));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn linear_map_insert_overwrites() {
        let m = LinearMap::new().insert("a", 1).insert("a", 99);
        assert_eq!(m.get(&"a"), Some(&99));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn linear_map_remove() {
        let m = LinearMap::new().insert("x", 10).insert("y", 20);
        let (m, removed) = m.remove(&"x");
        assert_eq!(removed, Some(10));
        assert_eq!(m.len(), 1);
        assert!(!m.contains_key(&"x"));
    }

    #[test]
    fn linear_map_remove_missing() {
        let m = LinearMap::new().insert("a", 1);
        let (m, removed) = m.remove(&"z");
        assert_eq!(removed, None);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn linear_map_merge() {
        let a = LinearMap::new().insert("x", 1).insert("y", 2);
        let b = LinearMap::new().insert("y", 99).insert("z", 3);
        let merged = a.merge(b);
        assert_eq!(merged.get(&"x"), Some(&1));
        assert_eq!(merged.get(&"y"), Some(&99)); // b wins on conflict
        assert_eq!(merged.get(&"z"), Some(&3));
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn linear_map_into_map() {
        let m = LinearMap::new().insert("k", 42);
        let hm = m.into_map();
        assert_eq!(hm.get("k"), Some(&42));
    }

    #[test]
    fn linear_map_contains_key() {
        let m = LinearMap::new().insert("present", ());
        assert!(m.contains_key(&"present"));
        assert!(!m.contains_key(&"absent"));
    }

    #[test]
    fn linear_map_default() {
        let m: LinearMap<i32, i32> = LinearMap::default();
        assert!(m.is_empty());
    }

    #[test]
    fn linear_map_from_hashmap() {
        let mut hm = HashMap::new();
        hm.insert("a", 1);
        hm.insert("b", 2);
        let m: LinearMap<&str, i32> = hm.into();
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn linear_map_into_iter() {
        let m = LinearMap::new().insert("a", 1).insert("b", 2);
        let collected: HashMap<&str, i32> = m.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn linear_map_keys_and_values() {
        let m = LinearMap::new().insert("a", 10).insert("b", 20);
        let keys: Vec<&&str> = m.keys().collect();
        assert_eq!(keys.len(), 2);
        let values: Vec<&i32> = m.values().collect();
        assert_eq!(values.len(), 2);
    }

    // -- Pipeline -----------------------------------------------------------

    #[test]
    fn pipeline_identity() {
        let result = Pipeline::new(42).finish();
        assert_eq!(result, 42);
    }

    #[test]
    fn pipeline_single_step() {
        let result = Pipeline::new(10).then(|x| x * 2).finish();
        assert_eq!(result, 20);
    }

    #[test]
    fn pipeline_multi_step() {
        let result = Pipeline::new(vec![1, 2, 3, 4, 5])
            .then(|v| v.into_iter().filter(|x| x % 2 == 0).collect::<Vec<_>>())
            .then(|v| v.into_iter().map(|x| x * 10).collect::<Vec<_>>())
            .finish();
        assert_eq!(result, vec![20, 40]);
    }

    #[test]
    fn pipeline_type_change() {
        let result = Pipeline::new(42)
            .then(|x| format!("value={}", x))
            .then(|s| s.len())
            .finish();
        assert_eq!(result, 8); // "value=42".len()
    }

    #[test]
    fn pipeline_with_linear_vec() {
        let result = Pipeline::new(LinearVec::from_vec(vec![1, 2, 3, 4, 5]))
            .then(|v| v.filter(|x| *x > 2))
            .then(|v| v.map(|x| x * 100))
            .then(|v| v.into_vec())
            .finish();
        assert_eq!(result, vec![300, 400, 500]);
    }

    #[test]
    fn pipeline_with_linear_map() {
        let result = Pipeline::new(LinearMap::new())
            .then(|m| m.insert("a", 1))
            .then(|m| m.insert("b", 2))
            .then(|m| m.insert("c", 3))
            .then(|m| m.len())
            .finish();
        assert_eq!(result, 3);
    }
}
