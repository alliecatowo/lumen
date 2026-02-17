const n = 1_000_000;
const data: number[] = new Array(n);

// Deterministic pseudo-random fill (LCG)
let val = 42;
for (let i = 0; i < n; i++) {
  val = (val * 1103515245 + 12345) & 0x7fffffff;
  data[i] = val % 100000;
}

data.sort((a, b) => a - b);

// Verify sorted
let ok = true;
for (let i = 0; i < n - 1; i++) {
  if (data[i] > data[i + 1]) {
    ok = false;
    break;
  }
}

console.log(`sort(${n}) sorted=${ok}`);
