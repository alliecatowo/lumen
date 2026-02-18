# Lumen Implementation Tasks — Nuclear Option Phase

## Phase 4: 64-bit Migration (Priority: High)
- [x] T600: Update `lumen-core/src/lir.rs`: Promote `Instruction` to 64-bit.
- [ ] T601: Update `lumen-compiler/src/compiler/lower.rs` to emit 64-bit instructions.
- [ ] T602: Update `lumen-codegen/src/ir.rs` to lower 64-bit instructions.
- [ ] T603: Update `lumen-rt/src/vm/mod.rs` to execute 64-bit instructions.
- [ ] T604: Verify: `cargo check --workspace` and `cargo test --workspace`.
- [ ] T605: [compiler] Update `RegAlloc` to support `u16` register indices.

## Phase 5: Deegen-style Stencils (Priority: High)
- [ ] T610: Stencil Definition: Create `lumen-rt/src/stencils/impl.rs`.
- [ ] T611: Stencil Extraction: Create `lumen-rt/build.rs`.
- [ ] T612: Stencil Generator: Implement `StencilGenerator` in `lumen-rt`.
- [ ] T613: Word-Stream Support: Implement multi-word instructions (e.g., `LoadConst64`).
- [ ] T614: [vm] Implement Inline Cache (IC) slots in Word-Stream for `GetField`.

## Phase 6: Tooling & Polish (Priority: Low)
- [ ] T620: Improve `run_all.sh` benchmark script.
- [ ] T621: Implement DAP (Debug Adapter Protocol).
- [ ] T622: Fix Markdown multi-line comments in LSP.
- [ ] T623: [test] Stress tests for 65k register usage.
- [ ] T624: [infra] CI check for stencil extraction correctness.
