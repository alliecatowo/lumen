//! Memory safety parity checklist — catalogues Lumen's memory safety features
//! and their implementation status relative to Rust.
//!
//! This module provides a structured inventory of every memory-safety guarantee
//! that Lumen provides (or plans to provide), along with the Rust equivalent,
//! current implementation status, and test-coverage flags.
//!
//! ## Usage
//!
//! ```rust
//! use lumen_compiler::compiler::parity_memory::MemoryParityChecklist;
//! let checklist = MemoryParityChecklist::full_checklist();
//! println!("{}", checklist.summary());
//! ```

use std::fmt;

// ── Categories ──────────────────────────────────────────────────────

/// Broad category for a memory-safety feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryCategory {
    OwnershipModel,
    BorrowChecking,
    LifetimeAnalysis,
    MoveSemantics,
    CopySemantics,
    DropSemantics,
    ArenaAllocation,
    GarbageCollection,
    StackAllocation,
    HeapManagement,
    RegionBasedMemory,
    LinearTypes,
    AffineTypes,
    ReferenceCountingOptimization,
    EscapeAnalysis,
}

impl fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::OwnershipModel => "Ownership Model",
            Self::BorrowChecking => "Borrow Checking",
            Self::LifetimeAnalysis => "Lifetime Analysis",
            Self::MoveSemantics => "Move Semantics",
            Self::CopySemantics => "Copy Semantics",
            Self::DropSemantics => "Drop Semantics",
            Self::ArenaAllocation => "Arena Allocation",
            Self::GarbageCollection => "Garbage Collection",
            Self::StackAllocation => "Stack Allocation",
            Self::HeapManagement => "Heap Management",
            Self::RegionBasedMemory => "Region-Based Memory",
            Self::LinearTypes => "Linear Types",
            Self::AffineTypes => "Affine Types",
            Self::ReferenceCountingOptimization => "Reference Counting Optimization",
            Self::EscapeAnalysis => "Escape Analysis",
        };
        write!(f, "{}", label)
    }
}

// ── Status ──────────────────────────────────────────────────────────

/// Implementation status of a parity item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParityStatus {
    /// Fully implemented and tested.
    Implemented,
    /// Partially implemented — description of what remains.
    PartiallyImplemented(String),
    /// Design exists but no code yet.
    Designed,
    /// Not applicable to Lumen's execution model.
    NotApplicable(String),
    /// Planned for a future milestone.
    Planned(String),
}

impl fmt::Display for ParityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Implemented => write!(f, "Implemented"),
            Self::PartiallyImplemented(detail) => {
                write!(f, "Partially Implemented ({})", detail)
            }
            Self::Designed => write!(f, "Designed"),
            Self::NotApplicable(reason) => write!(f, "N/A ({})", reason),
            Self::Planned(milestone) => write!(f, "Planned ({})", milestone),
        }
    }
}

impl ParityStatus {
    /// Returns `true` when the feature is fully implemented.
    pub fn is_implemented(&self) -> bool {
        matches!(self, Self::Implemented)
    }

    /// Returns `true` when the feature is not yet fully implemented
    /// (partially, designed, or planned).
    pub fn is_gap(&self) -> bool {
        matches!(
            self,
            Self::PartiallyImplemented(_) | Self::Designed | Self::Planned(_)
        )
    }
}

// ── Parity item ─────────────────────────────────────────────────────

/// A single entry in the memory-safety parity checklist.
#[derive(Debug, Clone)]
pub struct ParityItem {
    /// Unique identifier (e.g., "MEM-001").
    pub id: String,
    /// Category this item belongs to.
    pub category: MemoryCategory,
    /// Short feature name.
    pub feature: String,
    /// Human-readable description of the guarantee.
    pub description: String,
    /// Current implementation status.
    pub status: ParityStatus,
    /// The Rust language feature that provides this guarantee.
    pub rust_equivalent: String,
    /// How Lumen implements (or plans to implement) this guarantee.
    pub lumen_implementation: String,
    /// Whether automated test coverage exists.
    pub test_coverage: bool,
}

// ── The checklist ───────────────────────────────────────────────────

/// A collection of [`ParityItem`]s representing Lumen's memory-safety
/// posture relative to Rust.
#[derive(Debug, Clone)]
pub struct MemoryParityChecklist {
    pub items: Vec<ParityItem>,
}

impl MemoryParityChecklist {
    /// Build the canonical full checklist. Every known memory-safety
    /// property is enumerated here with its current status.
    pub fn full_checklist() -> Self {
        let items = vec![
            // ── Ownership Model ─────────────────────────────────
            ParityItem {
                id: "MEM-001".into(),
                category: MemoryCategory::OwnershipModel,
                feature: "Single owner".into(),
                description: "Each value has exactly one owning variable at any time".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Ownership system (each value has one owner)".into(),
                lumen_implementation: "ownership.rs: OwnershipChecker tracks per-variable state (Alive/Moved/Dropped)".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-002".into(),
                category: MemoryCategory::OwnershipModel,
                feature: "Ownership transfer".into(),
                description: "Assigning an owned value to another variable transfers ownership".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Move semantics (let y = x; moves x)".into(),
                lumen_implementation: "ownership.rs: use_var() marks Owned variables as Moved on first use".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-003".into(),
                category: MemoryCategory::OwnershipModel,
                feature: "Use-after-move detection".into(),
                description: "Compiler error when a moved value is used again".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust borrow checker E0382 (use of moved value)".into(),
                lumen_implementation: "ownership.rs: UseAfterMove error emitted by OwnershipChecker".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-004".into(),
                category: MemoryCategory::OwnershipModel,
                feature: "Ownership mode inference".into(),
                description: "Compiler infers whether a type is Copy or Owned from its definition".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust trait inference for Copy vs non-Copy types".into(),
                lumen_implementation: "ownership.rs: ownership_mode_for_type() classifies types as Copy or Owned".into(),
                test_coverage: true,
            },

            // ── Borrow Checking ─────────────────────────────────
            ParityItem {
                id: "MEM-005".into(),
                category: MemoryCategory::BorrowChecking,
                feature: "Immutable borrows".into(),
                description: "Multiple immutable borrows can coexist".into(),
                status: ParityStatus::PartiallyImplemented("Infrastructure exists in ownership.rs but no surface syntax yet".into()),
                rust_equivalent: "Shared references (&T)".into(),
                lumen_implementation: "ownership.rs: borrow_var() tracks borrow_count for immutable borrows".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-006".into(),
                category: MemoryCategory::BorrowChecking,
                feature: "Mutable borrows".into(),
                description: "Only one mutable borrow at a time, no concurrent immutable borrows".into(),
                status: ParityStatus::PartiallyImplemented("Infrastructure exists in ownership.rs but no surface syntax yet".into()),
                rust_equivalent: "Exclusive references (&mut T)".into(),
                lumen_implementation: "ownership.rs: borrow_var(mutable=true) enforces exclusivity".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-007".into(),
                category: MemoryCategory::BorrowChecking,
                feature: "Aliasing XOR mutation".into(),
                description: "Cannot have a mutable borrow while immutable borrows exist".into(),
                status: ParityStatus::PartiallyImplemented("Checker enforces but no surface syntax to trigger".into()),
                rust_equivalent: "Rust aliasing rules (E0502, E0499)".into(),
                lumen_implementation: "ownership.rs: AlreadyBorrowed error when mixing mut/immut borrows".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-008".into(),
                category: MemoryCategory::BorrowChecking,
                feature: "Move while borrowed".into(),
                description: "Cannot move a value while it is borrowed".into(),
                status: ParityStatus::PartiallyImplemented("Checker enforces but no surface syntax to trigger".into()),
                rust_equivalent: "Rust E0505 (moved while borrowed)".into(),
                lumen_implementation: "ownership.rs: MoveWhileBorrowed error in move_var()".into(),
                test_coverage: true,
            },

            // ── Lifetime Analysis ───────────────────────────────
            ParityItem {
                id: "MEM-009".into(),
                category: MemoryCategory::LifetimeAnalysis,
                feature: "Scope-based lifetimes".into(),
                description: "Values are dropped when their scope exits".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Lexical lifetimes / NLL".into(),
                lumen_implementation: "ownership.rs: exit_scope() checks and drops scope-local variables".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-010".into(),
                category: MemoryCategory::LifetimeAnalysis,
                feature: "Region-based lifetime analysis".into(),
                description: "References cannot outlive the region they borrow from".into(),
                status: ParityStatus::Designed,
                rust_equivalent: "Lifetime parameters ('a)".into(),
                lumen_implementation: "Planned: region annotations for borrow scoping".into(),
                test_coverage: false,
            },
            ParityItem {
                id: "MEM-011".into(),
                category: MemoryCategory::LifetimeAnalysis,
                feature: "Branch state merging".into(),
                description: "If/else branches conservatively merge ownership state".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "NLL control-flow analysis".into(),
                lumen_implementation: "ownership.rs: merge_branch_states() merges if/else/match arms".into(),
                test_coverage: true,
            },

            // ── Move Semantics ──────────────────────────────────
            ParityItem {
                id: "MEM-012".into(),
                category: MemoryCategory::MoveSemantics,
                feature: "Move on assignment".into(),
                description: "Assigning an owned value moves it; original is invalidated".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust move semantics for non-Copy types".into(),
                lumen_implementation: "ownership.rs: use_var() transitions Owned vars from Alive to Moved".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-013".into(),
                category: MemoryCategory::MoveSemantics,
                feature: "Move into function calls".into(),
                description: "Passing an owned value to a function consumes it".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust pass-by-value moves".into(),
                lumen_implementation: "ownership.rs: check_expr for Call walks args through use_var()".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-014".into(),
                category: MemoryCategory::MoveSemantics,
                feature: "Move across branches".into(),
                description: "Variables moved in any branch are conservatively marked moved".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust borrow checker branch analysis".into(),
                lumen_implementation: "ownership.rs: merge_branch_states marks moved in either branch as moved".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-015".into(),
                category: MemoryCategory::MoveSemantics,
                feature: "Reassignment restores ownership".into(),
                description: "Re-assigning to a moved variable restores it to Alive state".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust re-initialization after move".into(),
                lumen_implementation: "ownership.rs: check_stmt for Assign restores state to Alive".into(),
                test_coverage: true,
            },

            // ── Copy Semantics ──────────────────────────────────
            ParityItem {
                id: "MEM-016".into(),
                category: MemoryCategory::CopySemantics,
                feature: "Copy types (scalars)".into(),
                description: "Primitives (Int, Float, Bool, String) are freely copyable".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Copy trait for primitive types".into(),
                lumen_implementation: "ownership.rs: ownership_mode_for_type returns Copy for Int/Float/Bool/String/Null/Bytes/Json".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-017".into(),
                category: MemoryCategory::CopySemantics,
                feature: "Owned compound types".into(),
                description: "List, Map, Set, Tuple, Record, Fn are non-Copy (Owned)".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Non-Copy types (Vec, HashMap, etc.)".into(),
                lumen_implementation: "ownership.rs: ownership_mode_for_type returns Owned for compound types".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-018".into(),
                category: MemoryCategory::CopySemantics,
                feature: "Union Copy inference".into(),
                description: "Union types are Copy only if all variants are Copy".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust enum Copy derivation (all variants must be Copy)".into(),
                lumen_implementation: "ownership.rs: ownership_mode_for_type checks all Union arms".into(),
                test_coverage: true,
            },

            // ── Drop Semantics ──────────────────────────────────
            ParityItem {
                id: "MEM-019".into(),
                category: MemoryCategory::DropSemantics,
                feature: "LIFO drop order".into(),
                description: "Variables are dropped in reverse declaration order when scope exits".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust drop order (reverse declaration order)".into(),
                lumen_implementation: "ownership.rs: exit_scope iterates scope.vars (LIFO) checking/dropping".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-020".into(),
                category: MemoryCategory::DropSemantics,
                feature: "Defer blocks".into(),
                description: "defer blocks execute in LIFO order at scope exit for cleanup".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Drop trait / RAII pattern".into(),
                lumen_implementation: "AST Stmt::Defer; VM executes defer blocks in LIFO order".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-021".into(),
                category: MemoryCategory::DropSemantics,
                feature: "Not-consumed diagnostic".into(),
                description: "Owned variables that leave scope without being consumed produce warnings".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust #[must_use] / unused variable warnings".into(),
                lumen_implementation: "ownership.rs: NotConsumed error emitted for unconsumed Owned vars".into(),
                test_coverage: true,
            },

            // ── Arena Allocation ─────────────────────────────────
            ParityItem {
                id: "MEM-022".into(),
                category: MemoryCategory::ArenaAllocation,
                feature: "Bump-pointer arena".into(),
                description: "Process-local bump allocator for batch allocation/deallocation".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "bumpalo or typed-arena crates".into(),
                lumen_implementation: "lumen-vm arena.rs: Arena struct with alloc/alloc_value/reset".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-023".into(),
                category: MemoryCategory::ArenaAllocation,
                feature: "Arena reset (bulk free)".into(),
                description: "Arena.reset() reclaims all allocations at once, O(1) deallocation".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "typed_arena reset / bumpalo reset".into(),
                lumen_implementation: "lumen-vm arena.rs: Arena::reset() rewinds cursor, keeps first chunk".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-024".into(),
                category: MemoryCategory::ArenaAllocation,
                feature: "Arena thread safety (!Send/!Sync)".into(),
                description: "Arena is pinned to its creating thread, preventing data races".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "PhantomData<*mut u8> / !Send markers".into(),
                lumen_implementation: "lumen-vm arena.rs: _not_send_sync: PhantomData<*mut u8>".into(),
                test_coverage: true,
            },

            // ── Garbage Collection ──────────────────────────────
            ParityItem {
                id: "MEM-025".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "Immix block/line allocator".into(),
                description: "Immix-style allocator with 32 KiB blocks and 128-byte lines".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "N/A (Rust uses manual memory management)".into(),
                lumen_implementation: "lumen-vm immix.rs: Block/ImmixAllocator with mark/sweep/recycle".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-026".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "Tri-color marking".into(),
                description: "GC header with White/Gray/Black marking for incremental collection".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "N/A (no GC in Rust)".into(),
                lumen_implementation: "lumen-vm gc.rs: GcHeader with GcColor tri-color marking".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-027".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "Object pinning".into(),
                description: "GC-managed objects can be pinned to prevent relocation".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "std::pin::Pin<T>".into(),
                lumen_implementation: "lumen-vm gc.rs: GcHeader pinned bit prevents compaction moves".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-028".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "Generational collection (nursery/old-gen)".into(),
                description: "Young objects collected frequently, old objects less often".into(),
                status: ParityStatus::Designed,
                rust_equivalent: "N/A (no GC in Rust)".into(),
                lumen_implementation: "Planned: nursery/old-gen partitioning on top of Immix blocks".into(),
                test_coverage: false,
            },

            // ── Stack Allocation ─────────────────────────────────
            ParityItem {
                id: "MEM-029".into(),
                category: MemoryCategory::StackAllocation,
                feature: "Register-based VM".into(),
                description: "Values stored in a fixed register file, eliminating many heap allocations".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Stack allocation for local variables".into(),
                lumen_implementation: "lumen-vm: register-based interpreter with call-frame stack (max 256 depth)".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-030".into(),
                category: MemoryCategory::StackAllocation,
                feature: "Copy-on-write collections".into(),
                description: "Rc<T>-wrapped collections use Rc::make_mut for CoW semantics".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Cow<T> / Arc::make_mut".into(),
                lumen_implementation: "lumen-vm values.rs: Collection variants wrapped in Rc<T> with make_mut".into(),
                test_coverage: true,
            },

            // ── Heap Management ──────────────────────────────────
            ParityItem {
                id: "MEM-031".into(),
                category: MemoryCategory::HeapManagement,
                feature: "TLAB (thread-local allocation buffer)".into(),
                description: "Per-thread bump allocator for lock-free allocation fast path".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Thread-local allocator (e.g., jemalloc thread caches)".into(),
                lumen_implementation: "lumen-vm tlab.rs: Tlab struct with alloc/reset/capacity".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-032".into(),
                category: MemoryCategory::HeapManagement,
                feature: "Immix sweep and recycle".into(),
                description: "Sweep phase categorizes blocks into free/recyclable/occupied".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "N/A (manual dealloc in Rust)".into(),
                lumen_implementation: "lumen-vm immix.rs: ImmixAllocator::sweep() with free/recyclable lists".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-033".into(),
                category: MemoryCategory::HeapManagement,
                feature: "Null safety".into(),
                description: "No null pointer dereferences; Null is a first-class type".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Option<T> (no null pointers)".into(),
                lumen_implementation: "Type system: T? desugars to T | Null; null-safe access (?.) and coalesce (??)".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-034".into(),
                category: MemoryCategory::HeapManagement,
                feature: "Array bounds checking".into(),
                description: "Runtime bounds checks on list/tuple index access".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust slice/vec bounds checking (panic on OOB)".into(),
                lumen_implementation: "VM IndexAccess returns runtime error for out-of-bounds indices".into(),
                test_coverage: true,
            },

            // ── Region-Based Memory ─────────────────────────────
            ParityItem {
                id: "MEM-035".into(),
                category: MemoryCategory::RegionBasedMemory,
                feature: "Process-local arenas".into(),
                description: "Each process gets its own arena; cross-process sharing requires copying".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Thread-local storage / arena per thread".into(),
                lumen_implementation: "lumen-vm arena.rs: Arena is !Send/!Sync, one per process runtime".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-036".into(),
                category: MemoryCategory::RegionBasedMemory,
                feature: "Region-scoped deallocation".into(),
                description: "All allocations in a region freed at once on scope exit".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Arena drop / region-based allocation".into(),
                lumen_implementation: "lumen-vm arena.rs: Arena Drop impl frees all chunks; reset() for reuse".into(),
                test_coverage: true,
            },

            // ── Linear Types ────────────────────────────────────
            ParityItem {
                id: "MEM-037".into(),
                category: MemoryCategory::LinearTypes,
                feature: "Use-exactly-once enforcement".into(),
                description: "Linear-typed values must be consumed exactly once".into(),
                status: ParityStatus::PartiallyImplemented(
                    "Affine (at-most-once) enforced; linear (exactly-once) via NotConsumed warning".into()
                ),
                rust_equivalent: "No direct equivalent (Rust is affine, not linear)".into(),
                lumen_implementation: "ownership.rs: NotConsumed error for Owned vars not consumed before scope exit".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-038".into(),
                category: MemoryCategory::LinearTypes,
                feature: "Typestate tracking".into(),
                description: "Variables transition through typed states; invalid transitions are compile errors".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Typestate pattern via phantom types / session types".into(),
                lumen_implementation: "typestate.rs: TypestateChecker with state transitions and branch merging".into(),
                test_coverage: true,
            },

            // ── Affine Types ────────────────────────────────────
            ParityItem {
                id: "MEM-039".into(),
                category: MemoryCategory::AffineTypes,
                feature: "At-most-once consumption".into(),
                description: "Owned values can be used at most once; second use is a compile error".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust ownership (affine: values consumed at most once)".into(),
                lumen_implementation: "ownership.rs: Owned mode + Moved state on first use_var()".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-040".into(),
                category: MemoryCategory::AffineTypes,
                feature: "Lambda capture semantics".into(),
                description: "Closures capturing owned variables consume them (move capture)".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust closure move captures (FnOnce)".into(),
                lumen_implementation: "ownership.rs: check_expr for Lambda walks body with outer scope use_var".into(),
                test_coverage: true,
            },

            // ── Reference Counting Optimization ─────────────────
            ParityItem {
                id: "MEM-041".into(),
                category: MemoryCategory::ReferenceCountingOptimization,
                feature: "Rc-wrapped collections".into(),
                description: "Collections use Rc<T> for cheap clone and CoW mutation".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rc<T> / Arc<T>".into(),
                lumen_implementation: "lumen-vm values.rs: List/Tuple/Set/Map/Record wrapped in Rc".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-042".into(),
                category: MemoryCategory::ReferenceCountingOptimization,
                feature: "Rc elision for unique owners".into(),
                description: "When Rc strong_count == 1, mutations happen in-place (no copy)".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rc::make_mut optimization".into(),
                lumen_implementation: "lumen-vm values.rs: Rc::make_mut() for in-place mutation when unique".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-043".into(),
                category: MemoryCategory::ReferenceCountingOptimization,
                feature: "Redundant inc/dec elision".into(),
                description: "Compiler elides unnecessary reference count increments/decrements".into(),
                status: ParityStatus::Planned("Future optimization pass".into()),
                rust_equivalent: "LLVM optimizations on Rc operations".into(),
                lumen_implementation: "Planned: LIR optimization pass to elide redundant Rc ops".into(),
                test_coverage: false,
            },

            // ── Escape Analysis ─────────────────────────────────
            ParityItem {
                id: "MEM-044".into(),
                category: MemoryCategory::EscapeAnalysis,
                feature: "Stack vs heap decision".into(),
                description: "Compiler determines whether a value can stay on the stack or must be heap-allocated".into(),
                status: ParityStatus::Planned("Future optimization pass".into()),
                rust_equivalent: "LLVM scalar replacement / stack promotion".into(),
                lumen_implementation: "Planned: escape analysis in lowering to guide allocation strategy".into(),
                test_coverage: false,
            },
            ParityItem {
                id: "MEM-045".into(),
                category: MemoryCategory::EscapeAnalysis,
                feature: "Closure environment optimization".into(),
                description: "Non-escaping closures can use stack-allocated environments".into(),
                status: ParityStatus::Planned("Future optimization pass".into()),
                rust_equivalent: "LLVM inlining / stack promotion for closures".into(),
                lumen_implementation: "Planned: detect non-escaping closures and stack-allocate captures".into(),
                test_coverage: false,
            },

            // ── Additional safety guarantees ────────────────────
            ParityItem {
                id: "MEM-046".into(),
                category: MemoryCategory::HeapManagement,
                feature: "Integer overflow protection".into(),
                description: "Arithmetic overflow is detected at runtime".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust debug-mode overflow checks / checked arithmetic".into(),
                lumen_implementation: "VM arithmetic ops detect overflow conditions".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-047".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "GC forwarding pointer".into(),
                description: "Forwarding bit in GcHeader supports object relocation during compaction".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "N/A (no GC in Rust)".into(),
                lumen_implementation: "lumen-vm gc.rs: GcHeader forwarded bit for compaction support".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-048".into(),
                category: MemoryCategory::GarbageCollection,
                feature: "Type-tagged heap objects".into(),
                description: "Every GC-managed object carries a TypeTag for safe traversal".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "N/A (Rust uses static type info)".into(),
                lumen_implementation: "lumen-vm gc.rs: TypeTag enum (List/Tuple/Map/Set/Record/String/Bytes/Closure/Future)".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-049".into(),
                category: MemoryCategory::LifetimeAnalysis,
                feature: "Match arm state analysis".into(),
                description: "Match arms independently analyzed and merged for ownership state".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust NLL match arm analysis".into(),
                lumen_implementation: "ownership.rs: check_stmt for Match snapshots/restores/merges per arm".into(),
                test_coverage: true,
            },
            ParityItem {
                id: "MEM-050".into(),
                category: MemoryCategory::MoveSemantics,
                feature: "Destructuring move semantics".into(),
                description: "Pattern destructuring correctly propagates ownership for each binding".into(),
                status: ParityStatus::Implemented,
                rust_equivalent: "Rust pattern binding modes (move by default)".into(),
                lumen_implementation: "ownership.rs: bind_pattern() infers ownership per element in tuple/record/list destructuring".into(),
                test_coverage: true,
            },
        ];

        Self { items }
    }

    /// Return items belonging to a specific category.
    pub fn by_category(&self, cat: MemoryCategory) -> Vec<&ParityItem> {
        self.items.iter().filter(|i| i.category == cat).collect()
    }

    /// Count of fully-implemented items.
    pub fn implemented_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status.is_implemented())
            .count()
    }

    /// Total number of checklist items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Percentage of items that are fully implemented (0.0–100.0).
    pub fn coverage_percent(&self) -> f64 {
        if self.items.is_empty() {
            return 0.0;
        }
        (self.implemented_count() as f64 / self.total_count() as f64) * 100.0
    }

    /// Items that are not yet fully implemented (gaps).
    pub fn gaps(&self) -> Vec<&ParityItem> {
        self.items.iter().filter(|i| i.status.is_gap()).collect()
    }

    /// Items that are not applicable to Lumen.
    pub fn not_applicable(&self) -> Vec<&ParityItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.status, ParityStatus::NotApplicable(_)))
            .collect()
    }

    /// Generate a Markdown report of the full checklist.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Lumen Memory Safety Parity Checklist\n\n");
        md.push_str(&format!(
            "**Coverage**: {}/{} ({:.1}%)\n\n",
            self.implemented_count(),
            self.total_count(),
            self.coverage_percent()
        ));

        md.push_str("| ID | Category | Feature | Status | Test? |\n");
        md.push_str("|---|---|---|---|---|\n");

        for item in &self.items {
            let status_icon = match &item.status {
                ParityStatus::Implemented => "Implemented",
                ParityStatus::PartiallyImplemented(_) => "Partial",
                ParityStatus::Designed => "Designed",
                ParityStatus::NotApplicable(_) => "N/A",
                ParityStatus::Planned(_) => "Planned",
            };
            let test_icon = if item.test_coverage { "Yes" } else { "No" };
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                item.id, item.category, item.feature, status_icon, test_icon
            ));
        }

        if !self.gaps().is_empty() {
            md.push_str("\n## Gaps\n\n");
            for item in self.gaps() {
                md.push_str(&format!(
                    "- **{}** ({}): {}\n",
                    item.id, item.feature, item.status
                ));
            }
        }

        md
    }

    /// One-line human-readable summary.
    pub fn summary(&self) -> String {
        let gaps = self.gaps();
        let gap_str = if gaps.is_empty() {
            "no gaps".to_string()
        } else {
            format!("{} gaps", gaps.len())
        };
        format!(
            "Memory parity: {}/{} implemented ({:.1}%), {}",
            self.implemented_count(),
            self.total_count(),
            self.coverage_percent(),
            gap_str
        )
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_checklist_has_at_least_30_items() {
        let cl = MemoryParityChecklist::full_checklist();
        assert!(
            cl.total_count() >= 30,
            "Expected >= 30 items, got {}",
            cl.total_count()
        );
    }

    #[test]
    fn coverage_percent_in_range() {
        let cl = MemoryParityChecklist::full_checklist();
        let pct = cl.coverage_percent();
        assert!(pct > 0.0 && pct <= 100.0, "Coverage {pct}% out of range");
    }

    #[test]
    fn implemented_le_total() {
        let cl = MemoryParityChecklist::full_checklist();
        assert!(cl.implemented_count() <= cl.total_count());
    }

    #[test]
    fn gaps_are_non_implemented() {
        let cl = MemoryParityChecklist::full_checklist();
        for gap in cl.gaps() {
            assert!(
                gap.status.is_gap(),
                "gap item {} should have is_gap() == true",
                gap.id
            );
            assert!(
                !gap.status.is_implemented(),
                "gap item {} should not be implemented",
                gap.id
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        let cl = MemoryParityChecklist::full_checklist();
        let mut seen = std::collections::HashSet::new();
        for item in &cl.items {
            assert!(seen.insert(&item.id), "Duplicate ID: {}", item.id);
        }
    }

    #[test]
    fn all_categories_represented() {
        let cl = MemoryParityChecklist::full_checklist();
        let cats: std::collections::HashSet<_> = cl.items.iter().map(|i| i.category).collect();
        // All 15 categories should appear
        assert!(
            cats.len() >= 14,
            "Expected >= 14 categories, got {}",
            cats.len()
        );
    }

    #[test]
    fn summary_is_nonempty() {
        let cl = MemoryParityChecklist::full_checklist();
        let s = cl.summary();
        assert!(!s.is_empty());
        assert!(s.contains("Memory parity:"));
    }

    #[test]
    fn to_markdown_contains_table() {
        let cl = MemoryParityChecklist::full_checklist();
        let md = cl.to_markdown();
        assert!(md.contains("| ID |"));
        assert!(md.contains("MEM-001"));
    }
}
