/* Sieve of Eratosthenes — primes up to 1,000,000 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main() {
    int limit = 1000000;
    char *sieve = (char *)calloc(limit + 1, 1);
    if (!sieve) return 1;

    /* 1 = composite, 0 = prime candidate */
    sieve[0] = 1;
    sieve[1] = 1;

    for (int i = 2; (long long)i * i <= limit; i++) {
        if (!sieve[i]) {
            for (int j = i * i; j <= limit; j += i) {
                sieve[j] = 1;
            }
        }
    }

    int count = 0;
    for (int i = 2; i <= limit; i++) {
        if (!sieve[i]) count++;
    }

    printf("primes_sieve(1000000): count = %d\n", count);
    free(sieve);
    return 0;
}
