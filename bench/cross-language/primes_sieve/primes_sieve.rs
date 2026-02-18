// Sieve of Eratosthenes — primes up to 1,000,000

fn main() {
    let limit: usize = 1_000_000;
    let mut sieve = vec![false; limit + 1];
    sieve[0] = true;
    sieve[1] = true;

    let mut i = 2;
    while i * i <= limit {
        if !sieve[i] {
            let mut j = i * i;
            while j <= limit {
                sieve[j] = true;
                j += i;
            }
        }
        i += 1;
    }

    let count = sieve[2..].iter().filter(|&&x| !x).count();
    println!("primes_sieve(1000000): count = {}", count);
}
