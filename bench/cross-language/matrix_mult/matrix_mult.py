"""Matrix multiplication — 200x200 dense naive O(n^3)"""

N = 200

# Initialize matrices
A = [[((i * N + j) % 1000) / 1000.0 for j in range(N)] for i in range(N)]
B = [[((j * N + i) % 1000) / 1000.0 for j in range(N)] for i in range(N)]
C = [[0.0] * N for _ in range(N)]

# Multiply C = A * B
for i in range(N):
    for j in range(N):
        s = 0.0
        for k in range(N):
            s += A[i][k] * B[k][j]
        C[i][j] = s

# Checksum
checksum = 0.0
for i in range(N):
    for j in range(N):
        checksum += C[i][j]

print(f"matrix_mult(200): checksum = {checksum:.6f}")
