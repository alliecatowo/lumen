/* Fannkuch-Redux benchmark, N=10 */
/* From the Computer Language Benchmarks Game */
#include <stdio.h>

#define N 10

int main() {
    int perm[N], perm1[N], count[N], temp[N];
    int max_flips = 0;
    int checksum = 0;
    int r = N;
    int perm_count = 0;

    for (int i = 0; i < N; i++) perm1[i] = i;

    for (;;) {
        while (r > 1) { count[r - 1] = r; r--; }

        for (int i = 0; i < N; i++) perm[i] = perm1[i];

        /* Count flips */
        int flips = 0;
        int k = perm[0];
        while (k != 0) {
            /* Reverse first k+1 elements */
            for (int i = 0; i < (k + 1) / 2; i++) {
                int t = perm[i];
                perm[i] = perm[k - i];
                perm[k - i] = t;
            }
            flips++;
            k = perm[0];
        }

        if (flips > max_flips) max_flips = flips;
        checksum += (perm_count % 2 == 0) ? flips : -flips;
        perm_count++;

        /* Next permutation */
        for (;;) {
            if (r == N) goto done;
            int p0 = perm1[0];
            for (int i = 0; i < r; i++) perm1[i] = perm1[i + 1];
            perm1[r] = p0;
            count[r]--;
            if (count[r] > 0) break;
            r++;
        }
    }

done:
    printf("%d\nPfannkuchen(%d) = %d\n", checksum, N, max_flips);
    return 0;
}
