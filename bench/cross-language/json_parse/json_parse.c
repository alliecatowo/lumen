#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define NUM_ENTRIES 10000
#define KEY_SIZE 16
#define VALUE_SIZE 16

typedef struct {
    char key[KEY_SIZE];
    char value[VALUE_SIZE];
} Entry;

Entry* build_entries(int count) {
    Entry* entries = (Entry*)malloc(count * sizeof(Entry));
    for (int i = 0; i < count; i++) {
        snprintf(entries[i].key, KEY_SIZE, "key_%d", i);
        snprintf(entries[i].value, VALUE_SIZE, "value_%d", i);
    }
    return entries;
}

const char* find_entry(Entry* entries, int count, const char* target) {
    for (int i = 0; i < count; i++) {
        if (strcmp(entries[i].key, target) == 0) {
            return entries[i].value;
        }
    }
    return "";
}

int main() {
    Entry* entries = build_entries(NUM_ENTRIES);
    const char* found = find_entry(entries, NUM_ENTRIES, "key_9999");
    printf("Found: %s\n", found);
    printf("Count: %d\n", NUM_ENTRIES);
    free(entries);
    return 0;
}
