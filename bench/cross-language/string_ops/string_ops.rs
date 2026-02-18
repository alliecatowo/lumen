fn main() {
    let mut s = String::with_capacity(100_000);
    for _ in 0..100_000 {
        s.push('x');
    }
    println!("Length: {}", s.len());
}
