// Sieve of Eratosthenes — primes up to 1,000,000

const limit = 1000000;
const sieve = new Uint8Array(limit + 1);

sieve[0] = 1;
sieve[1] = 1;

for (let i = 2; i * i <= limit; i++) {
  if (!sieve[i]) {
    for (let j = i * i; j <= limit; j += i) {
      sieve[j] = 1;
    }
  }
}

let count = 0;
for (let i = 2; i <= limit; i++) {
  if (!sieve[i]) count++;
}

console.log(`primes_sieve(1000000): count = ${count}`);
