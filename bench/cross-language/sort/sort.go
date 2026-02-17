package main

import (
	"fmt"
	"sort"
)

func main() {
	n := 1000000
	data := make([]int, n)

	// Deterministic pseudo-random fill (LCG)
	val := uint32(42)
	for i := 0; i < n; i++ {
		val = val*1103515245 + 12345
		data[i] = int(val % 100000)
	}

	sort.Ints(data)

	// Verify sorted
	ok := true
	for i := 0; i < n-1; i++ {
		if data[i] > data[i+1] {
			ok = false
			break
		}
	}

	fmt.Printf("sort(%d) sorted=%v\n", n, ok)
}
