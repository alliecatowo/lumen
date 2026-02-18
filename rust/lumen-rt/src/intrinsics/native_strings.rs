//! Fast native string concatenation for JIT-compiled code.
//!
//! This module provides optimized string operations that can be called directly
//! from JIT-compiled code. The primary optimization is in-place concatenation
//! when the left-hand string has sufficient capacity.

/// Concatenate two heap strings with in-place optimization.
///
/// Takes ownership of both input strings (they are `Box<String>` pointers).
/// If `a` has enough capacity to hold `b`'s contents, performs an in-place
/// concatenation to avoid reallocation. Otherwise, performs a standard push_str
/// which may reallocate.
///
/// The second string `b` is always dropped after its contents are consumed.
/// The first string `a` is returned (possibly with expanded contents).
///
/// # Arguments
///
/// * `a` - Pointer to the first string (as `*mut String`), which will be extended
/// * `b` - Pointer to the second string (as `*mut String`), which will be consumed
///
/// # Returns
///
/// A pointer to the concatenated string (same as input `a`, but modified).
///
/// # Safety
///
/// Both `a` and `b` must be valid `*mut String` pointers created by boxing a String.
/// The caller must not use either pointer after calling this function, except for
/// the returned pointer which represents the concatenated result.
///
/// # Example
///
/// ```ignore
/// // From JIT-compiled code:
/// let a = Box::into_raw(Box::new(String::from("hello ")));
/// let b = Box::into_raw(Box::new(String::from("world")));
/// let result = lumen_rt_string_concat(a, b);
/// // result points to a String containing "hello world"
/// // b has been freed, a has been modified in-place
/// ```
#[no_mangle]
pub extern "C" fn lumen_rt_string_concat(a: *mut String, b: *mut String) -> *mut String {
    // Take ownership of both strings
    let mut boxed_a = unsafe { Box::from_raw(a) };
    let boxed_b = unsafe { Box::from_raw(b) };

    // Check if `a` has enough capacity to hold `b` without reallocation
    let remaining_capacity = boxed_a.capacity() - boxed_a.len();

    if remaining_capacity >= boxed_b.len() {
        // Fast path: in-place concatenation
        boxed_a.push_str(&boxed_b);
    } else {
        // Standard path: push_str (which may reallocate)
        boxed_a.push_str(&boxed_b);
    }

    // `boxed_b` is dropped here automatically

    // Return the modified `a` as a raw pointer
    Box::into_raw(boxed_a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concat_with_sufficient_capacity() {
        let mut s1 = String::with_capacity(100);
        s1.push_str("hello ");
        let s2 = String::from("world");

        let a = Box::into_raw(Box::new(s1));
        let b = Box::into_raw(Box::new(s2));

        let result = lumen_rt_string_concat(a, b);

        let result_string = unsafe { Box::from_raw(result) };
        assert_eq!(&*result_string, "hello world");
    }

    #[test]
    fn test_concat_requires_reallocation() {
        let s1 = String::from("hello ");
        let s2 = String::from("world!");

        let a = Box::into_raw(Box::new(s1));
        let b = Box::into_raw(Box::new(s2));

        let result = lumen_rt_string_concat(a, b);

        let result_string = unsafe { Box::from_raw(result) };
        assert_eq!(&*result_string, "hello world!");
    }

    #[test]
    fn test_concat_empty_strings() {
        let s1 = String::new();
        let s2 = String::new();

        let a = Box::into_raw(Box::new(s1));
        let b = Box::into_raw(Box::new(s2));

        let result = lumen_rt_string_concat(a, b);

        let result_string = unsafe { Box::from_raw(result) };
        assert_eq!(&*result_string, "");
    }

    #[test]
    fn test_concat_one_empty() {
        let s1 = String::from("hello");
        let s2 = String::new();

        let a = Box::into_raw(Box::new(s1));
        let b = Box::into_raw(Box::new(s2));

        let result = lumen_rt_string_concat(a, b);

        let result_string = unsafe { Box::from_raw(result) };
        assert_eq!(&*result_string, "hello");
    }
}
