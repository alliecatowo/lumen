package main

import "fmt"

const N = 10

func main() {
	perm := make([]int, N)
	perm1 := make([]int, N)
	count := make([]int, N)
	maxFlips := 0
	checksum := 0
	r := N
	permCount := 0

	for i := 0; i < N; i++ {
		perm1[i] = i
	}

	for {
		for r > 1 {
			count[r-1] = r
			r--
		}

		copy(perm, perm1)

		// Count flips
		flips := 0
		k := perm[0]
		for k != 0 {
			// Reverse first k+1 elements
			for i, j := 0, k; i < j; i, j = i+1, j-1 {
				perm[i], perm[j] = perm[j], perm[i]
			}
			flips++
			k = perm[0]
		}

		if flips > maxFlips {
			maxFlips = flips
		}
		if permCount%2 == 0 {
			checksum += flips
		} else {
			checksum -= flips
		}
		permCount++

		// Next permutation
		found := false
		for {
			if r == N {
				goto done
			}
			p0 := perm1[0]
			for i := 0; i < r; i++ {
				perm1[i] = perm1[i+1]
			}
			perm1[r] = p0
			count[r]--
			if count[r] > 0 {
				found = true
				break
			}
			r++
		}
		if !found {
			break
		}
	}

done:
	fmt.Printf("%d\nPfannkuchen(%d) = %d\n", checksum, N, maxFlips)
}
