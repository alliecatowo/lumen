// Fannkuch-Redux benchmark, N=10

const N = 10;

function fannkuch(n: number): void {
  const perm = new Array(n);
  const perm1 = new Array(n);
  const count = new Array(n);
  let maxFlips = 0;
  let checksum = 0;
  let r = n;
  let permCount = 0;

  for (let i = 0; i < n; i++) perm1[i] = i;

  outer:
  for (;;) {
    while (r > 1) { count[r - 1] = r; r--; }

    for (let i = 0; i < n; i++) perm[i] = perm1[i];

    // Count flips
    let flips = 0;
    let k = perm[0];
    while (k !== 0) {
      // Reverse first k+1 elements
      for (let i = 0, j = k; i < j; i++, j--) {
        const t = perm[i]; perm[i] = perm[j]; perm[j] = t;
      }
      flips++;
      k = perm[0];
    }

    if (flips > maxFlips) maxFlips = flips;
    checksum += (permCount % 2 === 0) ? flips : -flips;
    permCount++;

    // Next permutation
    for (;;) {
      if (r === n) break outer;
      const p0 = perm1[0];
      for (let i = 0; i < r; i++) perm1[i] = perm1[i + 1];
      perm1[r] = p0;
      count[r]--;
      if (count[r] > 0) break;
      r++;
    }
  }

  console.log(checksum);
  console.log(`Pfannkuchen(${n}) = ${maxFlips}`);
}

fannkuch(N);
