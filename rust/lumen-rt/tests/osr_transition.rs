use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, OpCode};
use lumen_rt::vm::VM;

fn loop_module_with_osr_points(osr_points: Vec<lumen_core::lir::LirOsrPoint>) -> LirModule {
    let cell = LirCell {
        name: "main".to_string(),
        params: Vec::new(),
        returns: Some("Int".to_string()),
        registers: 10,
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
        osr_points,
    };

    LirModule {
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
    }
}

#[test]
fn osr_transition_uses_entry_transfer_without_fallback() {
    let module = loop_module_with_osr_points(vec![lumen_core::lir::LirOsrPoint {
        // Enter after OsrCheck so loop back-edges remap to the first loop op.
        ip: 4,
        live_registers: vec![0, 1, 2],
    }]);

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
    assert!(
        vm.osr_entry_transition_count() > 0,
        "expected one-way OSR entry transfer to trigger"
    );
    assert_eq!(
        vm.osr_restart_fallback_count(),
        0,
        "expected no restart fallback for compatible OSR entry"
    );
}

#[test]
fn osr_transition_falls_back_when_entry_is_unavailable() {
    // No OSR metadata means no synthetic entry cell exists, so transition
    // should safely fall back to full-cell restart.
    let module = loop_module_with_osr_points(Vec::new());

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
    assert_eq!(
        vm.osr_entry_transition_count(),
        0,
        "expected no entry transfer transitions"
    );
    assert!(
        vm.osr_restart_fallback_count() > 0,
        "expected restart fallback transition to be used"
    );
}
