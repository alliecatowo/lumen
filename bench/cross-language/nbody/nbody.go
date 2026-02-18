package main

import (
	"fmt"
	"math"
)

const (
	PI          = 3.141592653589793
	SolarMass   = 4.0 * PI * PI
	DaysPerYear = 365.24
	NumBodies   = 5
)

type Body struct {
	x, y, z    float64
	vx, vy, vz float64
	mass       float64
}

func offsetMomentum(bodies []Body) {
	px, py, pz := 0.0, 0.0, 0.0
	for _, b := range bodies {
		px += b.vx * b.mass
		py += b.vy * b.mass
		pz += b.vz * b.mass
	}
	bodies[0].vx = -px / SolarMass
	bodies[0].vy = -py / SolarMass
	bodies[0].vz = -pz / SolarMass
}

func energy(bodies []Body) float64 {
	e := 0.0
	for i := 0; i < len(bodies); i++ {
		b := &bodies[i]
		e += 0.5 * b.mass * (b.vx*b.vx + b.vy*b.vy + b.vz*b.vz)
		for j := i + 1; j < len(bodies); j++ {
			b2 := &bodies[j]
			dx := b.x - b2.x
			dy := b.y - b2.y
			dz := b.z - b2.z
			dist := math.Sqrt(dx*dx + dy*dy + dz*dz)
			e -= b.mass * b2.mass / dist
		}
	}
	return e
}

func advance(bodies []Body, dt float64) {
	for i := 0; i < len(bodies); i++ {
		for j := i + 1; j < len(bodies); j++ {
			dx := bodies[i].x - bodies[j].x
			dy := bodies[i].y - bodies[j].y
			dz := bodies[i].z - bodies[j].z
			d2 := dx*dx + dy*dy + dz*dz
			dist := math.Sqrt(d2)
			mag := dt / (d2 * dist)
			bodies[i].vx -= dx * bodies[j].mass * mag
			bodies[i].vy -= dy * bodies[j].mass * mag
			bodies[i].vz -= dz * bodies[j].mass * mag
			bodies[j].vx += dx * bodies[i].mass * mag
			bodies[j].vy += dy * bodies[i].mass * mag
			bodies[j].vz += dz * bodies[i].mass * mag
		}
	}
	for i := range bodies {
		bodies[i].x += dt * bodies[i].vx
		bodies[i].y += dt * bodies[i].vy
		bodies[i].z += dt * bodies[i].vz
	}
}

func main() {
	bodies := []Body{
		// Sun
		{0, 0, 0, 0, 0, 0, SolarMass},
		// Jupiter
		{4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
			1.66007664274403694e-03 * DaysPerYear, 7.69901118419740425e-03 * DaysPerYear,
			-6.90460016972063023e-05 * DaysPerYear, 9.54791938424326609e-04 * SolarMass},
		// Saturn
		{8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
			-2.76742510726862411e-03 * DaysPerYear, 4.99852801234917238e-03 * DaysPerYear,
			2.30417297573763929e-05 * DaysPerYear, 2.85885980666130812e-04 * SolarMass},
		// Uranus
		{1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
			2.96460137564761618e-03 * DaysPerYear, 2.37847173959480950e-03 * DaysPerYear,
			-2.96589568540237556e-05 * DaysPerYear, 4.36624404335156298e-05 * SolarMass},
		// Neptune
		{1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
			2.68067772490389322e-03 * DaysPerYear, 1.62824170038242295e-03 * DaysPerYear,
			-9.51592254519715870e-05 * DaysPerYear, 5.15138902046611451e-05 * SolarMass},
	}

	offsetMomentum(bodies)
	fmt.Printf("%.9f\n", energy(bodies))
	for i := 0; i < 1000000; i++ {
		advance(bodies, 0.01)
	}
	fmt.Printf("%.9f\n", energy(bodies))
}
