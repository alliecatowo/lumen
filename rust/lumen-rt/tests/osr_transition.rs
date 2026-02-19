use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, OpCode};
use lumen_rt::vm::VM;

#[test]
fn osr_transition_returns_compiled_result() {
    let cell = LirCell {
        name: "main".to_string(),
        params: Vec::new(),
        returns: Some("Int".to_string()),
        registers: 6,
        constants: vec![Constant::Int(0), Constant::Int(1), Constant::Int(2000)],
        instructions: vec![
            Instruction::abx(OpCode::LoadK, 0, 0),
            Instruction::abx(OpCode::LoadK, 1, 0),
            Instruction::abx(OpCode::LoadK, 2, 2),
            Instruction::abc(OpCode::OsrCheck, 0, 0, 0),
            Instruction::abc(OpCode::Lt, 3, 0, 2),
            Instruction::abc(OpCode::Test, 3, 0, 0),
            Instruction::sax(OpCode::Jmp, 4),
            Instruction::abc(OpCode::Add, 1, 1, 0),
            Instruction::abx(OpCode::LoadK, 4, 1),
            Instruction::abc(OpCode::Add, 0, 0, 4),
            Instruction::sax(OpCode::Jmp, -8),
            Instruction::abc(OpCode::Return, 1, 1, 0),
        ],
        effect_handler_metas: Vec::new(),
        osr_points: vec![lumen_core::lir::LirOsrPoint {
            ip: 3,
            live_registers: vec![0, 1, 2, 3, 4],
        }],
    };

    let module = LirModule {
        version: "1.0.0".to_string(),
        doc_hash: "test".to_string(),
        strings: Vec::new(),
        types: Vec::new(),
        cells: vec![cell],
        tools: Vec::new(),
        policies: Vec::new(),
        agents: Vec::new(),
        addons: Vec::new(),
        effects: Vec::new(),
        effect_binds: Vec::new(),
        handlers: Vec::new(),
    };

    let mut vm = VM::new();
    vm.enable_jit(1);
    vm.load(module);
    #[cfg(feature = "jit")]
    vm.enable_osr_jit();

    let result = vm.execute("main", Vec::new()).expect("execution succeeds");
    assert_eq!(result.as_int().unwrap_or(0), 1999000);
    assert!(
        vm.osr_transition_count() > 0,
        "expected OSR transition to trigger"
    );
}
