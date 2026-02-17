//! Neural network building blocks.
//!
//! Provides high-level primitives for constructing and training neural networks:
//! layers (Linear), activation functions (ReLU, sigmoid, softmax), and loss
//! functions (cross-entropy, MSE).

use crate::ops::{self, OpError};
use crate::shape::Shape;
use crate::tensor::Tensor;

// ── Layer trait ─────────────────────────────────────────────────────────

/// Trait for neural network layers.
///
/// Each layer computes a forward pass and exposes its learnable parameters
/// for optimizers.
pub trait Layer {
    /// Compute the forward pass given an input tensor.
    fn forward(&self, input: &Tensor) -> Tensor;

    /// Return references to all learnable parameters.
    fn params(&self) -> Vec<&Tensor>;
}

// ── Linear layer ────────────────────────────────────────────────────────

/// Fully-connected linear layer: `y = x @ W^T + b`.
///
/// Weight shape: `(out_features, in_features)` — stored transposed for
/// efficient `x @ W^T` computation.
/// Bias shape: `(out_features,)`.
pub struct Linear {
    /// Weight matrix of shape `(out_features, in_features)`.
    weight: Tensor,
    /// Bias vector of shape `(out_features,)`.
    bias: Tensor,
}

impl Linear {
    /// Create a new linear layer with the given dimensions.
    ///
    /// Weights are initialised from `Tensor::randn` (N(0,1)) scaled by
    /// `1/sqrt(in_features)` (Kaiming-style), and biases are initialised to
    /// zero.
    pub fn new(in_features: usize, out_features: usize) -> Self {
        // Kaiming uniform-ish initialisation: scale randn by 1/sqrt(fan_in).
        let scale = 1.0 / (in_features as f64).sqrt();
        let mut weight = Tensor::randn(Shape::new(vec![out_features, in_features]));
        for v in weight.data_mut().iter_mut() {
            *v *= scale;
        }
        let bias = Tensor::zeros(Shape::new(vec![out_features]));
        Linear { weight, bias }
    }

    /// Create a linear layer from explicit weight and bias tensors.
    ///
    /// # Panics
    ///
    /// Panics if weight is not 2-D or bias is not 1-D with matching `out_features`.
    pub fn from_tensors(weight: Tensor, bias: Tensor) -> Self {
        assert_eq!(weight.ndim(), 2, "Linear::from_tensors: weight must be 2-D");
        assert_eq!(bias.ndim(), 1, "Linear::from_tensors: bias must be 1-D");
        assert_eq!(
            weight.shape().dims()[0],
            bias.shape().dims()[0],
            "Linear::from_tensors: weight rows must equal bias length"
        );
        Linear { weight, bias }
    }

    /// Return a reference to the weight tensor.
    pub fn weight(&self) -> &Tensor {
        &self.weight
    }

    /// Return a mutable reference to the weight tensor (for optimizers).
    pub fn weight_mut(&mut self) -> &mut Tensor {
        &mut self.weight
    }

    /// Return a reference to the bias tensor.
    pub fn bias(&self) -> &Tensor {
        &self.bias
    }

    /// Return a mutable reference to the bias tensor (for optimizers).
    pub fn bias_mut(&mut self) -> &mut Tensor {
        &mut self.bias
    }

    /// Return `in_features` (number of input features).
    pub fn in_features(&self) -> usize {
        self.weight.shape().dims()[1]
    }

    /// Return `out_features` (number of output features).
    pub fn out_features(&self) -> usize {
        self.weight.shape().dims()[0]
    }
}

impl Layer for Linear {
    /// Compute `y = x @ W^T + b`.
    ///
    /// Supports both single-sample (`(in_features,)` → `(out_features,)`) and
    /// batched (`(batch, in_features)` → `(batch, out_features)`) inputs.
    fn forward(&self, input: &Tensor) -> Tensor {
        // W^T: (in_features, out_features)
        let wt = ops::transpose(&self.weight).expect("Linear::forward: transpose failed");
        // x @ W^T
        let out = ops::matmul(input, &wt).expect("Linear::forward: matmul failed");
        // + bias (broadcasts over batch dimension)
        ops::add(&out, &self.bias).expect("Linear::forward: bias add failed")
    }

    fn params(&self) -> Vec<&Tensor> {
        vec![&self.weight, &self.bias]
    }
}

// ── Activation functions ────────────────────────────────────────────────

/// Element-wise ReLU activation: `max(0, x)`.
pub fn relu(tensor: &Tensor) -> Tensor {
    ops::relu(tensor)
}

/// Element-wise sigmoid activation: `1 / (1 + exp(-x))`.
pub fn sigmoid(tensor: &Tensor) -> Tensor {
    ops::sigmoid(tensor)
}

/// Softmax along the given axis.
///
/// For numerical stability, subtracts the per-row max before exponentiating.
///
/// # Panics
///
/// Panics if `axis` is out of bounds for the tensor's dimensions.
pub fn softmax(tensor: &Tensor, axis: usize) -> Tensor {
    let dims = tensor.shape().dims();
    assert!(
        axis < dims.len(),
        "softmax: axis {} out of bounds for {}D tensor",
        axis,
        dims.len()
    );

    let data = tensor.data();
    let shape = tensor.shape().clone();
    let n = tensor.numel();
    let strides = tensor.strides();

    let axis_size = dims[axis];

    // Compute the "stride" along the softmax axis and the total number of
    // independent softmax lanes.
    let axis_stride = strides[axis];
    let num_lanes = n / axis_size;

    let mut result = vec![0.0f64; n];

    // For each independent lane (all indices except the softmax axis):
    for lane in 0..num_lanes {
        // Convert lane index to a multi-dim index with axis=0.
        // We iterate over the flat index space skipping the axis dimension.
        let mut multi_idx = vec![0usize; dims.len()];
        let mut rem = lane;
        for d in (0..dims.len()).rev() {
            if d == axis {
                continue;
            }
            multi_idx[d] = rem % dims[d];
            rem /= dims[d];
        }

        // Compute flat offsets for each position along the axis.
        let base_flat: usize = multi_idx
            .iter()
            .zip(strides.iter())
            .map(|(&i, &s)| i * s)
            .sum();

        // Find max for numerical stability.
        let mut max_val = f64::NEG_INFINITY;
        for a in 0..axis_size {
            let idx = base_flat + a * axis_stride;
            if data[idx] > max_val {
                max_val = data[idx];
            }
        }

        // Compute exp(x - max) and sum.
        let mut sum_exp = 0.0f64;
        for a in 0..axis_size {
            let idx = base_flat + a * axis_stride;
            let e = (data[idx] - max_val).exp();
            result[idx] = e;
            sum_exp += e;
        }

        // Normalize.
        for a in 0..axis_size {
            let idx = base_flat + a * axis_stride;
            result[idx] /= sum_exp;
        }
    }

    Tensor::from_vec(result, shape).unwrap()
}

// ── Loss functions ──────────────────────────────────────────────────────

/// Cross-entropy loss between predicted log-probabilities and target indices.
///
/// `predictions` should be raw logits of shape `(batch, num_classes)` or
/// `(num_classes,)`. `targets` should contain class indices as floats with
/// shape `(batch,)` or scalar.
///
/// Computes: `-mean(log(softmax(predictions))[target_class])`.
///
/// Returns a scalar tensor.
pub fn cross_entropy_loss(predictions: &Tensor, targets: &Tensor) -> Result<Tensor, OpError> {
    match predictions.ndim() {
        1 => {
            // Single sample: predictions = (num_classes,), targets = scalar
            let probs = softmax(predictions, 0);
            let target_idx = targets.data()[0] as usize;
            let log_prob = probs.data()[target_idx].ln();
            Ok(Tensor::scalar(-log_prob))
        }
        2 => {
            // Batched: predictions = (batch, num_classes), targets = (batch,)
            let batch_size = predictions.shape().dims()[0];
            let num_classes = predictions.shape().dims()[1];
            let probs = softmax(predictions, 1);

            let mut total_loss = 0.0f64;
            for b in 0..batch_size {
                let target_idx = targets.data()[b] as usize;
                if target_idx >= num_classes {
                    return Err(OpError::InvalidOperation(format!(
                        "cross_entropy_loss: target index {} >= num_classes {}",
                        target_idx, num_classes
                    )));
                }
                let prob = probs.data()[b * num_classes + target_idx];
                total_loss -= prob.max(1e-15).ln();
            }
            Ok(Tensor::scalar(total_loss / batch_size as f64))
        }
        _ => Err(OpError::InvalidOperation(format!(
            "cross_entropy_loss: predictions must be 1D or 2D, got {}D",
            predictions.ndim()
        ))),
    }
}

/// Mean squared error loss: `mean((predictions - targets)^2)`.
///
/// `predictions` and `targets` must have the same shape.
///
/// Returns a scalar tensor.
pub fn mse_loss(predictions: &Tensor, targets: &Tensor) -> Result<Tensor, OpError> {
    let diff = ops::sub(predictions, targets)?;
    let sq = ops::mul(&diff, &diff)?;
    Ok(ops::mean(&sq))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    // ── Linear layer ────────────────────────────────────────────────────

    #[test]
    fn linear_creation() {
        let layer = Linear::new(4, 3);
        assert_eq!(layer.in_features(), 4);
        assert_eq!(layer.out_features(), 3);
        assert_eq!(layer.weight().shape(), &Shape::new(vec![3, 4]));
        assert_eq!(layer.bias().shape(), &Shape::new(vec![3]));
    }

    #[test]
    fn linear_from_tensors() {
        let w =
            Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], Shape::new(vec![2, 3])).unwrap();
        let b = Tensor::from_vec(vec![0.1, 0.2], Shape::new(vec![2])).unwrap();
        let layer = Linear::from_tensors(w, b);
        assert_eq!(layer.in_features(), 3);
        assert_eq!(layer.out_features(), 2);
    }

    #[test]
    fn linear_forward_single() {
        // W = [[1, 0], [0, 1]], b = [1, 2]
        // x = [3, 4]
        // y = x @ W^T + b = [3*1+4*0+1, 3*0+4*1+2] = [4, 6]
        let w = Tensor::from_vec(vec![1.0, 0.0, 0.0, 1.0], Shape::new(vec![2, 2])).unwrap();
        let b = Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap();
        let layer = Linear::from_tensors(w, b);

        let x = Tensor::from_vec(vec![3.0, 4.0], Shape::new(vec![2])).unwrap();
        let y = layer.forward(&x);
        assert_eq!(y.shape(), &Shape::new(vec![2]));
        assert!(approx_eq(y.data()[0], 4.0));
        assert!(approx_eq(y.data()[1], 6.0));
    }

    #[test]
    fn linear_forward_batched() {
        // W = [[1, 2], [3, 4]], b = [0, 0]
        // x = [[1, 0], [0, 1]]  (batch=2)
        // y = x @ W^T = [[1*1+0*2, 1*3+0*4], [0*1+1*2, 0*3+1*4]] = [[1, 3], [2, 4]]
        let w = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], Shape::new(vec![2, 2])).unwrap();
        let b = Tensor::zeros(Shape::new(vec![2]));
        let layer = Linear::from_tensors(w, b);

        let x = Tensor::from_vec(vec![1.0, 0.0, 0.0, 1.0], Shape::new(vec![2, 2])).unwrap();
        let y = layer.forward(&x);
        assert_eq!(y.shape(), &Shape::new(vec![2, 2]));
        assert!(approx_eq(y.data()[0], 1.0));
        assert!(approx_eq(y.data()[1], 3.0));
        assert!(approx_eq(y.data()[2], 2.0));
        assert!(approx_eq(y.data()[3], 4.0));
    }

    #[test]
    fn linear_params() {
        let layer = Linear::new(3, 2);
        let p = layer.params();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].shape(), &Shape::new(vec![2, 3])); // weight
        assert_eq!(p[1].shape(), &Shape::new(vec![2])); // bias
    }

    #[test]
    fn linear_mut_accessors() {
        let mut layer = Linear::new(2, 2);
        // Should be able to modify weight and bias for optimizer step
        layer.weight_mut().data_mut()[0] = 99.0;
        assert!(approx_eq(layer.weight().data()[0], 99.0));
        layer.bias_mut().data_mut()[0] = 42.0;
        assert!(approx_eq(layer.bias().data()[0], 42.0));
    }

    // ── ReLU ────────────────────────────────────────────────────────────

    #[test]
    fn relu_positive_and_negative() {
        let x = Tensor::from_vec(vec![-2.0, -1.0, 0.0, 1.0, 2.0], Shape::new(vec![5])).unwrap();
        let y = relu(&x);
        assert!(approx_eq(y.data()[0], 0.0));
        assert!(approx_eq(y.data()[1], 0.0));
        assert!(approx_eq(y.data()[2], 0.0));
        assert!(approx_eq(y.data()[3], 1.0));
        assert!(approx_eq(y.data()[4], 2.0));
    }

    // ── Sigmoid ─────────────────────────────────────────────────────────

    #[test]
    fn sigmoid_values() {
        let x = Tensor::from_vec(vec![0.0, 100.0, -100.0], Shape::new(vec![3])).unwrap();
        let y = sigmoid(&x);
        assert!(approx_eq(y.data()[0], 0.5));
        assert!((y.data()[1] - 1.0).abs() < 1e-10); // sigmoid(100) ≈ 1
        assert!(y.data()[2].abs() < 1e-10); // sigmoid(-100) ≈ 0
    }

    // ── Softmax ─────────────────────────────────────────────────────────

    #[test]
    fn softmax_1d() {
        let x = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
        let y = softmax(&x, 0);
        // Should sum to 1
        let sum: f64 = y.data().iter().sum();
        assert!(approx_eq(sum, 1.0));
        // Should be monotonically increasing
        assert!(y.data()[0] < y.data()[1]);
        assert!(y.data()[1] < y.data()[2]);
    }

    #[test]
    fn softmax_2d_axis1() {
        // 2x3 matrix, softmax along axis 1 (columns within each row)
        let x =
            Tensor::from_vec(vec![1.0, 2.0, 3.0, 1.0, 1.0, 1.0], Shape::new(vec![2, 3])).unwrap();
        let y = softmax(&x, 1);
        assert_eq!(y.shape(), &Shape::new(vec![2, 3]));

        // Row 0 should sum to 1
        let row0_sum: f64 = y.data()[0..3].iter().sum();
        assert!(approx_eq(row0_sum, 1.0));

        // Row 1 should sum to 1
        let row1_sum: f64 = y.data()[3..6].iter().sum();
        assert!(approx_eq(row1_sum, 1.0));

        // Row 1 has equal inputs, so equal outputs (1/3 each)
        assert!(approx_eq(y.data()[3], 1.0 / 3.0));
        assert!(approx_eq(y.data()[4], 1.0 / 3.0));
        assert!(approx_eq(y.data()[5], 1.0 / 3.0));
    }

    #[test]
    fn softmax_numerical_stability() {
        // Large values shouldn't cause overflow
        let x = Tensor::from_vec(vec![1000.0, 1001.0, 1002.0], Shape::new(vec![3])).unwrap();
        let y = softmax(&x, 0);
        let sum: f64 = y.data().iter().sum();
        assert!(approx_eq(sum, 1.0));
        // All values should be finite
        assert!(y.data().iter().all(|v| v.is_finite()));
    }

    // ── Cross-entropy loss ──────────────────────────────────────────────

    #[test]
    fn cross_entropy_single_sample() {
        // Perfect prediction should give low loss
        let logits = Tensor::from_vec(vec![10.0, 0.0, 0.0], Shape::new(vec![3])).unwrap();
        let target = Tensor::scalar(0.0); // class 0
        let loss = cross_entropy_loss(&logits, &target).unwrap();
        assert!(loss.data()[0] < 0.01); // should be near 0
    }

    #[test]
    fn cross_entropy_wrong_prediction() {
        // Confident wrong prediction should give high loss
        let logits = Tensor::from_vec(vec![0.0, 0.0, 10.0], Shape::new(vec![3])).unwrap();
        let target = Tensor::scalar(0.0); // class 0, but model predicts class 2
        let loss = cross_entropy_loss(&logits, &target).unwrap();
        assert!(loss.data()[0] > 5.0); // should be large
    }

    #[test]
    fn cross_entropy_batched() {
        // Batch of 2 samples, 3 classes
        let logits =
            Tensor::from_vec(vec![10.0, 0.0, 0.0, 0.0, 0.0, 10.0], Shape::new(vec![2, 3])).unwrap();
        // targets: sample 0 = class 0 (correct), sample 1 = class 2 (correct)
        let targets = Tensor::from_vec(vec![0.0, 2.0], Shape::new(vec![2])).unwrap();
        let loss = cross_entropy_loss(&logits, &targets).unwrap();
        assert!(loss.data()[0] < 0.01); // both predictions correct
    }

    // ── MSE loss ────────────────────────────────────────────────────────

    #[test]
    fn mse_loss_zero() {
        let pred = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
        let target = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
        let loss = mse_loss(&pred, &target).unwrap();
        assert!(approx_eq(loss.data()[0], 0.0));
    }

    #[test]
    fn mse_loss_basic() {
        // pred = [1, 2, 3], target = [4, 5, 6]
        // diff = [-3, -3, -3], sq = [9, 9, 9], mean = 9
        let pred = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
        let target = Tensor::from_vec(vec![4.0, 5.0, 6.0], Shape::new(vec![3])).unwrap();
        let loss = mse_loss(&pred, &target).unwrap();
        assert!(approx_eq(loss.data()[0], 9.0));
    }

    #[test]
    fn mse_loss_scalar() {
        let pred = Tensor::scalar(3.0);
        let target = Tensor::scalar(5.0);
        let loss = mse_loss(&pred, &target).unwrap();
        assert!(approx_eq(loss.data()[0], 4.0)); // (3-5)^2 = 4
    }

    // ── Layer trait ─────────────────────────────────────────────────────

    #[test]
    fn layer_trait_dispatch() {
        // Verify that Linear can be used as a dyn Layer
        let w = Tensor::from_vec(vec![1.0, 0.0, 0.0, 1.0], Shape::new(vec![2, 2])).unwrap();
        let b = Tensor::zeros(Shape::new(vec![2]));
        let layer: Box<dyn Layer> = Box::new(Linear::from_tensors(w, b));

        let x = Tensor::from_vec(vec![5.0, 7.0], Shape::new(vec![2])).unwrap();
        let y = layer.forward(&x);
        assert!(approx_eq(y.data()[0], 5.0));
        assert!(approx_eq(y.data()[1], 7.0));

        assert_eq!(layer.params().len(), 2);
    }
}
