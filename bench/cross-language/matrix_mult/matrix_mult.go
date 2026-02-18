package main

import "fmt"

const N = 200

func main() {
	var A [N][N]float64
	var B [N][N]float64
	var C [N][N]float64

	// Initialize matrices
	for i := 0; i < N; i++ {
		for j := 0; j < N; j++ {
			A[i][j] = float64((i*N+j)%1000) / 1000.0
			B[i][j] = float64((j*N+i)%1000) / 1000.0
			C[i][j] = 0.0
		}
	}

	// Multiply C = A * B
	for i := 0; i < N; i++ {
		for j := 0; j < N; j++ {
			sum := 0.0
			for k := 0; k < N; k++ {
				sum += A[i][k] * B[k][j]
			}
			C[i][j] = sum
		}
	}

	// Checksum
	checksum := 0.0
	for i := 0; i < N; i++ {
		for j := 0; j < N; j++ {
			checksum += C[i][j]
		}
	}

	fmt.Printf("matrix_mult(200): checksum = %.6f\n", checksum)
}
