// Matrix multiplication — 200x200 dense naive O(n^3)

const N: usize = 200;

fn main() {
    let mut a = vec![vec![0.0f64; N]; N];
    let mut b = vec![vec![0.0f64; N]; N];
    let mut c = vec![vec![0.0f64; N]; N];

    // Initialize matrices
    for i in 0..N {
        for j in 0..N {
            a[i][j] = ((i * N + j) % 1000) as f64 / 1000.0;
            b[i][j] = ((j * N + i) % 1000) as f64 / 1000.0;
        }
    }

    // Multiply C = A * B
    for i in 0..N {
        for j in 0..N {
            let mut sum = 0.0;
            for k in 0..N {
                sum += a[i][k] * b[k][j];
            }
            c[i][j] = sum;
        }
    }

    // Checksum
    let mut checksum = 0.0;
    for i in 0..N {
        for j in 0..N {
            checksum += c[i][j];
        }
    }

    println!("matrix_mult(200): checksum = {:.6}", checksum);
}
