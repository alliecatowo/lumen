// Example: Using the unified opcode definition system

use lumen_codegen::opcode_def::OpcodeRegistry;
use lumen_core::lir::OpCode;

fn main() {
    println!("=== Unified Opcode Definition System Demo ===\n");

    // Create registry of all opcodes
    let registry = OpcodeRegistry::new();

    // 1. Show all registered opcodes
    println!("Registered Opcodes:");
    for def in registry.iter() {
        println!(
            "  - {} (OpCode::{:?}): {}",
            def.name, def.opcode, def.description
        );
    }
    println!();

    // 2. Look up a specific opcode
    println!("Looking up OpCode::Add:");
    if let Some(def) = registry.get(OpCode::Add) {
        println!("  Name: {}", def.name);
        println!("  Format: {:?}", def.format);
        println!("  Operands: {}", def.operands.len());
        println!("  Description: {}", def.description);
        println!("  JIT Strategy: {:?}", def.jit_strategy);
        println!();

        // Show operand details
        println!("  Operands:");
        for op in &def.operands {
            println!("    - {} ({:?}, {:?})", op.name, op.kind, op.ty);
        }
    }
    println!();

    // 3. Generate interpreter dispatch code
    println!("=== Generated Interpreter Dispatch Code ===");
    let interp_code = registry.generate_interpreter_dispatch();
    println!("{}", interp_code);
    println!();

    // 4. Generate JIT compilation stub
    println!("=== Generated JIT Compilation Stub ===");
    let jit_stub = registry.generate_jit_stub(OpCode::Add);
    println!("{}", jit_stub);
}
