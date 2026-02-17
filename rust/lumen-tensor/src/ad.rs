use crate::ops;
use crate::shape::Shape;
use crate::tensor::Tensor;

/// Index into the tape's value/entry arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapeIndex(pub usize);

/// Recorded operations for automatic differentiation.
#[derive(Debug, Clone)]
pub enum Op {
    Leaf,
    Add,
    Sub,
    Mul,
    Div,
    MatMul,
    Exp,
    Log,
    Relu,
    Sigmoid,
    Tanh,
    Sum,
    Neg,
    Transpose,
}

/// A node in the computation graph.
#[derive(Debug)]
struct TapeEntry {
    /// Which operation produced this value.
    op: Op,
    /// Indices of input tensors in the tape.
    inputs: Vec<usize>,
    /// The shape of the output tensor.
    output_shape: Shape,
}

/// Tape-based reverse-mode automatic differentiation.
///
/// Records a computation graph as operations are applied, then computes
/// gradients via reverse accumulation (backpropagation).
pub struct Tape {
    entries: Vec<TapeEntry>,
    values: Vec<Tensor>,
}

impl Tape {
    /// Create an empty tape.
    pub fn new() -> Self {
        Tape {
            entries: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Register an input variable (leaf node) on the tape.
    pub fn var(&mut self, tensor: Tensor) -> TapeIndex {
        let idx = self.entries.len();
        let shape = tensor.shape().clone();
        self.entries.push(TapeEntry {
            op: Op::Leaf,
            inputs: vec![],
            output_shape: shape,
        });
        self.values.push(tensor);
        TapeIndex(idx)
    }

    /// Return a reference to the tensor value at the given tape index.
    pub fn value(&self, idx: TapeIndex) -> &Tensor {
        &self.values[idx.0]
    }

    // ── Binary operations ───────────────────────────────────────────────

    /// Record element-wise addition.
    pub fn add(&mut self, a: TapeIndex, b: TapeIndex) -> TapeIndex {
        let result =
            ops::add(&self.values[a.0], &self.values[b.0]).expect("ad::add: broadcast failure");
        self.push_entry(Op::Add, vec![a.0, b.0], result)
    }

    /// Record element-wise subtraction.
    pub fn sub(&mut self, a: TapeIndex, b: TapeIndex) -> TapeIndex {
        let result =
            ops::sub(&self.values[a.0], &self.values[b.0]).expect("ad::sub: broadcast failure");
        self.push_entry(Op::Sub, vec![a.0, b.0], result)
    }

    /// Record element-wise multiplication.
    pub fn mul(&mut self, a: TapeIndex, b: TapeIndex) -> TapeIndex {
        let result =
            ops::mul(&self.values[a.0], &self.values[b.0]).expect("ad::mul: broadcast failure");
        self.push_entry(Op::Mul, vec![a.0, b.0], result)
    }

    /// Record element-wise division.
    pub fn div(&mut self, a: TapeIndex, b: TapeIndex) -> TapeIndex {
        let result =
            ops::div(&self.values[a.0], &self.values[b.0]).expect("ad::div: broadcast failure");
        self.push_entry(Op::Div, vec![a.0, b.0], result)
    }

    /// Record matrix multiplication.
    pub fn matmul(&mut self, a: TapeIndex, b: TapeIndex) -> TapeIndex {
        let result =
            ops::matmul(&self.values[a.0], &self.values[b.0]).expect("ad::matmul: shape error");
        self.push_entry(Op::MatMul, vec![a.0, b.0], result)
    }

    // ── Unary operations ────────────────────────────────────────────────

    /// Record element-wise exponential.
    pub fn exp(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::exp(&self.values[a.0]);
        self.push_entry(Op::Exp, vec![a.0], result)
    }

    /// Record element-wise natural logarithm.
    pub fn log(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::log(&self.values[a.0]);
        self.push_entry(Op::Log, vec![a.0], result)
    }

    /// Record element-wise ReLU.
    pub fn relu(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::relu(&self.values[a.0]);
        self.push_entry(Op::Relu, vec![a.0], result)
    }

    /// Record element-wise sigmoid.
    pub fn sigmoid(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::sigmoid(&self.values[a.0]);
        self.push_entry(Op::Sigmoid, vec![a.0], result)
    }

    /// Record element-wise tanh.
    pub fn tanh(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::tanh(&self.values[a.0]);
        self.push_entry(Op::Tanh, vec![a.0], result)
    }

    /// Record sum reduction.
    pub fn sum(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::sum(&self.values[a.0]);
        self.push_entry(Op::Sum, vec![a.0], result)
    }

    /// Record element-wise negation.
    pub fn neg(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::neg(&self.values[a.0]);
        self.push_entry(Op::Neg, vec![a.0], result)
    }

    /// Record 2-D transpose.
    pub fn transpose(&mut self, a: TapeIndex) -> TapeIndex {
        let result = ops::transpose(&self.values[a.0]).expect("ad::transpose: not 2D");
        self.push_entry(Op::Transpose, vec![a.0], result)
    }

    // ── Backward pass ───────────────────────────────────────────────────

    /// Compute gradients via reverse-mode accumulation.
    ///
    /// `output` is the tape index of the scalar loss whose gradient we
    /// backpropagate. Returns a `Vec<Tensor>` aligned with the tape; the
    /// gradient at index `i` is `d(output)/d(tape[i])`.
    pub fn backward(&self, output: TapeIndex) -> Vec<Tensor> {
        let n = self.entries.len();
        assert!(output.0 < n, "output index out of tape range");

        // Initialize grad accumulator for every tape entry
        let mut grads: Vec<Tensor> = self
            .entries
            .iter()
            .map(|e| Tensor::zeros(e.output_shape.clone()))
            .collect();

        // Seed: d(output)/d(output) = 1
        let out_shape = &self.entries[output.0].output_shape;
        grads[output.0] = Tensor::ones(out_shape.clone());

        // Walk tape in reverse topological order
        for i in (0..n).rev() {
            let entry = &self.entries[i];
            let grad_output = grads[i].clone();

            match &entry.op {
                Op::Leaf => {
                    // No upstream to propagate to.
                }
                Op::Add => {
                    // d/da (a + b) = 1, d/db (a + b) = 1
                    let (ia, ib) = (entry.inputs[0], entry.inputs[1]);
                    accumulate_grad(&mut grads, ia, &grad_output, &self.values[ia]);
                    accumulate_grad(&mut grads, ib, &grad_output, &self.values[ib]);
                }
                Op::Sub => {
                    // d/da (a - b) = 1, d/db (a - b) = -1
                    let (ia, ib) = (entry.inputs[0], entry.inputs[1]);
                    accumulate_grad(&mut grads, ia, &grad_output, &self.values[ia]);
                    let neg_grad = ops::neg(&grad_output);
                    accumulate_grad(&mut grads, ib, &neg_grad, &self.values[ib]);
                }
                Op::Mul => {
                    // d/da (a * b) = b, d/db (a * b) = a
                    let (ia, ib) = (entry.inputs[0], entry.inputs[1]);
                    let grad_a =
                        ops::mul(&grad_output, &self.values[ib]).expect("mul grad broadcast");
                    let grad_b =
                        ops::mul(&grad_output, &self.values[ia]).expect("mul grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                    accumulate_grad(&mut grads, ib, &grad_b, &self.values[ib]);
                }
                Op::Div => {
                    // d/da (a / b) = 1/b, d/db (a / b) = -a / b^2
                    let (ia, ib) = (entry.inputs[0], entry.inputs[1]);
                    let grad_a =
                        ops::div(&grad_output, &self.values[ib]).expect("div grad broadcast");
                    let b_sq = ops::mul(&self.values[ib], &self.values[ib]).expect("div grad b^2");
                    let a_over_bsq = ops::div(&self.values[ia], &b_sq).expect("div grad a/b^2");
                    let neg_a_over_bsq = ops::neg(&a_over_bsq);
                    let grad_b =
                        ops::mul(&grad_output, &neg_a_over_bsq).expect("div grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                    accumulate_grad(&mut grads, ib, &grad_b, &self.values[ib]);
                }
                Op::Neg => {
                    let ia = entry.inputs[0];
                    let grad_a = ops::neg(&grad_output);
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Exp => {
                    // d/da exp(a) = exp(a)
                    let ia = entry.inputs[0];
                    let grad_a =
                        ops::mul(&grad_output, &self.values[i]).expect("exp grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Log => {
                    // d/da log(a) = 1/a
                    let ia = entry.inputs[0];
                    let grad_a =
                        ops::div(&grad_output, &self.values[ia]).expect("log grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Relu => {
                    // d/da relu(a) = 1 if a > 0, else 0
                    let ia = entry.inputs[0];
                    let mask_data: Vec<f64> = self.values[ia]
                        .data()
                        .iter()
                        .map(|&x| if x > 0.0 { 1.0 } else { 0.0 })
                        .collect();
                    let mask =
                        Tensor::from_vec(mask_data, self.values[ia].shape().clone()).unwrap();
                    let grad_a = ops::mul(&grad_output, &mask).expect("relu grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Sigmoid => {
                    // d/da sigmoid(a) = sigmoid(a) * (1 - sigmoid(a))
                    let ia = entry.inputs[0];
                    let sig = &self.values[i]; // output is sigmoid(a)
                    let one = Tensor::ones(sig.shape().clone());
                    let one_minus_sig = ops::sub(&one, sig).expect("sigmoid grad sub");
                    let sig_deriv = ops::mul(sig, &one_minus_sig).expect("sigmoid grad mul");
                    let grad_a =
                        ops::mul(&grad_output, &sig_deriv).expect("sigmoid grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Tanh => {
                    // d/da tanh(a) = 1 - tanh(a)^2
                    let ia = entry.inputs[0];
                    let tanh_val = &self.values[i];
                    let tanh_sq = ops::mul(tanh_val, tanh_val).expect("tanh grad sq");
                    let one = Tensor::ones(tanh_val.shape().clone());
                    let deriv = ops::sub(&one, &tanh_sq).expect("tanh grad sub");
                    let grad_a = ops::mul(&grad_output, &deriv).expect("tanh grad broadcast");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::Sum => {
                    // d/da sum(a) = ones_like(a)  (grad broadcasts to input shape)
                    let ia = entry.inputs[0];
                    let input_shape = self.values[ia].shape().clone();
                    // grad_output is scalar; broadcast to input shape
                    let grad_val = grad_output.data()[0];
                    let grad_a =
                        Tensor::from_vec(vec![grad_val; input_shape.numel()], input_shape).unwrap();
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
                Op::MatMul => {
                    // For C = A @ B:
                    //   dA = grad @ B^T
                    //   dB = A^T @ grad
                    let (ia, ib) = (entry.inputs[0], entry.inputs[1]);
                    let a_val = &self.values[ia];
                    let b_val = &self.values[ib];

                    match (a_val.ndim(), b_val.ndim()) {
                        (2, 2) => {
                            let bt = ops::transpose(b_val).expect("matmul grad transpose B");
                            let at = ops::transpose(a_val).expect("matmul grad transpose A");
                            let grad_a = ops::matmul(&grad_output, &bt).expect("matmul grad A");
                            let grad_b = ops::matmul(&at, &grad_output).expect("matmul grad B");
                            accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                            accumulate_grad(&mut grads, ib, &grad_b, &self.values[ib]);
                        }
                        _ => {
                            // For non-2D matmul, use element-wise approximation
                            // (simplified; full support would need more cases)
                            // Pass through grad as-is for now
                            accumulate_grad(&mut grads, ia, &grad_output, &self.values[ia]);
                            accumulate_grad(&mut grads, ib, &grad_output, &self.values[ib]);
                        }
                    }
                }
                Op::Transpose => {
                    // Transpose of grad
                    let ia = entry.inputs[0];
                    let grad_a = ops::transpose(&grad_output).expect("transpose grad");
                    accumulate_grad(&mut grads, ia, &grad_a, &self.values[ia]);
                }
            }
        }

        grads
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn push_entry(&mut self, op: Op, inputs: Vec<usize>, value: Tensor) -> TapeIndex {
        let idx = self.entries.len();
        let shape = value.shape().clone();
        self.entries.push(TapeEntry {
            op,
            inputs,
            output_shape: shape,
        });
        self.values.push(value);
        TapeIndex(idx)
    }
}

impl Default for Tape {
    fn default() -> Self {
        Self::new()
    }
}

/// Accumulate gradient into the grads array, handling the case where the
/// gradient may need to be reduced (summed) to match the target shape due
/// to broadcasting.
fn accumulate_grad(grads: &mut [Tensor], target_idx: usize, grad: &Tensor, _target_value: &Tensor) {
    let target_shape = grads[target_idx].shape().clone();

    let reduced = reduce_grad_to_shape(grad, &target_shape);

    let current = &grads[target_idx];
    let updated = ops::add(current, &reduced).expect("accumulate_grad: shape mismatch");
    grads[target_idx] = updated;
}

/// Reduce a gradient tensor by summing over dimensions that were broadcast.
fn reduce_grad_to_shape(grad: &Tensor, target_shape: &Shape) -> Tensor {
    if grad.shape() == target_shape {
        return grad.clone();
    }

    // If target is scalar, sum everything
    if target_shape.is_scalar() {
        return ops::sum(grad);
    }

    let grad_dims = grad.shape().dims();
    let target_dims = target_shape.dims();

    // If same number of dims, sum over axes where target_dim == 1
    if grad_dims.len() == target_dims.len() {
        let mut result = grad.clone();
        // Sum from highest dim to lowest so indices stay valid
        for d in (0..target_dims.len()).rev() {
            if target_dims[d] == 1 && grad_dims[d] > 1 {
                result = sum_along_axis(&result, d);
            }
        }
        return result;
    }

    // If grad has more dims, sum over leading axes first
    if grad_dims.len() > target_dims.len() {
        let extra = grad_dims.len() - target_dims.len();
        let mut result = grad.clone();
        for _ in 0..extra {
            result = sum_along_axis(&result, 0);
        }
        // Now same ndim, recursively reduce
        return reduce_grad_to_shape(&result, target_shape);
    }

    // Fallback: if shapes still don't match, just sum to scalar and broadcast
    ops::sum(grad)
}

/// Sum a tensor along a single axis, collapsing that dimension to 1.
fn sum_along_axis(t: &Tensor, axis: usize) -> Tensor {
    let dims = t.shape().dims();
    let ndim = dims.len();
    assert!(axis < ndim);

    let mut new_dims: Vec<usize> = dims.to_vec();
    new_dims[axis] = 1;
    let new_shape = Shape::new(new_dims);
    let new_numel = new_shape.numel();

    let mut data = vec![0.0f64; new_numel];

    let strides = t.strides();
    let axis_size = dims[axis];

    // For each element in the output, accumulate over the axis
    for flat_out in 0..new_numel {
        // Convert flat_out to multi-dim index in output
        let out_strides = new_shape.strides();
        let mut idx = vec![0usize; ndim];
        let mut rem = flat_out;
        for d in 0..ndim {
            if !out_strides.is_empty() && d < out_strides.len() {
                idx[d] = rem / out_strides[d];
                rem %= out_strides[d];
            }
        }

        // Sum over the axis dimension
        let mut s = 0.0;
        for a in 0..axis_size {
            idx[axis] = a;
            let flat_in: usize = idx.iter().zip(strides.iter()).map(|(&i, &st)| i * st).sum();
            s += t.data()[flat_in];
        }
        data[flat_out] = s;
    }

    Tensor::from_vec(data, new_shape).unwrap()
}
