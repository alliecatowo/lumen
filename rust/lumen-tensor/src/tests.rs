use crate::ad::Tape;
use crate::dtype::DType;
use crate::ops;
use crate::shape::Shape;
use crate::tensor::Tensor;

const EPS: f64 = 1e-6;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

// ─── DType tests ────────────────────────────────────────────────────────

#[test]
fn dtype_size_bytes() {
    assert_eq!(DType::F32.size_bytes(), 4);
    assert_eq!(DType::F64.size_bytes(), 8);
    assert_eq!(DType::I32.size_bytes(), 4);
    assert_eq!(DType::I64.size_bytes(), 8);
    assert_eq!(DType::Bool.size_bytes(), 1);
}

#[test]
fn dtype_is_float() {
    assert!(DType::F32.is_float());
    assert!(DType::F64.is_float());
    assert!(!DType::I32.is_float());
    assert!(!DType::Bool.is_float());
}

#[test]
fn dtype_is_integer() {
    assert!(DType::I32.is_integer());
    assert!(DType::I64.is_integer());
    assert!(!DType::F32.is_integer());
    assert!(!DType::Bool.is_integer());
}

// ─── Shape tests ────────────────────────────────────────────────────────

#[test]
fn shape_basics() {
    let s = Shape::new(vec![2, 3, 4]);
    assert_eq!(s.ndim(), 3);
    assert_eq!(s.numel(), 24);
    assert!(!s.is_scalar());
    assert!(!s.is_vector());
    assert!(!s.is_matrix());
}

#[test]
fn shape_scalar() {
    let s = Shape::scalar();
    assert_eq!(s.ndim(), 0);
    assert_eq!(s.numel(), 1);
    assert!(s.is_scalar());
}

#[test]
fn shape_vector_and_matrix() {
    assert!(Shape::new(vec![5]).is_vector());
    assert!(Shape::new(vec![3, 4]).is_matrix());
}

#[test]
fn shape_strides() {
    let s = Shape::new(vec![2, 3, 4]);
    assert_eq!(s.strides(), vec![12, 4, 1]);
}

#[test]
fn shape_strides_2d() {
    let s = Shape::new(vec![3, 4]);
    assert_eq!(s.strides(), vec![4, 1]);
}

#[test]
fn shape_broadcast_same() {
    let a = Shape::new(vec![3, 4]);
    let b = Shape::new(vec![3, 4]);
    assert_eq!(a.broadcast_with(&b).unwrap(), Shape::new(vec![3, 4]));
}

#[test]
fn shape_broadcast_expand() {
    let a = Shape::new(vec![3, 1]);
    let b = Shape::new(vec![1, 4]);
    assert_eq!(a.broadcast_with(&b).unwrap(), Shape::new(vec![3, 4]));
}

#[test]
fn shape_broadcast_different_ndim() {
    let a = Shape::new(vec![3, 4]);
    let b = Shape::new(vec![4]);
    assert_eq!(a.broadcast_with(&b).unwrap(), Shape::new(vec![3, 4]));
}

#[test]
fn shape_broadcast_incompatible() {
    let a = Shape::new(vec![3]);
    let b = Shape::new(vec![4]);
    assert!(a.broadcast_with(&b).is_err());
}

#[test]
fn shape_matmul_2d() {
    let a = Shape::new(vec![2, 3]);
    let b = Shape::new(vec![3, 4]);
    assert_eq!(Shape::matmul_shape(&a, &b).unwrap(), Shape::new(vec![2, 4]));
}

#[test]
fn shape_matmul_incompatible() {
    let a = Shape::new(vec![2, 3]);
    let b = Shape::new(vec![4, 5]);
    assert!(Shape::matmul_shape(&a, &b).is_err());
}

#[test]
fn shape_matmul_1d_dot() {
    let a = Shape::new(vec![3]);
    let b = Shape::new(vec![3]);
    assert_eq!(Shape::matmul_shape(&a, &b).unwrap(), Shape::scalar());
}

// ─── Tensor tests ───────────────────────────────────────────────────────

#[test]
fn tensor_zeros_and_ones() {
    let z = Tensor::zeros(Shape::new(vec![2, 3]));
    assert_eq!(z.numel(), 6);
    assert!(z.data().iter().all(|&x| x == 0.0));

    let o = Tensor::ones(Shape::new(vec![2, 3]));
    assert!(o.data().iter().all(|&x| x == 1.0));
}

#[test]
fn tensor_scalar() {
    let s = Tensor::scalar(3.14);
    assert!(s.shape().is_scalar());
    assert_eq!(s.numel(), 1);
    assert!(approx_eq(s.data()[0], 3.14));
}

#[test]
fn tensor_from_vec() {
    let t = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], Shape::new(vec![2, 2])).unwrap();
    assert!(approx_eq(t.get(&[0, 0]).unwrap(), 1.0));
    assert!(approx_eq(t.get(&[0, 1]).unwrap(), 2.0));
    assert!(approx_eq(t.get(&[1, 0]).unwrap(), 3.0));
    assert!(approx_eq(t.get(&[1, 1]).unwrap(), 4.0));
}

#[test]
fn tensor_from_vec_mismatch() {
    let r = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![2, 2]));
    assert!(r.is_err());
}

#[test]
fn tensor_indexing_set() {
    let mut t = Tensor::zeros(Shape::new(vec![3, 3]));
    t.set(&[1, 2], 42.0).unwrap();
    assert!(approx_eq(t.get(&[1, 2]).unwrap(), 42.0));
}

#[test]
fn tensor_index_out_of_bounds() {
    let t = Tensor::zeros(Shape::new(vec![2, 2]));
    assert!(t.get(&[2, 0]).is_err());
}

#[test]
fn tensor_reshape() {
    let t = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], Shape::new(vec![2, 3])).unwrap();
    let r = t.reshape(Shape::new(vec![3, 2])).unwrap();
    assert_eq!(r.shape(), &Shape::new(vec![3, 2]));
    // Data stays in the same flat order
    assert!(approx_eq(r.get(&[0, 0]).unwrap(), 1.0));
    assert!(approx_eq(r.get(&[2, 1]).unwrap(), 6.0));
}

#[test]
fn tensor_reshape_incompatible() {
    let t = Tensor::zeros(Shape::new(vec![2, 3]));
    assert!(t.reshape(Shape::new(vec![2, 2])).is_err());
}

#[test]
fn tensor_randn_shape() {
    let t = Tensor::randn(Shape::new(vec![10, 10]));
    assert_eq!(t.numel(), 100);
    // Values should not all be zero
    assert!(t.data().iter().any(|&x| x != 0.0));
}

#[test]
fn tensor_requires_grad() {
    let mut t = Tensor::scalar(1.0);
    assert!(!t.requires_grad());
    t.set_requires_grad(true);
    assert!(t.requires_grad());
}

// ─── Ops tests ──────────────────────────────────────────────────────────

#[test]
fn ops_add_elementwise() {
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
    let b = Tensor::from_vec(vec![4.0, 5.0, 6.0], Shape::new(vec![3])).unwrap();
    let c = ops::add(&a, &b).unwrap();
    assert!(approx_eq(c.data()[0], 5.0));
    assert!(approx_eq(c.data()[1], 7.0));
    assert!(approx_eq(c.data()[2], 9.0));
}

#[test]
fn ops_mul_elementwise() {
    let a = Tensor::from_vec(vec![2.0, 3.0], Shape::new(vec![2])).unwrap();
    let b = Tensor::from_vec(vec![4.0, 5.0], Shape::new(vec![2])).unwrap();
    let c = ops::mul(&a, &b).unwrap();
    assert!(approx_eq(c.data()[0], 8.0));
    assert!(approx_eq(c.data()[1], 15.0));
}

#[test]
fn ops_add_broadcast() {
    // [2, 3] + [3] -> [2, 3]
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], Shape::new(vec![2, 3])).unwrap();
    let b = Tensor::from_vec(vec![10.0, 20.0, 30.0], Shape::new(vec![3])).unwrap();
    let c = ops::add(&a, &b).unwrap();
    assert_eq!(c.shape(), &Shape::new(vec![2, 3]));
    assert!(approx_eq(c.data()[0], 11.0));
    assert!(approx_eq(c.data()[3], 14.0));
}

#[test]
fn ops_sub_elementwise() {
    let a = Tensor::from_vec(vec![5.0, 3.0], Shape::new(vec![2])).unwrap();
    let b = Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap();
    let c = ops::sub(&a, &b).unwrap();
    assert!(approx_eq(c.data()[0], 4.0));
    assert!(approx_eq(c.data()[1], 1.0));
}

#[test]
fn ops_div_elementwise() {
    let a = Tensor::from_vec(vec![6.0, 8.0], Shape::new(vec![2])).unwrap();
    let b = Tensor::from_vec(vec![2.0, 4.0], Shape::new(vec![2])).unwrap();
    let c = ops::div(&a, &b).unwrap();
    assert!(approx_eq(c.data()[0], 3.0));
    assert!(approx_eq(c.data()[1], 2.0));
}

#[test]
fn ops_neg() {
    let a = Tensor::from_vec(vec![1.0, -2.0, 3.0], Shape::new(vec![3])).unwrap();
    let b = ops::neg(&a);
    assert!(approx_eq(b.data()[0], -1.0));
    assert!(approx_eq(b.data()[1], 2.0));
    assert!(approx_eq(b.data()[2], -3.0));
}

#[test]
fn ops_exp_and_log() {
    let a = Tensor::from_vec(vec![0.0, 1.0], Shape::new(vec![2])).unwrap();
    let e = ops::exp(&a);
    assert!(approx_eq(e.data()[0], 1.0));
    assert!(approx_eq(e.data()[1], std::f64::consts::E));

    let l = ops::log(&e);
    assert!(approx_eq(l.data()[0], 0.0));
    assert!(approx_eq(l.data()[1], 1.0));
}

#[test]
fn ops_relu() {
    let a = Tensor::from_vec(vec![-1.0, 0.0, 1.0, -0.5], Shape::new(vec![4])).unwrap();
    let r = ops::relu(&a);
    assert!(approx_eq(r.data()[0], 0.0));
    assert!(approx_eq(r.data()[1], 0.0));
    assert!(approx_eq(r.data()[2], 1.0));
    assert!(approx_eq(r.data()[3], 0.0));
}

#[test]
fn ops_sigmoid() {
    let a = Tensor::scalar(0.0);
    let s = ops::sigmoid(&a);
    assert!(approx_eq(s.data()[0], 0.5));
}

#[test]
fn ops_tanh() {
    let a = Tensor::scalar(0.0);
    let t = ops::tanh(&a);
    assert!(approx_eq(t.data()[0], 0.0));
}

#[test]
fn ops_sum_and_mean() {
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], Shape::new(vec![4])).unwrap();
    let s = ops::sum(&a);
    assert!(approx_eq(s.data()[0], 10.0));
    let m = ops::mean(&a);
    assert!(approx_eq(m.data()[0], 2.5));
}

#[test]
fn ops_matmul_2x2() {
    // [[1, 2], [3, 4]] @ [[5, 6], [7, 8]] = [[19, 22], [43, 50]]
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], Shape::new(vec![2, 2])).unwrap();
    let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], Shape::new(vec![2, 2])).unwrap();
    let c = ops::matmul(&a, &b).unwrap();
    assert!(approx_eq(c.data()[0], 19.0));
    assert!(approx_eq(c.data()[1], 22.0));
    assert!(approx_eq(c.data()[2], 43.0));
    assert!(approx_eq(c.data()[3], 50.0));
}

#[test]
fn ops_matmul_shapes() {
    // (2,3) @ (3,4) -> (2,4)
    let a = Tensor::ones(Shape::new(vec![2, 3]));
    let b = Tensor::ones(Shape::new(vec![3, 4]));
    let c = ops::matmul(&a, &b).unwrap();
    assert_eq!(c.shape(), &Shape::new(vec![2, 4]));
    // Each element should be 3.0 (sum of 3 ones)
    assert!(c.data().iter().all(|&x| approx_eq(x, 3.0)));
}

#[test]
fn ops_matmul_dot_product() {
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap();
    let b = Tensor::from_vec(vec![4.0, 5.0, 6.0], Shape::new(vec![3])).unwrap();
    let c = ops::matmul(&a, &b).unwrap();
    assert!(c.shape().is_scalar());
    assert!(approx_eq(c.data()[0], 32.0));
}

#[test]
fn ops_transpose() {
    let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], Shape::new(vec![2, 3])).unwrap();
    let t = ops::transpose(&a).unwrap();
    assert_eq!(t.shape(), &Shape::new(vec![3, 2]));
    // [[1, 2, 3], [4, 5, 6]] -> [[1, 4], [2, 5], [3, 6]]
    assert!(approx_eq(t.get(&[0, 0]).unwrap(), 1.0));
    assert!(approx_eq(t.get(&[0, 1]).unwrap(), 4.0));
    assert!(approx_eq(t.get(&[1, 0]).unwrap(), 2.0));
    assert!(approx_eq(t.get(&[2, 1]).unwrap(), 6.0));
}

#[test]
fn ops_std_ops_traits() {
    let a = Tensor::from_vec(vec![1.0, 2.0], Shape::new(vec![2])).unwrap();
    let b = Tensor::from_vec(vec![3.0, 4.0], Shape::new(vec![2])).unwrap();
    let c = &a + &b;
    assert!(approx_eq(c.data()[0], 4.0));
    assert!(approx_eq(c.data()[1], 6.0));

    let d = &a * &b;
    assert!(approx_eq(d.data()[0], 3.0));
    assert!(approx_eq(d.data()[1], 8.0));

    let e = &a - &b;
    assert!(approx_eq(e.data()[0], -2.0));

    let f = &b / &a;
    assert!(approx_eq(f.data()[0], 3.0));
    assert!(approx_eq(f.data()[1], 2.0));

    let g = -&a;
    assert!(approx_eq(g.data()[0], -1.0));
}

// ─── AD tests ───────────────────────────────────────────────────────────

#[test]
fn ad_x_squared_grad() {
    // f(x) = x^2, f'(x) = 2x, at x=3 -> f'(3) = 6
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(3.0));
    let x2 = tape.mul(x, x);
    let grads = tape.backward(x2);
    // grad of x in x*x: d/dx(x*x) = 2x = 6
    assert!(approx_eq(grads[x.0].data()[0], 6.0));
}

#[test]
fn ad_exp_grad() {
    // f(x) = exp(x), f'(x) = exp(x), at x=0 -> f'(0) = 1.0
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(0.0));
    let y = tape.exp(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));
}

#[test]
fn ad_exp_grad_at_one() {
    // f(x) = exp(x) at x=1 -> f'(1) = e
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(1.0));
    let y = tape.exp(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], std::f64::consts::E));
}

#[test]
fn ad_log_grad() {
    // f(x) = log(x), f'(x) = 1/x, at x=2 -> f'(2) = 0.5
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(2.0));
    let y = tape.log(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 0.5));
}

#[test]
fn ad_sigmoid_grad() {
    // f(x) = sigmoid(x), f'(x) = sigmoid(x) * (1 - sigmoid(x))
    // At x=0: sigmoid(0) = 0.5, f'(0) = 0.25
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(0.0));
    let y = tape.sigmoid(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 0.25));
}

#[test]
fn ad_tanh_grad() {
    // f(x) = tanh(x), f'(x) = 1 - tanh(x)^2
    // At x=0: tanh(0) = 0, f'(0) = 1.0
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(0.0));
    let y = tape.tanh(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));
}

#[test]
fn ad_multi_variable() {
    // f(x, y) = x * y, df/dx = y, df/dy = x
    // At x=3, y=5: df/dx = 5, df/dy = 3
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(3.0));
    let y = tape.var(Tensor::scalar(5.0));
    let z = tape.mul(x, y);
    let grads = tape.backward(z);
    assert!(approx_eq(grads[x.0].data()[0], 5.0));
    assert!(approx_eq(grads[y.0].data()[0], 3.0));
}

#[test]
fn ad_chain_rule() {
    // f(x) = exp(x^2), f'(x) = 2x * exp(x^2)
    // At x=1: f'(1) = 2 * e ≈ 5.4366
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(1.0));
    let x2 = tape.mul(x, x);
    let y = tape.exp(x2);
    let grads = tape.backward(y);
    let expected = 2.0 * std::f64::consts::E;
    assert!(approx_eq(grads[x.0].data()[0], expected));
}

#[test]
fn ad_add_grad() {
    // f(x, y) = x + y, df/dx = 1, df/dy = 1
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(2.0));
    let y = tape.var(Tensor::scalar(3.0));
    let z = tape.add(x, y);
    let grads = tape.backward(z);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));
    assert!(approx_eq(grads[y.0].data()[0], 1.0));
}

#[test]
fn ad_sub_grad() {
    // f(x, y) = x - y, df/dx = 1, df/dy = -1
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(5.0));
    let y = tape.var(Tensor::scalar(3.0));
    let z = tape.sub(x, y);
    let grads = tape.backward(z);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));
    assert!(approx_eq(grads[y.0].data()[0], -1.0));
}

#[test]
fn ad_neg_grad() {
    // f(x) = -x, f'(x) = -1
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(7.0));
    let y = tape.neg(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], -1.0));
}

#[test]
fn ad_relu_grad() {
    // f(x) = relu(x) at x=2 -> f'(2) = 1
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(2.0));
    let y = tape.relu(x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));

    // f(x) = relu(x) at x=-1 -> f'(-1) = 0
    let mut tape2 = Tape::new();
    let x2 = tape2.var(Tensor::scalar(-1.0));
    let y2 = tape2.relu(x2);
    let grads2 = tape2.backward(y2);
    assert!(approx_eq(grads2[x2.0].data()[0], 0.0));
}

#[test]
fn ad_sum_grad() {
    // f(x) = sum([x1, x2, x3]), df/dxi = 1
    let mut tape = Tape::new();
    let x = tape.var(Tensor::from_vec(vec![1.0, 2.0, 3.0], Shape::new(vec![3])).unwrap());
    let y = tape.sum(x);
    let grads = tape.backward(y);
    assert_eq!(grads[x.0].numel(), 3);
    assert!(approx_eq(grads[x.0].data()[0], 1.0));
    assert!(approx_eq(grads[x.0].data()[1], 1.0));
    assert!(approx_eq(grads[x.0].data()[2], 1.0));
}

#[test]
fn ad_div_grad() {
    // f(x, y) = x / y, df/dx = 1/y, df/dy = -x/y^2
    // At x=6, y=2: df/dx = 0.5, df/dy = -6/4 = -1.5
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(6.0));
    let y = tape.var(Tensor::scalar(2.0));
    let z = tape.div(x, y);
    let grads = tape.backward(z);
    assert!(approx_eq(grads[x.0].data()[0], 0.5));
    assert!(approx_eq(grads[y.0].data()[0], -1.5));
}

#[test]
fn ad_matmul_grad_2x2() {
    // C = A @ B, dA = grad @ B^T, dB = A^T @ grad
    let mut tape = Tape::new();
    let a = tape.var(Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], Shape::new(vec![2, 2])).unwrap());
    let b = tape.var(Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], Shape::new(vec![2, 2])).unwrap());
    let c = tape.matmul(a, b);

    // To get scalar loss, sum the output
    let loss = tape.sum(c);
    let grads = tape.backward(loss);

    // dA = ones(2,2) @ B^T = [[5+7, 6+8], [5+7, 6+8]] = wait...
    // grad_output for matmul node is ones(2,2)
    // dA = ones @ B^T where B^T = [[5,7],[6,8]]
    // dA = [[5+6, 7+8], [5+6, 7+8]] = [[11, 15], [11, 15]]
    let ga = &grads[a.0];
    assert!(approx_eq(ga.get(&[0, 0]).unwrap(), 11.0));
    assert!(approx_eq(ga.get(&[0, 1]).unwrap(), 15.0));
    assert!(approx_eq(ga.get(&[1, 0]).unwrap(), 11.0));
    assert!(approx_eq(ga.get(&[1, 1]).unwrap(), 15.0));

    // dB = A^T @ ones = [[1+3, 1+3], [2+4, 2+4]] = [[4, 4], [6, 6]]
    let gb = &grads[b.0];
    assert!(approx_eq(gb.get(&[0, 0]).unwrap(), 4.0));
    assert!(approx_eq(gb.get(&[0, 1]).unwrap(), 4.0));
    assert!(approx_eq(gb.get(&[1, 0]).unwrap(), 6.0));
    assert!(approx_eq(gb.get(&[1, 1]).unwrap(), 6.0));
}

#[test]
fn ad_complex_expression() {
    // f(x) = (x + 1) * x = x^2 + x, f'(x) = 2x + 1
    // At x=4: f'(4) = 9
    let mut tape = Tape::new();
    let x = tape.var(Tensor::scalar(4.0));
    let one = tape.var(Tensor::scalar(1.0));
    let x_plus_1 = tape.add(x, one);
    let y = tape.mul(x_plus_1, x);
    let grads = tape.backward(y);
    assert!(approx_eq(grads[x.0].data()[0], 9.0));
}

#[test]
fn ad_transpose_grad() {
    let mut tape = Tape::new();
    let a = tape
        .var(Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], Shape::new(vec![2, 3])).unwrap());
    let t = tape.transpose(a);
    let s = tape.sum(t);
    let grads = tape.backward(s);
    // Sum of transposed = sum of original. Grad should be all ones with original shape.
    assert_eq!(grads[a.0].shape(), &Shape::new(vec![2, 3]));
    assert!(grads[a.0].data().iter().all(|&x| approx_eq(x, 1.0)));
}
