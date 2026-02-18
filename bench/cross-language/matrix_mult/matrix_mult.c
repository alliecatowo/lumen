/* Matrix multiplication — 200x200 dense naive O(n^3) */
#include <stdio.h>

#define N 200

double A[N][N], B[N][N], C[N][N];

int main() {
    /* Initialize matrices */
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            A[i][j] = ((i * N + j) % 1000) / 1000.0;
            B[i][j] = ((j * N + i) % 1000) / 1000.0;
            C[i][j] = 0.0;
        }
    }

    /* Multiply C = A * B */
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            double sum = 0.0;
            for (int k = 0; k < N; k++) {
                sum += A[i][k] * B[k][j];
            }
            C[i][j] = sum;
        }
    }

    /* Checksum: sum of all elements */
    double checksum = 0.0;
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            checksum += C[i][j];
        }
    }

    printf("matrix_mult(200): checksum = %.6f\n", checksum);
    return 0;
}
