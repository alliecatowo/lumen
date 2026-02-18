#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Quicksort partition */
int partition(int *arr, int lo, int hi) {
    int pivot = arr[hi];
    int i = lo - 1;
    for (int j = lo; j < hi; j++) {
        if (arr[j] <= pivot) {
            i++;
            int tmp = arr[i];
            arr[i] = arr[j];
            arr[j] = tmp;
        }
    }
    int tmp = arr[i + 1];
    arr[i + 1] = arr[hi];
    arr[hi] = tmp;
    return i + 1;
}

void quicksort(int *arr, int lo, int hi) {
    if (lo < hi) {
        int p = partition(arr, lo, hi);
        quicksort(arr, lo, p - 1);
        quicksort(arr, p + 1, hi);
    }
}

int main() {
    int n = 1000000;
    int *data = malloc(n * sizeof(int));
    if (!data) { fprintf(stderr, "alloc failed\n"); return 1; }

    /* Deterministic pseudo-random fill (LCG) */
    unsigned int val = 42;
    for (int i = 0; i < n; i++) {
        val = val * 1103515245u + 12345u;
        data[i] = (int)(val % 100000u);
    }

    quicksort(data, 0, n - 1);

    /* Verify sorted */
    int ok = 1;
    for (int i = 0; i < n - 1; i++) {
        if (data[i] > data[i + 1]) { ok = 0; break; }
    }

    printf("sort(%d) sorted=%s\n", n, ok ? "true" : "false");
    free(data);
    return 0;
}
