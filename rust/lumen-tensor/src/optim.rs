use crate::tensor::Tensor;

/// Trait for all optimizers.
pub trait Optimizer {
    /// Perform one optimization step given parameters and their gradients.
    ///
    /// `params` and `grads` must have the same length; each `grads[i]` is the
    /// gradient of the loss with respect to `params[i]` and they must share
    /// the same shape.
    fn step(&mut self, params: &mut [Tensor], grads: &[Tensor]);

    /// Reset optimizer state (momentum buffers, moment estimates, etc.).
    fn zero_state(&mut self);
}

// ── SGD ─────────────────────────────────────────────────────────────────

/// Stochastic Gradient Descent with optional momentum.
///
/// Update rule (no momentum):  `param -= lr * grad`
/// Update rule (momentum):     `v = momentum * v + grad; param -= lr * v`
pub struct SGD {
    learning_rate: f64,
    momentum: f64,
    velocities: Vec<Tensor>,
}

impl SGD {
    /// Create a new SGD optimizer.
    ///
    /// * `learning_rate` — step size (e.g. 0.01)
    /// * `momentum` — momentum factor; 0.0 disables momentum
    pub fn new(learning_rate: f64, momentum: f64) -> Self {
        SGD {
            learning_rate,
            momentum,
            velocities: Vec::new(),
        }
    }

    /// Convenience constructor with no momentum.
    pub fn basic(learning_rate: f64) -> Self {
        Self::new(learning_rate, 0.0)
    }
}

impl Optimizer for SGD {
    fn step(&mut self, params: &mut [Tensor], grads: &[Tensor]) {
        assert_eq!(
            params.len(),
            grads.len(),
            "SGD::step: params and grads must have the same length"
        );

        // Lazily initialise velocity buffers on the first call.
        if self.velocities.is_empty() && self.momentum != 0.0 {
            self.velocities = params
                .iter()
                .map(|p| Tensor::zeros(p.shape().clone()))
                .collect();
        }

        let lr = self.learning_rate;
        let mom = self.momentum;

        for (i, (param, grad)) in params.iter_mut().zip(grads.iter()).enumerate() {
            assert_eq!(
                param.shape(),
                grad.shape(),
                "SGD::step: param and grad shapes must match"
            );

            if mom == 0.0 {
                // Basic SGD: param -= lr * grad
                let pd = param.data_mut();
                let gd = grad.data();
                for (p, &g) in pd.iter_mut().zip(gd.iter()) {
                    *p -= lr * g;
                }
            } else {
                // Momentum SGD: v = mom * v + grad; param -= lr * v
                let vd = self.velocities[i].data_mut();
                let gd = grad.data();
                for (v, &g) in vd.iter_mut().zip(gd.iter()) {
                    *v = mom * (*v) + g;
                }
                let pd = param.data_mut();
                let vd = self.velocities[i].data();
                for (p, &v) in pd.iter_mut().zip(vd.iter()) {
                    *p -= lr * v;
                }
            }
        }
    }

    fn zero_state(&mut self) {
        for v in &mut self.velocities {
            let d = v.data_mut();
            for x in d.iter_mut() {
                *x = 0.0;
            }
        }
    }
}

// ── Adam ────────────────────────────────────────────────────────────────

/// Adam optimizer (Kingma & Ba, 2014).
///
/// Maintains per-parameter running estimates of the first moment (mean) and
/// second moment (uncentred variance) of the gradients, with bias correction.
pub struct Adam {
    learning_rate: f64,
    beta1: f64,
    beta2: f64,
    epsilon: f64,
    step_count: u64,
    m: Vec<Tensor>, // first moment estimates
    v: Vec<Tensor>, // second moment estimates
}

impl Adam {
    /// Create a new Adam optimizer with explicit hyper-parameters.
    pub fn new(learning_rate: f64, beta1: f64, beta2: f64, epsilon: f64) -> Self {
        Adam {
            learning_rate,
            beta1,
            beta2,
            epsilon,
            step_count: 0,
            m: Vec::new(),
            v: Vec::new(),
        }
    }

    /// Create an Adam optimizer with the paper defaults (β₁=0.9, β₂=0.999, ε=1e-8).
    pub fn default_with_lr(learning_rate: f64) -> Self {
        Self::new(learning_rate, 0.9, 0.999, 1e-8)
    }

    /// Return the number of steps taken so far.
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Read-only access to first-moment buffers.
    pub fn first_moments(&self) -> &[Tensor] {
        &self.m
    }

    /// Read-only access to second-moment buffers.
    pub fn second_moments(&self) -> &[Tensor] {
        &self.v
    }
}

impl Optimizer for Adam {
    fn step(&mut self, params: &mut [Tensor], grads: &[Tensor]) {
        assert_eq!(
            params.len(),
            grads.len(),
            "Adam::step: params and grads must have the same length"
        );

        // Lazily initialise moment buffers on the first call.
        if self.m.is_empty() {
            self.m = params
                .iter()
                .map(|p| Tensor::zeros(p.shape().clone()))
                .collect();
            self.v = params
                .iter()
                .map(|p| Tensor::zeros(p.shape().clone()))
                .collect();
        }

        self.step_count += 1;
        let t = self.step_count as f64;
        let lr = self.learning_rate;
        let b1 = self.beta1;
        let b2 = self.beta2;
        let eps = self.epsilon;

        // Bias-correction factors.
        let bc1 = 1.0 - b1.powf(t);
        let bc2 = 1.0 - b2.powf(t);

        for (i, (param, grad)) in params.iter_mut().zip(grads.iter()).enumerate() {
            assert_eq!(
                param.shape(),
                grad.shape(),
                "Adam::step: param and grad shapes must match"
            );

            let gd = grad.data();

            // Update biased first moment: m = β₁·m + (1-β₁)·g
            {
                let md = self.m[i].data_mut();
                for (m, &g) in md.iter_mut().zip(gd.iter()) {
                    *m = b1 * (*m) + (1.0 - b1) * g;
                }
            }

            // Update biased second moment: v = β₂·v + (1-β₂)·g²
            {
                let vd = self.v[i].data_mut();
                for (v, &g) in vd.iter_mut().zip(gd.iter()) {
                    *v = b2 * (*v) + (1.0 - b2) * g * g;
                }
            }

            // Apply bias-corrected update:
            //   m̂ = m / (1 - β₁ᵗ)
            //   v̂ = v / (1 - β₂ᵗ)
            //   param -= lr * m̂ / (√v̂ + ε)
            let md = self.m[i].data();
            let vd = self.v[i].data();
            let pd = param.data_mut();
            for j in 0..pd.len() {
                let m_hat = md[j] / bc1;
                let v_hat = vd[j] / bc2;
                pd[j] -= lr * m_hat / (v_hat.sqrt() + eps);
            }
        }
    }

    fn zero_state(&mut self) {
        self.step_count = 0;
        for m in &mut self.m {
            let d = m.data_mut();
            for x in d.iter_mut() {
                *x = 0.0;
            }
        }
        for v in &mut self.v {
            let d = v.data_mut();
            for x in d.iter_mut() {
                *x = 0.0;
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ad::Tape;
    use crate::shape::Shape;

    const EPS: f64 = 1e-6;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn approx_eq_tol(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    // ── SGD basic step on scalar ────────────────────────────────────────

    #[test]
    fn sgd_basic_scalar() {
        // param = 5.0, grad = 2.0, lr = 0.1 -> param = 5 - 0.1*2 = 4.8
        let mut opt = SGD::basic(0.1);
        let mut params = vec![Tensor::scalar(5.0)];
        let grads = vec![Tensor::scalar(2.0)];
        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 4.8));
    }

    // ── SGD basic step on vector ────────────────────────────────────────

    #[test]
    fn sgd_basic_vector() {
        // params = [1, 2, 3], grads = [0.5, 1.0, 1.5], lr = 0.2
        // result = [1-0.1, 2-0.2, 3-0.3] = [0.9, 1.8, 2.7]
        let mut opt = SGD::basic(0.2);
        let mut params = vec![Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap()];
        let grads = vec![Tensor::from_vec(vec![0.5, 1.0, 1.5], Shape::new(vec![3])).unwrap()];
        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 0.9));
        assert!(approx_eq(params[0].data()[1], 1.8));
        assert!(approx_eq(params[0].data()[2], 2.7));
    }

    // ── SGD basic step on matrix ────────────────────────────────────────

    #[test]
    fn sgd_basic_matrix() {
        // 2x2 param, lr = 0.5, grad = ones
        let mut opt = SGD::basic(0.5);
        let mut params =
            vec![Tensor::from_vec(vec![4.0, 3.0, 2.0, 1.0], Shape::new(vec![2, 2])).unwrap()];
        let grads = vec![Tensor::ones(Shape::new(vec![2, 2]))];
        opt.step(&mut params, &grads);
        // each element decreases by 0.5
        assert!(approx_eq(params[0].data()[0], 3.5));
        assert!(approx_eq(params[0].data()[1], 2.5));
        assert!(approx_eq(params[0].data()[2], 1.5));
        assert!(approx_eq(params[0].data()[3], 0.5));
    }

    // ── SGD with momentum ───────────────────────────────────────────────

    #[test]
    fn sgd_momentum() {
        // lr = 0.1, momentum = 0.9
        // Step 1: v = 0.9*0 + 2 = 2,  param = 5 - 0.1*2 = 4.8
        // Step 2: v = 0.9*2 + 2 = 3.8, param = 4.8 - 0.1*3.8 = 4.42
        let mut opt = SGD::new(0.1, 0.9);
        let mut params = vec![Tensor::scalar(5.0)];
        let grads = vec![Tensor::scalar(2.0)];

        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 4.8));

        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 4.42));
    }

    // ── SGD momentum vector ─────────────────────────────────────────────

    #[test]
    fn sgd_momentum_vector() {
        let mut opt = SGD::new(0.1, 0.5);
        let mut params = vec![Tensor::from_vec(vec![10.0, 20.0], Shape::new(vec![2])).unwrap()];
        let grads = vec![Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap()];

        // Step 1: v = 0.5*0 + g = [1,2]; param -= 0.1*v = [9.9, 19.8]
        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 9.9));
        assert!(approx_eq(params[0].data()[1], 19.8));

        // Step 2: v = 0.5*[1,2] + [1,2] = [1.5,3]; param -= 0.1*v = [9.75, 19.5]
        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 9.75));
        assert!(approx_eq(params[0].data()[1], 19.5));
    }

    // ── Adam basic step ─────────────────────────────────────────────────

    #[test]
    fn adam_basic_scalar() {
        let mut opt = Adam::default_with_lr(0.01);
        let mut params = vec![Tensor::scalar(5.0)];
        let grads = vec![Tensor::scalar(1.0)];

        let before = params[0].data()[0];
        opt.step(&mut params, &grads);
        let after = params[0].data()[0];
        // Parameter should decrease (positive gradient)
        assert!(after < before);
        assert_eq!(opt.step_count(), 1);
    }

    // ── Adam basic step on vector ───────────────────────────────────────

    #[test]
    fn adam_basic_vector() {
        let mut opt = Adam::default_with_lr(0.001);
        let mut params = vec![Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap()];
        let grads = vec![Tensor::from_vec(vec![0.1, 0.2, 0.3], Shape::new(vec![3])).unwrap()];

        opt.step(&mut params, &grads);

        // All params should decrease (all grads positive)
        assert!(params[0].data()[0] < 1.0);
        assert!(params[0].data()[1] < 2.0);
        assert!(params[0].data()[2] < 3.0);
    }

    // ── Adam bias correction (first few steps) ─────────────────────────

    #[test]
    fn adam_bias_correction() {
        // With β₁=0.9, β₂=0.999, the first step has large bias correction.
        // m̂ = m/(1-0.9) = 10*m, v̂ = v/(1-0.999) ≈ 1000*v
        // This effectively makes the first step size ≈ lr regardless of
        // gradient magnitude (for a constant gradient).
        let lr = 0.1;
        let mut opt = Adam::new(lr, 0.9, 0.999, 1e-8);
        let mut params = vec![Tensor::scalar(0.0)];
        let grads = vec![Tensor::scalar(1.0)];

        opt.step(&mut params, &grads);

        // After step 1 with g=1:
        //   m = 0.1, v = 0.001
        //   m_hat = 0.1/0.1 = 1.0
        //   v_hat = 0.001/0.001 = 1.0
        //   update = lr * 1.0 / (1.0 + eps) ≈ lr = 0.1
        //   param = 0 - 0.1 ≈ -0.1
        assert!(approx_eq_tol(params[0].data()[0], -lr, 1e-6));
    }

    // ── Adam bias correction evolves over steps ─────────────────────────

    #[test]
    fn adam_bias_correction_evolves() {
        // Verify that step_count increments and moments accumulate.
        let mut opt = Adam::default_with_lr(0.01);
        let mut params = vec![Tensor::scalar(10.0)];
        let grads = vec![Tensor::scalar(1.0)];

        opt.step(&mut params, &grads);
        assert_eq!(opt.step_count(), 1);
        let m1 = opt.first_moments()[0].data()[0];
        let v1 = opt.second_moments()[0].data()[0];

        opt.step(&mut params, &grads);
        assert_eq!(opt.step_count(), 2);
        let m2 = opt.first_moments()[0].data()[0];
        let v2 = opt.second_moments()[0].data()[0];

        // Moments should grow with more gradient signal
        assert!(m2 > m1);
        assert!(v2 > v1);
    }

    // ── Multiple steps converge loss toward minimum ─────────────────────

    #[test]
    fn sgd_converges_quadratic() {
        // Minimise f(x) = x^2. Gradient = 2x. Minimum at x=0.
        let mut opt = SGD::basic(0.1);
        let mut params = vec![Tensor::scalar(5.0)];

        for _ in 0..100 {
            let x = params[0].data()[0];
            let grad = Tensor::scalar(2.0 * x); // df/dx = 2x
            opt.step(&mut params, &[grad]);
        }

        // Should be very close to 0
        assert!(params[0].data()[0].abs() < 1e-4);
    }

    #[test]
    fn adam_converges_quadratic() {
        // Minimise f(x) = x^2. Gradient = 2x.
        let mut opt = Adam::default_with_lr(0.1);
        let mut params = vec![Tensor::scalar(5.0)];

        for _ in 0..200 {
            let x = params[0].data()[0];
            let grad = Tensor::scalar(2.0 * x);
            opt.step(&mut params, &[grad]);
        }

        assert!(params[0].data()[0].abs() < 0.05);
    }

    // ── zero_state resets momentum buffers ───────────────────────────────

    #[test]
    fn sgd_zero_state_resets() {
        let mut opt = SGD::new(0.1, 0.9);
        let mut params = vec![Tensor::scalar(5.0)];
        let grads = vec![Tensor::scalar(2.0)];

        // Take a step to populate velocity buffers
        opt.step(&mut params, &grads);
        assert!(opt
            .velocities
            .iter()
            .any(|v| v.data().iter().any(|&x| x != 0.0)));

        opt.zero_state();

        // Velocities should be all zeros
        for v in &opt.velocities {
            assert!(v.data().iter().all(|&x| x == 0.0));
        }
    }

    #[test]
    fn adam_zero_state_resets() {
        let mut opt = Adam::default_with_lr(0.01);
        let mut params = vec![Tensor::scalar(1.0)];
        let grads = vec![Tensor::scalar(0.5)];

        opt.step(&mut params, &grads);
        assert_eq!(opt.step_count(), 1);

        opt.zero_state();

        assert_eq!(opt.step_count(), 0);
        for m in opt.first_moments() {
            assert!(m.data().iter().all(|&x| x == 0.0));
        }
        for v in opt.second_moments() {
            assert!(v.data().iter().all(|&x| x == 0.0));
        }
    }

    // ── Different learning rates ────────────────────────────────────────

    #[test]
    fn sgd_different_learning_rates() {
        // Larger lr should give a larger step
        let mut opt_small = SGD::basic(0.01);
        let mut opt_large = SGD::basic(1.0);

        let mut p_small = vec![Tensor::scalar(10.0)];
        let mut p_large = vec![Tensor::scalar(10.0)];
        let grads = vec![Tensor::scalar(1.0)];

        opt_small.step(&mut p_small, &grads);
        opt_large.step(&mut p_large, &grads);

        // Smaller lr -> closer to original
        assert!(p_small[0].data()[0] > p_large[0].data()[0]);
        assert!(approx_eq(p_small[0].data()[0], 9.99));
        assert!(approx_eq(p_large[0].data()[0], 9.0));
    }

    #[test]
    fn adam_different_learning_rates() {
        let mut opt_small = Adam::default_with_lr(0.001);
        let mut opt_large = Adam::default_with_lr(0.5);

        let mut p_small = vec![Tensor::scalar(10.0)];
        let mut p_large = vec![Tensor::scalar(10.0)];
        let grads = vec![Tensor::scalar(1.0)];

        opt_small.step(&mut p_small, &grads);
        opt_large.step(&mut p_large, &grads);

        // Larger lr -> param moved further
        let delta_small = (10.0 - p_small[0].data()[0]).abs();
        let delta_large = (10.0 - p_large[0].data()[0]).abs();
        assert!(delta_large > delta_small);
    }

    // ── AD tape gradient + optimizer step ───────────────────────────────

    #[test]
    fn ad_quadratic_sgd_step() {
        // f(x) = x^2, compute gradient via AD tape, then take an SGD step.
        let x_val = 3.0;
        let mut tape = Tape::new();
        let x = tape.var(Tensor::scalar(x_val));
        let x2 = tape.mul(x, x);
        let grads = tape.backward(x2);
        // grad = 2*3 = 6
        assert!(approx_eq(grads[x.0].data()[0], 6.0));

        let mut opt = SGD::basic(0.1);
        let mut params = vec![Tensor::scalar(x_val)];
        opt.step(&mut params, &[grads[x.0].clone()]);
        // x = 3 - 0.1 * 6 = 2.4
        assert!(approx_eq(params[0].data()[0], 2.4));
    }

    #[test]
    fn ad_quadratic_adam_step() {
        // f(x) = x^2, compute gradient via AD tape, then take an Adam step.
        let x_val = 3.0;
        let mut tape = Tape::new();
        let x = tape.var(Tensor::scalar(x_val));
        let x2 = tape.mul(x, x);
        let grads = tape.backward(x2);

        let mut opt = Adam::default_with_lr(0.1);
        let mut params = vec![Tensor::scalar(x_val)];
        opt.step(&mut params, &[grads[x.0].clone()]);
        // After one Adam step, param should decrease
        assert!(params[0].data()[0] < x_val);
    }

    // ── AD tape gradient on vector + optimizer ──────────────────────────

    #[test]
    fn ad_vector_loss_sgd_step() {
        // f(w) = sum(w * w) = sum([w1^2, w2^2])
        // grad = [2*w1, 2*w2]
        let mut tape = Tape::new();
        let w = tape.var(Tensor::from_vec(vec![3.0, 4.0], Shape::new(vec![2])).unwrap());
        let w2 = tape.mul(w, w);
        let loss = tape.sum(w2);
        let grads = tape.backward(loss);

        // grad = [6, 8]
        assert!(approx_eq(grads[w.0].data()[0], 6.0));
        assert!(approx_eq(grads[w.0].data()[1], 8.0));

        let mut opt = SGD::basic(0.01);
        let mut params = vec![Tensor::from_vec(vec![3.0, 4.0], Shape::new(vec![2])).unwrap()];
        opt.step(&mut params, &[grads[w.0].clone()]);
        // w = [3 - 0.06, 4 - 0.08] = [2.94, 3.92]
        assert!(approx_eq(params[0].data()[0], 2.94));
        assert!(approx_eq(params[0].data()[1], 3.92));
    }

    // ── Multiple parameters ─────────────────────────────────────────────

    #[test]
    fn sgd_multiple_params() {
        let mut opt = SGD::basic(0.1);
        let mut params = vec![
            Tensor::scalar(10.0),
            Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap(),
        ];
        let grads = vec![
            Tensor::scalar(1.0),
            Tensor::from_vec(vec![0.5, -0.5], Shape::new(vec![2])).unwrap(),
        ];

        opt.step(&mut params, &grads);
        assert!(approx_eq(params[0].data()[0], 9.9));
        assert!(approx_eq(params[1].data()[0], 0.95));
        assert!(approx_eq(params[1].data()[1], 2.05));
    }

    #[test]
    fn adam_multiple_params() {
        let mut opt = Adam::default_with_lr(0.01);
        let mut params = vec![
            Tensor::scalar(5.0),
            Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap(),
        ];
        let grads = vec![
            Tensor::scalar(1.0),
            Tensor::from_vec(vec![0.1, 0.2], Shape::new(vec![2])).unwrap(),
        ];

        let p0_before = params[0].data()[0];
        let p1_0_before = params[1].data()[0];
        let p1_1_before = params[1].data()[1];

        opt.step(&mut params, &grads);

        assert!(params[0].data()[0] < p0_before);
        assert!(params[1].data()[0] < p1_0_before);
        assert!(params[1].data()[1] < p1_1_before);
    }
}
