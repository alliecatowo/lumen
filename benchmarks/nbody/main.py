#!/usr/bin/env python3
"""N-body gravitational simulation — 5-body solar system, 50,000 steps"""
import math

PI = math.pi
SOLAR_MASS = 4.0 * PI * PI
DAYS_PER_YEAR = 365.24

BODIES = [
    # Sun
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, SOLAR_MASS],
    # Jupiter
    [4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
     1.66007664274403694e-03 * DAYS_PER_YEAR, 7.69901118419740425e-03 * DAYS_PER_YEAR,
     -6.90460016972063023e-05 * DAYS_PER_YEAR, 9.54791938424326609e-04 * SOLAR_MASS],
    # Saturn
    [8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
     -2.76742510726862411e-03 * DAYS_PER_YEAR, 4.99852801234917238e-03 * DAYS_PER_YEAR,
     2.30417297573763929e-05 * DAYS_PER_YEAR, 2.85885980666130812e-04 * SOLAR_MASS],
    # Uranus
    [1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
     2.96460137564761618e-03 * DAYS_PER_YEAR, 2.37847173959480950e-03 * DAYS_PER_YEAR,
     -2.96589568540237556e-05 * DAYS_PER_YEAR, 4.36624404335156298e-05 * SOLAR_MASS],
    # Neptune
    [1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
     2.68067772490389322e-03 * DAYS_PER_YEAR, 1.62824170038242295e-03 * DAYS_PER_YEAR,
     -9.51592254519715870e-05 * DAYS_PER_YEAR, 5.15138902046611451e-05 * SOLAR_MASS],
]

# Offset momentum
px = py = pz = 0.0
for b in BODIES[1:]:
    px += b[3] * b[6]
    py += b[4] * b[6]
    pz += b[5] * b[6]
BODIES[0][3] = -px / SOLAR_MASS
BODIES[0][4] = -py / SOLAR_MASS
BODIES[0][5] = -pz / SOLAR_MASS


def energy():
    e = 0.0
    n = len(BODIES)
    for i in range(n):
        b = BODIES[i]
        e += 0.5 * b[6] * (b[3]**2 + b[4]**2 + b[5]**2)
        for j in range(i + 1, n):
            c = BODIES[j]
            dx = b[0] - c[0]; dy = b[1] - c[1]; dz = b[2] - c[2]
            e -= b[6] * c[6] / math.sqrt(dx**2 + dy**2 + dz**2)
    return e


def advance(dt, n):
    bodies = BODIES
    nb = len(bodies)
    for _ in range(n):
        for i in range(nb):
            bi = bodies[i]
            for j in range(i + 1, nb):
                bj = bodies[j]
                dx = bi[0] - bj[0]; dy = bi[1] - bj[1]; dz = bi[2] - bj[2]
                d2 = dx**2 + dy**2 + dz**2
                mag = dt / (d2 * math.sqrt(d2))
                bi[3] -= dx * bj[6] * mag; bi[4] -= dy * bj[6] * mag; bi[5] -= dz * bj[6] * mag
                bj[3] += dx * bi[6] * mag; bj[4] += dy * bi[6] * mag; bj[5] += dz * bi[6] * mag
        for b in bodies:
            b[0] += dt * b[3]; b[1] += dt * b[4]; b[2] += dt * b[5]


print(f"{energy():.9f}")
advance(0.01, 50000)
print(f"{energy():.9f}")
