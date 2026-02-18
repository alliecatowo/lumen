package main

import "fmt"

func main() {
	limit := 1000000
	sieve := make([]bool, limit+1)

	sieve[0] = true
	sieve[1] = true

	for i := 2; i*i <= limit; i++ {
		if !sieve[i] {
			for j := i * i; j <= limit; j += i {
				sieve[j] = true
			}
		}
	}

	count := 0
	for i := 2; i <= limit; i++ {
		if !sieve[i] {
			count++
		}
	}

	fmt.Printf("primes_sieve(1000000): count = %d\n", count)
}
