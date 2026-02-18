//! Pre-built LIR programs for benchmarking and testing.
//!
//! Each function returns a self-contained `LirModule` suitable for passing
//! to `lower_module` + `emit_object`.

use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

/// Create an empty `LirModule` shell that can hold cells.
fn empty_module(cells: Vec<LirCell>) -> LirModule {
    LirModule {
        version: "1.0.0".to_string(),
        doc_hash: "bench".to_string(),
        strings: Vec::new(),
        types: Vec::new(),
        cells,
        tools: Vec::new(),
        policies: Vec::new(),
        agents: Vec::new(),
        addons: Vec::new(),
        effects: Vec::new(),
        effect_binds: Vec::new(),
        handlers: Vec::new(),
    }
}

/// Recursive fibonacci — `fib(n)`.
///
/// This is a *naive* recursive implementation that calls itself twice, making
/// it a good stress test for function-call overhead.
///
/// Pseudo-code:
/// ```text
/// cell fib(n: Int) -> Int
///   if n <= 1 then return n end
///   fib(n - 1) + fib(n - 2)
/// end
/// ```
///
/// LIR layout:
/// ```text
///  0: LoadInt   r1, 1
///  1: Le        r2, r0, r1      (n <= 1?)
///  2: Test      r2, 0, 0
///  3: Jmp       +1              (→ 5: not base case)
///  4: Return    r0              (return n)
///  5: LoadK     r3, 0           ("fib")
///  6: Sub       r4, r0, r1      (n - 1)
///  7: Call      r3, 1, 1        (fib(n - 1))
///  8: Move      r5, r3          (save result)
///  9: LoadInt   r6, 2
/// 10: Sub       r4, r0, r6      (n - 2)
/// 11: LoadK     r3, 0           ("fib")
/// 12: Call      r3, 1, 1        (fib(n - 2))
/// 13: Add       r7, r5, r3      (fib(n-1) + fib(n-2))
/// 14: Return    r7
/// ```
pub fn fibonacci_lir() -> LirModule {
    let cell = LirCell {
        name: "fib".to_string(),
        params: vec![LirParam {
            name: "n".to_string(),
            ty: "Int".to_string(),
            register: 0,
            variadic: false,
        }],
        returns: Some("Int".to_string()),
        registers: 8,
        constants: vec![Constant::String("fib".to_string())],
        instructions: vec![
            Instruction::abc(OpCode::LoadInt, 1, 1, 0), //  0: r1 = 1
            Instruction::abc(OpCode::Le, 2, 0, 1),      //  1: r2 = n <= 1
            Instruction::abc(OpCode::Test, 2, 0, 0),    //  2: test r2
            Instruction::sax(OpCode::Jmp, 1),           //  3: → 5
            Instruction::abc(OpCode::Return, 0, 1, 0),  //  4: return n
            Instruction::abx(OpCode::LoadK, 3, 0),      //  5: r3 = "fib"
            Instruction::abc(OpCode::Sub, 4, 0, 1),     //  6: r4 = n - 1
            Instruction::abc(OpCode::Call, 3, 1, 1),    //  7: call fib(r4)
            Instruction::abc(OpCode::Move, 5, 3, 0),    //  8: r5 = result
            Instruction::abc(OpCode::LoadInt, 6, 2, 0), //  9: r6 = 2
            Instruction::abc(OpCode::Sub, 4, 0, 6),     // 10: r4 = n - 2
            Instruction::abx(OpCode::LoadK, 3, 0),      // 11: r3 = "fib"
            Instruction::abc(OpCode::Call, 3, 1, 1),    // 12: call fib(r4)
            Instruction::abc(OpCode::Add, 7, 5, 3),     // 13: r7 = fib(n-1)+fib(n-2)
            Instruction::abc(OpCode::Return, 7, 1, 0),  // 14: return r7
        ],
        effect_handler_metas: Vec::new(),
    };

    empty_module(vec![cell])
}

/// Heavy arithmetic — many sequential operations in a single cell.
///
/// Pseudo-code:
/// ```text
/// cell arith(a: Int, b: Int) -> Int
///   let x = a + b
///   let y = a * b
///   let z = x - y
///   let w = z / (a | 1)
///   let v = w ^ b
///   x + y + z + w + v
/// end
/// ```
pub fn arithmetic_lir() -> LirModule {
    let cell = LirCell {
        name: "arith".to_string(),
        params: vec![
            LirParam {
                name: "a".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            },
            LirParam {
                name: "b".to_string(),
                ty: "Int".to_string(),
                register: 1,
                variadic: false,
            },
        ],
        returns: Some("Int".to_string()),
        registers: 10,
        constants: vec![Constant::Int(1)],
        instructions: vec![
            // x = a + b
            Instruction::abc(OpCode::Add, 2, 0, 1),
            // y = a * b
            Instruction::abc(OpCode::Mul, 3, 0, 1),
            // z = x - y
            Instruction::abc(OpCode::Sub, 4, 2, 3),
            // tmp = a | 1
            Instruction::abx(OpCode::LoadK, 5, 0), // r5 = 1
            Instruction::abc(OpCode::BitOr, 6, 0, 5),
            // w = z / tmp
            Instruction::abc(OpCode::Div, 7, 4, 6),
            // v = w ^ b
            Instruction::abc(OpCode::BitXor, 8, 7, 1),
            // result = x + y + z + w + v
            Instruction::abc(OpCode::Add, 9, 2, 3),
            Instruction::abc(OpCode::Add, 9, 9, 4),
            Instruction::abc(OpCode::Add, 9, 9, 7),
            Instruction::abc(OpCode::Add, 9, 9, 8),
            Instruction::abc(OpCode::Return, 9, 1, 0),
        ],
        effect_handler_metas: Vec::new(),
    };

    empty_module(vec![cell])
}

/// Simple loop — counts from 0 to a limit.
///
/// Pseudo-code:
/// ```text
/// cell count_to(limit: Int) -> Int
///   let i = 0
///   while i < limit
///     i = i + 1
///   end
///   i
/// end
/// ```
pub fn simple_loop_lir() -> LirModule {
    let cell = LirCell {
        name: "count_to".to_string(),
        params: vec![LirParam {
            name: "limit".to_string(),
            ty: "Int".to_string(),
            register: 0,
            variadic: false,
        }],
        returns: Some("Int".to_string()),
        registers: 5,
        constants: vec![],
        instructions: vec![
            // i = 0
            Instruction::abc(OpCode::LoadInt, 1, 0, 0), // 0: r1 = 0
            // step = 1
            Instruction::abc(OpCode::LoadInt, 2, 1, 0), // 1: r2 = 1
            // loop header: r3 = i < limit
            Instruction::abc(OpCode::Lt, 3, 1, 0), // 2: r3 = r1 < r0
            Instruction::abc(OpCode::Test, 3, 0, 0), // 3: test r3
            Instruction::sax(OpCode::Jmp, 2),      // 4: → 7 (exit)
            // i = i + 1
            Instruction::abc(OpCode::Add, 1, 1, 2), // 5: r1 += 1
            Instruction::sax(OpCode::Jmp, -5),      // 6: → 2 (loop)
            // return i
            Instruction::abc(OpCode::Return, 1, 1, 0), // 7: return r1
        ],
        effect_handler_metas: Vec::new(),
    };

    empty_module(vec![cell])
}

/// Multi-cell module with 20 trivial cells, each computing a different
/// arithmetic expression. Useful for benchmarking compilation throughput
/// with many functions.
pub fn multi_cell_lir(count: usize) -> LirModule {
    let cells: Vec<LirCell> = (0..count)
        .map(|i| {
            let name = format!("cell_{}", i);
            LirCell {
                name,
                params: vec![
                    LirParam {
                        name: "a".to_string(),
                        ty: "Int".to_string(),
                        register: 0,
                        variadic: false,
                    },
                    LirParam {
                        name: "b".to_string(),
                        ty: "Int".to_string(),
                        register: 1,
                        variadic: false,
                    },
                ],
                returns: Some("Int".to_string()),
                registers: 6,
                constants: vec![Constant::Int((i + 1) as i64)],
                instructions: vec![
                    // r2 = a + b
                    Instruction::abc(OpCode::Add, 2, 0, 1),
                    // r3 = constant[i+1]
                    Instruction::abx(OpCode::LoadK, 3, 0),
                    // r4 = r2 * r3
                    Instruction::abc(OpCode::Mul, 4, 2, 3),
                    // r5 = r4 - a
                    Instruction::abc(OpCode::Sub, 5, 4, 0),
                    // return r5
                    Instruction::abc(OpCode::Return, 5, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            }
        })
        .collect();

    empty_module(cells)
}

/// Tail-recursive countdown — ideal for TCO benchmarking.
///
/// Pseudo-code:
/// ```text
/// cell countdown(n: Int) -> Int
///   if n <= 0 then 0 else countdown(n - 1) end
/// end
/// ```
pub fn tail_recursive_countdown_lir() -> LirModule {
    let cell = LirCell {
        name: "countdown".to_string(),
        params: vec![LirParam {
            name: "n".to_string(),
            ty: "Int".to_string(),
            register: 0,
            variadic: false,
        }],
        returns: Some("Int".to_string()),
        registers: 6,
        constants: vec![Constant::String("countdown".to_string())],
        instructions: vec![
            Instruction::abc(OpCode::LoadInt, 1, 0, 0),  // 0: r1 = 0
            Instruction::abc(OpCode::Le, 2, 0, 1),       // 1: r2 = n <= 0
            Instruction::abc(OpCode::Test, 2, 0, 0),     // 2: test
            Instruction::sax(OpCode::Jmp, 2),            // 3: → 6 (else)
            Instruction::abc(OpCode::Move, 0, 1, 0),     // 4: r0 = 0
            Instruction::abc(OpCode::Return, 0, 1, 0),   // 5: return 0
            Instruction::abx(OpCode::LoadK, 3, 0),       // 6: r3 = "countdown"
            Instruction::abc(OpCode::LoadInt, 5, 1, 0),  // 7: r5 = 1
            Instruction::abc(OpCode::Sub, 4, 0, 5),      // 8: r4 = n - 1
            Instruction::abc(OpCode::TailCall, 3, 1, 1), // 9: tail-call countdown(r4)
        ],
        effect_handler_metas: Vec::new(),
    };

    empty_module(vec![cell])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::compile_object_module;
    use crate::emit::emit_object;

    fn compile_lir(lir: &LirModule) -> Vec<u8> {
        let ptr_ty = cranelift_codegen::ir::types::I64;
        let module = compile_object_module(lir, ptr_ty).expect("compilation should succeed");
        emit_object(module).expect("emission should succeed")
    }

    #[test]
    fn bench_fibonacci_compiles() {
        let lir = fibonacci_lir();
        let bytes = compile_lir(&lir);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn bench_arithmetic_compiles() {
        let lir = arithmetic_lir();
        let bytes = compile_lir(&lir);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn bench_simple_loop_compiles() {
        let lir = simple_loop_lir();
        let bytes = compile_lir(&lir);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn bench_multi_cell_compiles() {
        let lir = multi_cell_lir(20);
        assert_eq!(lir.cells.len(), 20);
        let bytes = compile_lir(&lir);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn bench_tail_recursive_compiles() {
        let lir = tail_recursive_countdown_lir();
        let bytes = compile_lir(&lir);
        assert!(!bytes.is_empty());
    }
}
