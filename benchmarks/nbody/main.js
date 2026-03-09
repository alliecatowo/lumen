// N-body gravitational simulation — 5-body solar system, 50,000 steps
const PI = Math.PI;
const SOLAR_MASS = 4.0 * PI * PI;
const DAYS_PER_YEAR = 365.24;

const bodies = [
  // Sun
  [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, SOLAR_MASS],
  // Jupiter
  [4.84143144246472090, -1.16032004402742839, -0.103622044471123109,
   1.66007664274403694e-3 * DAYS_PER_YEAR, 7.69901118419740425e-3 * DAYS_PER_YEAR,
   -6.90460016972063023e-5 * DAYS_PER_YEAR, 9.54791938424326609e-4 * SOLAR_MASS],
  // Saturn
  [8.34336671824457987, 4.12479856412430479, -0.403523417114321381,
   -2.76742510726862411e-3 * DAYS_PER_YEAR, 4.99852801234917238e-3 * DAYS_PER_YEAR,
   2.30417297573763929e-5 * DAYS_PER_YEAR, 2.85885980666130812e-4 * SOLAR_MASS],
  // Uranus
  [12.8943695621391310, -15.1111514016986312, -0.223307578892655734,
   2.96460137564761618e-3 * DAYS_PER_YEAR, 2.37847173959480950e-3 * DAYS_PER_YEAR,
   -2.96589568540237556e-5 * DAYS_PER_YEAR, 4.36624404335156298e-5 * SOLAR_MASS],
  // Neptune
  [15.3796971148509165, -25.9193146099879641, 0.179258772950371181,
   2.68067772490389322e-3 * DAYS_PER_YEAR, 1.62824170038242295e-3 * DAYS_PER_YEAR,
   -9.51592254519715870e-5 * DAYS_PER_YEAR, 5.15138902046611451e-5 * SOLAR_MASS],
];

// Offset momentum
let px = 0, py = 0, pz = 0;
for (let i = 1; i < bodies.length; i++) {
  px += bodies[i][3] * bodies[i][6];
  py += bodies[i][4] * bodies[i][6];
  pz += bodies[i][5] * bodies[i][6];
}
bodies[0][3] = -px / SOLAR_MASS;
bodies[0][4] = -py / SOLAR_MASS;
bodies[0][5] = -pz / SOLAR_MASS;

function energy() {
  let e = 0;
  for (let i = 0; i < bodies.length; i++) {
    const b = bodies[i];
    e += 0.5 * b[6] * (b[3]*b[3] + b[4]*b[4] + b[5]*b[5]);
    for (let j = i + 1; j < bodies.length; j++) {
      const c = bodies[j];
      const dx = b[0]-c[0], dy = b[1]-c[1], dz = b[2]-c[2];
      e -= b[6] * c[6] / Math.sqrt(dx*dx + dy*dy + dz*dz);
    }
  }
  return e;
}

function advance(dt, n) {
  for (let s = 0; s < n; s++) {
    for (let i = 0; i < bodies.length; i++) {
      const bi = bodies[i];
      for (let j = i + 1; j < bodies.length; j++) {
        const bj = bodies[j];
        const dx = bi[0]-bj[0], dy = bi[1]-bj[1], dz = bi[2]-bj[2];
        const d2 = dx*dx + dy*dy + dz*dz;
        const mag = dt / (d2 * Math.sqrt(d2));
        bi[3] -= dx * bj[6] * mag; bi[4] -= dy * bj[6] * mag; bi[5] -= dz * bj[6] * mag;
        bj[3] += dx * bi[6] * mag; bj[4] += dy * bi[6] * mag; bj[5] += dz * bi[6] * mag;
      }
    }
    for (const b of bodies) {
      b[0] += dt * b[3]; b[1] += dt * b[4]; b[2] += dt * b[5];
    }
  }
}

console.log(energy().toFixed(9));
advance(0.01, 50000);
console.log(energy().toFixed(9));
