// Matrix multiplication — 200x200 dense naive O(n^3)

const N = 200;

// Initialize matrices
const A: number[][] = [];
const B: number[][] = [];
const C: number[][] = [];

for (let i = 0; i < N; i++) {
  A[i] = new Array(N);
  B[i] = new Array(N);
  C[i] = new Array(N).fill(0.0);
  for (let j = 0; j < N; j++) {
    A[i][j] = ((i * N + j) % 1000) / 1000.0;
    B[i][j] = ((j * N + i) % 1000) / 1000.0;
  }
}

// Multiply C = A * B
for (let i = 0; i < N; i++) {
  for (let j = 0; j < N; j++) {
    let sum = 0.0;
    for (let k = 0; k < N; k++) {
      sum += A[i][k] * B[k][j];
    }
    C[i][j] = sum;
  }
}

// Checksum
let checksum = 0.0;
for (let i = 0; i < N; i++) {
  for (let j = 0; j < N; j++) {
    checksum += C[i][j];
  }
}

console.log(`matrix_mult(200): checksum = ${checksum.toFixed(6)}`);
