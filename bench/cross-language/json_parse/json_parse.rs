use std::collections::HashMap;

fn main() {
    let mut data: HashMap<String, String> = HashMap::with_capacity(10_000);

    for i in 0..10_000 {
        data.insert(format!("key_{}", i), format!("value_{}", i));
    }

    let found = data.get("key_9999").map(|s| s.as_str()).unwrap_or("");
    println!("Found: {}", found);
    println!("Count: {}", data.len());
}
