// Mergesort benchmark: sort 100,000 integers [100000..1], print sum
const n = 100000;
const data = new Array(n);
for (let i = 0; i < n; i++) data[i] = n - i;
data.sort((a, b) => a - b);
let s = 0;
for (let i = 0; i < n; i++) s += data[i];
console.log(`sort(${n}) sum=${s}`);
