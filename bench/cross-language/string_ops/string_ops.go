package main

import (
	"fmt"
	"strings"
)

func main() {
	var builder strings.Builder
	for i := 0; i < 100000; i++ {
		builder.WriteString("x")
	}
	s := builder.String()
	fmt.Printf("Length: %d\n", len(s))
}
