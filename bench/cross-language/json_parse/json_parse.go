package main

import (
	"encoding/json"
	"fmt"
)

func main() {
	// Build a JSON string with 10000 entries
	data := make(map[string]string)
	for i := 0; i < 10000; i++ {
		key := fmt.Sprintf("key_%d", i)
		value := fmt.Sprintf("value_%d", i)
		data[key] = value
	}

	// Serialize to JSON
	jsonBytes, _ := json.Marshal(data)

	// Parse back
	var parsed map[string]string
	json.Unmarshal(jsonBytes, &parsed)

	// Access a field
	found := parsed["key_9999"]
	fmt.Printf("Found: %s\n", found)
	fmt.Printf("Count: %d\n", len(parsed))
}
