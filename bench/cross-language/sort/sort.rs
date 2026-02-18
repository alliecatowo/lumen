fn main() {
    let n: usize = 1_000_000;
    let mut data = Vec::with_capacity(n);

    // Deterministic pseudo-random fill (LCG)
    let mut val: u32 = 42;
    for _ in 0..n {
        val = val.wrapping_mul(1103515245).wrapping_add(12345);
        data.push((val % 100000) as i32);
    }

    data.sort();

    // Verify sorted
    let ok = data.windows(2).all(|w| w[0] <= w[1]);

    println!("sort({}) sorted={}", n, ok);
}
