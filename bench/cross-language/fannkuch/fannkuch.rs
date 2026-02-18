// Fannkuch-Redux benchmark, N=10

const N: usize = 10;

fn main() {
    let mut perm = [0usize; N];
    let mut perm1 = [0usize; N];
    let mut count = [0usize; N];
    let mut max_flips = 0;
    let mut checksum: i32 = 0;
    let mut r = N;
    let mut perm_count: usize = 0;

    for i in 0..N {
        perm1[i] = i;
    }

    'outer: loop {
        while r > 1 {
            count[r - 1] = r;
            r -= 1;
        }

        perm = perm1;

        // Count flips
        let mut flips = 0;
        let mut k = perm[0];
        while k != 0 {
            // Reverse first k+1 elements
            perm[..=k].reverse();
            flips += 1;
            k = perm[0];
        }

        if flips > max_flips {
            max_flips = flips;
        }
        if perm_count % 2 == 0 {
            checksum += flips;
        } else {
            checksum -= flips;
        }
        perm_count += 1;

        // Next permutation
        loop {
            if r == N {
                break 'outer;
            }
            let p0 = perm1[0];
            for i in 0..r {
                perm1[i] = perm1[i + 1];
            }
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
    }

    println!("{}", checksum);
    println!("Pfannkuchen({}) = {}", N, max_flips);
}
