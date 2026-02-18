# lumen-tensor

First-class tensor type with automatic differentiation for Lumen.

## Overview

`lumen-tensor` provides n-dimensional array operations with automatic differentiation (autodiff) for building machine learning models in Lumen. It includes a computation graph-based reverse-mode autodiff engine, common neural network layers, optimizers, and SIMD-accelerated operations for numeric computing.

The crate is designed to be a pure-Rust alternative to PyTorch/JAX for AI-native Lumen programs, with first-class support for training models, computing gradients, and deploying inference pipelines. Tensors are garbage-collected values that integrate seamlessly with Lumen's type system and effect handlers.

## Architecture

| Module | Purpose |
|--------|---------|
| `tensor.rs` | Core tensor type (shape, data, device) |
| `dtype.rs` | Data type abstraction (F32, F64, I32, etc.) |
| `shape.rs` | Shape manipulation (broadcast, reshape, transpose) |
| `ops.rs` | Tensor operations (matmul, conv, reduce, element-wise) |
| `ad.rs` | Automatic differentiation (computation graph, backprop) |
| `nn.rs` | Neural network layers (Linear, Conv2d, ReLU, Dropout) |
| `optim.rs` | Optimizers (SGD, Adam, AdamW, RMSprop) |
| `simd.rs` | SIMD-accelerated kernels (AVX2, NEON) |

## Key Types

### Tensor

```rust
pub struct Tensor {
    shape: Vec<usize>,
    data: Vec<f32>,         // Or other dtype
    device: Device,         // CPU, CUDA, Metal (future)
    grad: Option<Box<Tensor>>,  // Gradient tensor
    requires_grad: bool,
}

impl Tensor {
    pub fn new(data: Vec<f32>, shape: Vec<usize>) -> Self;
    pub fn zeros(shape: Vec<usize>) -> Self;
    pub fn ones(shape: Vec<usize>) -> Self;
    pub fn randn(shape: Vec<usize>) -> Self;
    
    pub fn matmul(&self, other: &Tensor) -> Tensor;
    pub fn add(&self, other: &Tensor) -> Tensor;
    pub fn relu(&self) -> Tensor;
    pub fn backward(&self);  // Compute gradients
}
```

### Automatic Differentiation

```rust
use lumen_tensor::ad::Variable;

let x = Variable::new(Tensor::from_vec(vec![1.0, 2.0, 3.0]));
let w = Variable::new(Tensor::from_vec(vec![0.5, -0.3, 0.8]));

let y = x.matmul(&w);
let loss = y.sum();

loss.backward();  // Compute gradients

println!("w.grad: {:?}", w.grad());
```

### Neural Network Layers

```rust
use lumen_tensor::nn::{Linear, ReLU, Module};

let layer1 = Linear::new(784, 128);
let relu = ReLU::new();
let layer2 = Linear::new(128, 10);

let x = Tensor::randn(vec![32, 784]);  // Batch of 32 images
let hidden = relu.forward(&layer1.forward(&x));
let logits = layer2.forward(&hidden);
```

### Optimizers

```rust
use lumen_tensor::optim::{Adam, Optimizer};

let mut optimizer = Adam::new(0.001);  // Learning rate
optimizer.add_param(&mut layer1.weight);
optimizer.add_param(&mut layer1.bias);

// Training loop
for epoch in 0..100 {
    let loss = compute_loss(&x, &y);
    loss.backward();
    optimizer.step();
    optimizer.zero_grad();
}
```

## Usage

### Basic Tensor Operations

```rust
use lumen_tensor::tensor::Tensor;

let a = Tensor::new(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]);
let b = Tensor::new(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]);

let c = a.add(&b);      // Element-wise addition
let d = a.matmul(&b);   // Matrix multiplication
let e = a.relu();       // Element-wise ReLU
```

### Training a Model

```rust
use lumen_tensor::nn::{Linear, Module};
use lumen_tensor::optim::Adam;

// Define model
let mut model = Linear::new(10, 1);
let mut optimizer = Adam::new(0.01);
optimizer.add_params(&model.parameters());

// Training loop
for (x, y) in dataset {
    let pred = model.forward(&x);
    let loss = (pred - y).pow(2.0).mean();
    
    loss.backward();
    optimizer.step();
    optimizer.zero_grad();
}
```

### Integration with Lumen

```lumen
# Lumen code using tensors
use tool tensor.create as TensorCreate
use tool tensor.matmul as MatMul

cell train_model() -> Null / {tensor}
  let x = TensorCreate(data: [1.0, 2.0, 3.0], shape: [1, 3])
  let w = TensorCreate(data: [0.5, -0.3, 0.8], shape: [3, 1])
  let y = MatMul(a: x, b: w)
  return null
end
```

## Operations

### Element-wise

`add`, `sub`, `mul`, `div`, `pow`, `exp`, `log`, `sqrt`, `sin`, `cos`, `tanh`, `relu`, `sigmoid`

### Reduction

`sum`, `mean`, `max`, `min`, `argmax`, `argmin`

### Matrix

`matmul`, `transpose`, `dot`, `norm`, `det`, `inv`

### Shape

`reshape`, `squeeze`, `unsqueeze`, `expand`, `permute`, `flatten`

### Advanced

`conv2d`, `pool2d`, `gather`, `scatter`, `index_select`, `masked_fill`

## SIMD Acceleration

The crate includes SIMD kernels for:
- Matrix multiplication (AVX2, NEON)
- Element-wise operations (AVX2, NEON)
- Reductions (AVX2, NEON)

SIMD is automatically detected and used when available. Fallback to scalar code on unsupported platforms.

## Testing

```bash
cargo test -p lumen-tensor

# Specific tests
cargo test -p lumen-tensor ops::
cargo test -p lumen-tensor ad::
cargo test -p lumen-tensor nn::

# Benchmarks
cargo bench -p lumen-tensor
```

## Performance

- **Matrix multiply (512x512)**: ~10ms on modern CPU
- **Gradient computation**: ~2x overhead vs forward pass
- **SIMD speedup**: 2-4x vs scalar code

## Future Work

- CUDA/Metal backend for GPU acceleration
- Sparse tensor support
- Quantization (INT8, FP16)
- Graph optimization (fusion, layout transforms)
- Distributed training primitives

See `ROADMAP.md` for timeline.

## Related Crates

- **lumen-rt** — Runtime integration (tensor as a Value type)
- **lumen-codegen** — JIT compilation of tensor kernels
- **num-traits** — Numeric trait abstractions
