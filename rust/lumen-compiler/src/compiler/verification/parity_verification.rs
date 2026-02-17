//! Verification parity checklist — tracks how Lumen's verification
//! capabilities compare to established verification languages and tools
//! such as F*, Dafny, Idris, Liquid Haskell, TLA+, and others.
//!
//! This module provides a structured inventory of verification features
//! across multiple categories, recording implementation status, the
//! comparable system, and the Lumen-specific approach.

use std::fmt;

// ── Verification category ──────────────────────────────────────────

/// Broad categories of verification capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationCategory {
    /// Static type safety (Hindley-Milner, subtyping, etc.)
    TypeSafety,
    /// Refinement types and `where`-clause constraints.
    RefinementTypes,
    /// Dependent types with value-indexed type families.
    DependentTypes,
    /// Runtime/compile-time contract checking.
    ContractChecking,
    /// Pre- and post-condition enforcement.
    PrePostConditions,
    /// Object / loop invariant checking.
    InvariantChecking,
    /// Termination / well-foundedness proofs.
    TerminationChecking,
    /// Exhaustiveness and redundancy checking for pattern matching.
    ExhaustivenessChecking,
    /// Algebraic effect tracking and row-polymorphism.
    EffectTracking,
    /// Resource accounting (budgets, quotas).
    ResourceAccounting,
    /// Information flow / taint analysis.
    InformationFlow,
    /// Machine-checkable proof generation.
    ProofGeneration,
    /// Explicit-state or symbolic model checking.
    ModelChecking,
    /// Abstract interpretation for numeric / pointer domains.
    AbstractInterpretation,
    /// Symbolic execution for path exploration.
    SymbolicExecution,
    /// Integration with external SMT solvers (Z3, CVC5).
    SmtIntegration,
    /// Fuzz testing integration.
    FuzzTesting,
    /// Property-based testing (QuickCheck-style).
    PropertyBasedTesting,
}

impl fmt::Display for VerificationCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::TypeSafety => "Type Safety",
            Self::RefinementTypes => "Refinement Types",
            Self::DependentTypes => "Dependent Types",
            Self::ContractChecking => "Contract Checking",
            Self::PrePostConditions => "Pre/Post Conditions",
            Self::InvariantChecking => "Invariant Checking",
            Self::TerminationChecking => "Termination Checking",
            Self::ExhaustivenessChecking => "Exhaustiveness Checking",
            Self::EffectTracking => "Effect Tracking",
            Self::ResourceAccounting => "Resource Accounting",
            Self::InformationFlow => "Information Flow",
            Self::ProofGeneration => "Proof Generation",
            Self::ModelChecking => "Model Checking",
            Self::AbstractInterpretation => "Abstract Interpretation",
            Self::SymbolicExecution => "Symbolic Execution",
            Self::SmtIntegration => "SMT Integration",
            Self::FuzzTesting => "Fuzz Testing",
            Self::PropertyBasedTesting => "Property-Based Testing",
        };
        write!(f, "{}", label)
    }
}

// ── Parity status ──────────────────────────────────────────────────

/// How far along Lumen's implementation of a given feature is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifParityStatus {
    /// Fully implemented and tested.
    Implemented,
    /// Partially implemented — the `String` describes what is missing.
    Partial(String),
    /// Designed but not yet implemented.
    Designed,
    /// Not applicable to Lumen's domain — the `String` explains why.
    NotApplicable(String),
}

impl fmt::Display for VerifParityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Implemented => write!(f, "Implemented"),
            Self::Partial(detail) => write!(f, "Partial ({})", detail),
            Self::Designed => write!(f, "Designed"),
            Self::NotApplicable(reason) => write!(f, "N/A ({})", reason),
        }
    }
}

// ── Parity item ────────────────────────────────────────────────────

/// A single line-item in the verification parity checklist.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationParityItem {
    /// Unique identifier, e.g. `"VP-001"`.
    pub id: String,
    /// Broad verification category.
    pub category: VerificationCategory,
    /// Short feature name (e.g. "Hindley-Milner type inference").
    pub feature: String,
    /// Longer description of what this verification capability entails.
    pub description: String,
    /// Current implementation status.
    pub status: VerifParityStatus,
    /// Name of the comparable system (e.g. "Dafny", "F*", "Idris").
    pub comparable_to: String,
    /// How Lumen implements or plans to implement this feature.
    pub lumen_approach: String,
}

impl fmt::Display for VerificationParityItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} — {} (status: {}, cf. {})",
            self.id, self.feature, self.description, self.status, self.comparable_to,
        )
    }
}

// ── Checklist ──────────────────────────────────────────────────────

/// The complete verification parity checklist.
#[derive(Debug, Clone)]
pub struct VerificationParityChecklist {
    /// All checklist items.
    pub items: Vec<VerificationParityItem>,
}

impl VerificationParityChecklist {
    /// Build the canonical Lumen verification parity checklist.
    ///
    /// This is the single source of truth for which verification features
    /// Lumen supports, how they compare to other systems, and what the
    /// Lumen-specific approach looks like.
    pub fn build() -> Self {
        let items = vec![
            // ── 1. Type Safety ─────────────────────────────────
            VerificationParityItem {
                id: "VP-001".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Hindley-Milner type inference".into(),
                description: "Bidirectional type inference with let-polymorphism".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Haskell, OCaml".into(),
                lumen_approach: "typecheck.rs implements bidirectional HM with extensions for records and enums".into(),
            },
            VerificationParityItem {
                id: "VP-002".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Generics with type parameters".into(),
                description: "Parametric polymorphism for cells, records, and enums".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Rust, TypeScript".into(),
                lumen_approach: "Generic params on cell/record/enum declarations with monomorphization at lowering".into(),
            },
            VerificationParityItem {
                id: "VP-003".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Union types".into(),
                description: "Ad-hoc union types with T | U syntax and optional sugar T?".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "TypeScript".into(),
                lumen_approach: "Parser desugars T? to T | Null; typechecker handles union widening and narrowing".into(),
            },
            VerificationParityItem {
                id: "VP-004".into(),
                category: VerificationCategory::TypeSafety,
                feature: "GADT type refinement".into(),
                description: "Pattern matching on GADTs refines type variables in each branch".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Haskell, OCaml".into(),
                lumen_approach: "gadts.rs tracks per-variant return type constraints and propagates refinements through match arms".into(),
            },

            // ── 2. Refinement Types ────────────────────────────
            VerificationParityItem {
                id: "VP-005".into(),
                category: VerificationCategory::RefinementTypes,
                feature: "Record field where-clauses".into(),
                description: "Refinement predicates on record fields checked at construction time".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Liquid Haskell".into(),
                lumen_approach: "constraints.rs lowers where-clause Exprs to Constraint IR; solver verifies at compile time".into(),
            },
            VerificationParityItem {
                id: "VP-006".into(),
                category: VerificationCategory::RefinementTypes,
                feature: "Cell where-clause preconditions".into(),
                description: "Where-clauses on cell signatures act as preconditions for callers".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny requires, Liquid Haskell refinements".into(),
                lumen_approach: "verification/mod.rs collects where-clauses and checks callers supply satisfying arguments".into(),
            },
            VerificationParityItem {
                id: "VP-007".into(),
                category: VerificationCategory::RefinementTypes,
                feature: "Path-sensitive refinement".into(),
                description: "Branch conditions refine variable constraints inside then/else bodies".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Liquid Haskell, F*".into(),
                lumen_approach: "refinement.rs RefinementContext tracks per-variable facts with path-sensitive merge at join points".into(),
            },

            // ── 3. Dependent Types ─────────────────────────────
            VerificationParityItem {
                id: "VP-008".into(),
                category: VerificationCategory::DependentTypes,
                feature: "Full dependent types".into(),
                description: "Types that depend on runtime values (Pi types, Sigma types)".into(),
                status: VerifParityStatus::NotApplicable("Lumen targets AI-native systems, not proof assistants".into()),
                comparable_to: "Idris, Agda, Coq".into(),
                lumen_approach: "Not planned; refinement types + where-clauses cover the practical subset".into(),
            },

            // ── 4. Contract Checking ───────────────────────────
            VerificationParityItem {
                id: "VP-009".into(),
                category: VerificationCategory::ContractChecking,
                feature: "Proof hint assertions".into(),
                description: "User-supplied @assert annotations checked by the solver".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny assert, SPARK Ada pragma Assert".into(),
                lumen_approach: "proof_hints.rs ProofHint::Assert validated and applied as solver assumptions".into(),
            },
            VerificationParityItem {
                id: "VP-010".into(),
                category: VerificationCategory::ContractChecking,
                feature: "Proof hint assumptions".into(),
                description: "User-supplied @assume annotations (unsound, for exploration)".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny assume, F* admit".into(),
                lumen_approach: "proof_hints.rs ProofHint::Assume with HintSeverity::Warning for soundness tracking".into(),
            },
            VerificationParityItem {
                id: "VP-011".into(),
                category: VerificationCategory::ContractChecking,
                feature: "Named lemmas".into(),
                description: "Named proof obligations that can be referenced across hints".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny lemma, F* val/let".into(),
                lumen_approach: "proof_hints.rs HintRegistry stores and resolves named lemmas".into(),
            },

            // ── 5. Pre/Post Conditions ─────────────────────────
            VerificationParityItem {
                id: "VP-012".into(),
                category: VerificationCategory::PrePostConditions,
                feature: "Caller-side precondition verification".into(),
                description: "Verify that call-site arguments satisfy callee where-clauses".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny requires, SPARK precondition".into(),
                lumen_approach: "verify_cell_contracts() walks call sites, substitutes args, checks preconditions via solver".into(),
            },
            VerificationParityItem {
                id: "VP-013".into(),
                category: VerificationCategory::PrePostConditions,
                feature: "Postcondition verification".into(),
                description: "Verify that return values satisfy declared postconditions".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "Dafny ensures, SPARK postcondition".into(),
                lumen_approach: "Planned: where-clauses on return types will be checked against return expressions".into(),
            },

            // ── 6. Invariant Checking ──────────────────────────
            VerificationParityItem {
                id: "VP-014".into(),
                category: VerificationCategory::InvariantChecking,
                feature: "Loop invariant hints".into(),
                description: "User-annotated loop invariants checked by the solver".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny invariant, SPARK loop_invariant".into(),
                lumen_approach: "proof_hints.rs ProofHint::LoopInvariant with optional loop_id for multi-loop cells".into(),
            },
            VerificationParityItem {
                id: "VP-015".into(),
                category: VerificationCategory::InvariantChecking,
                feature: "Machine state invariants".into(),
                description: "State-machine processes have typed state payloads checked at transitions".into(),
                status: VerifParityStatus::Partial("guard conditions checked; payload type invariants not yet verified statically".into()),
                comparable_to: "TLA+ state invariants".into(),
                lumen_approach: "VM enforces typed payloads at transition time; compile-time guard analysis planned".into(),
            },

            // ── 7. Termination Checking ────────────────────────
            VerificationParityItem {
                id: "VP-016".into(),
                category: VerificationCategory::TerminationChecking,
                feature: "Decreases annotations".into(),
                description: "User-supplied termination measures for recursive cells".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny decreases, Idris totality".into(),
                lumen_approach: "proof_hints.rs ProofHint::Decreases stores the measure expression for solver use".into(),
            },
            VerificationParityItem {
                id: "VP-017".into(),
                category: VerificationCategory::TerminationChecking,
                feature: "Automatic termination analysis".into(),
                description: "Infer termination measures for simple structural recursion".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "Agda, Idris totality checker".into(),
                lumen_approach: "Planned: detect structurally decreasing arguments in recursive cells".into(),
            },

            // ── 8. Exhaustiveness Checking ─────────────────────
            VerificationParityItem {
                id: "VP-018".into(),
                category: VerificationCategory::ExhaustivenessChecking,
                feature: "Match exhaustiveness on enums".into(),
                description: "Compiler checks all enum variants are covered in match statements".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Rust, Haskell, OCaml".into(),
                lumen_approach: "typecheck.rs IncompleteMatch error for missing variants; wildcard _ makes any match exhaustive".into(),
            },
            VerificationParityItem {
                id: "VP-019".into(),
                category: VerificationCategory::ExhaustivenessChecking,
                feature: "Guard-aware exhaustiveness".into(),
                description: "Match arms with guards do not count toward variant coverage".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Rust, OCaml".into(),
                lumen_approach: "Guarded arms excluded from coverage analysis; compiler warns if all arms are guarded".into(),
            },

            // ── 9. Effect Tracking ─────────────────────────────
            VerificationParityItem {
                id: "VP-020".into(),
                category: VerificationCategory::EffectTracking,
                feature: "Row-polymorphic effect system".into(),
                description: "Cells declare effect rows; callers must handle or propagate effects".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Koka, Eff, Frank".into(),
                lumen_approach: "resolve.rs infers effects; UndeclaredEffect diagnostic with provenance chain".into(),
            },
            VerificationParityItem {
                id: "VP-021".into(),
                category: VerificationCategory::EffectTracking,
                feature: "Algebraic effect handlers".into(),
                description: "handle/resume for one-shot delimited continuations".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Koka, OCaml 5 effects".into(),
                lumen_approach: "Four LIR opcodes (Perform, HandlePush, HandlePop, Resume) implement the handler stack in the VM".into(),
            },
            VerificationParityItem {
                id: "VP-022".into(),
                category: VerificationCategory::EffectTracking,
                feature: "Effect budget enforcement".into(),
                description: "Static cap on the number of effect invocations per cell".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Granule quantitative types".into(),
                lumen_approach: "EffectBudget constraint counts calls in cell body vs. declared max; verified at compile time".into(),
            },

            // ── 10. Resource Accounting ────────────────────────
            VerificationParityItem {
                id: "VP-023".into(),
                category: VerificationCategory::ResourceAccounting,
                feature: "Tool policy enforcement".into(),
                description: "Grant policies constrain which tools a cell can invoke and with what parameters".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Capability-safe languages (Wyvern, Pony)".into(),
                lumen_approach: "Runtime validate_tool_policy() checks domain, timeout_ms, max_tokens against merged grant policies".into(),
            },
            VerificationParityItem {
                id: "VP-024".into(),
                category: VerificationCategory::ResourceAccounting,
                feature: "Deterministic mode verification".into(),
                description: "@deterministic true rejects nondeterministic operations at resolve time".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny ghost/compiled distinction".into(),
                lumen_approach: "Resolver rejects uuid, timestamp, unknown external calls; defaults futures to DeferredFifo".into(),
            },

            // ── 11. Information Flow ───────────────────────────
            VerificationParityItem {
                id: "VP-025".into(),
                category: VerificationCategory::InformationFlow,
                feature: "Taint tracking for tool outputs".into(),
                description: "Track which values originate from untrusted tool calls".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "Jif, FlowCaml".into(),
                lumen_approach: "Planned: label tool results as tainted; require explicit sanitization before security-sensitive use".into(),
            },

            // ── 12. Proof Generation ───────────────────────────
            VerificationParityItem {
                id: "VP-026".into(),
                category: VerificationCategory::ProofGeneration,
                feature: "Verification certificate output".into(),
                description: "Emit machine-checkable proof certificates from successful verification runs".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "CakeML, CompCert".into(),
                lumen_approach: "Planned: serialize solver results + hint chain as JSON proof artifacts".into(),
            },

            // ── 13. Model Checking ─────────────────────────────
            VerificationParityItem {
                id: "VP-027".into(),
                category: VerificationCategory::ModelChecking,
                feature: "State-machine reachability analysis".into(),
                description: "Check whether terminal states are reachable / unreachable states are dead".into(),
                status: VerifParityStatus::Partial("VM validates transitions at runtime; static reachability not yet implemented".into()),
                comparable_to: "TLA+, Spin".into(),
                lumen_approach: "Machine process states have typed payloads and guards; planned: build state graph at compile time and check reachability".into(),
            },

            // ── 14. Abstract Interpretation ────────────────────
            VerificationParityItem {
                id: "VP-028".into(),
                category: VerificationCategory::AbstractInterpretation,
                feature: "Interval-based numeric bounds".into(),
                description: "Track integer intervals through assignments, branches, and operations".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Astrée, Polyspace".into(),
                lumen_approach: "smt_solver.rs BuiltinSmtSolver uses IntBounds for conjunction evaluation; bounds.rs BoundsContext for index checks".into(),
            },
            VerificationParityItem {
                id: "VP-029".into(),
                category: VerificationCategory::AbstractInterpretation,
                feature: "Array bounds propagation".into(),
                description: "Flow-sensitive analysis proving list/tuple index accesses in bounds".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "CBMC, Polyspace".into(),
                lumen_approach: "bounds.rs check_index_access and check_dynamic_index with condition-driven length inference".into(),
            },

            // ── 15. Symbolic Execution ─────────────────────────
            VerificationParityItem {
                id: "VP-030".into(),
                category: VerificationCategory::SymbolicExecution,
                feature: "Constraint-guided path exploration".into(),
                description: "Symbolically execute cell bodies to discover violating paths".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "KLEE, Symbolic PathFinder".into(),
                lumen_approach: "Planned: use SMT backend to explore branch conditions systematically".into(),
            },

            // ── 16. SMT Integration ────────────────────────────
            VerificationParityItem {
                id: "VP-031".into(),
                category: VerificationCategory::SmtIntegration,
                feature: "Z3 process backend".into(),
                description: "Communicate with Z3 via SMT-LIB2 over stdin/stdout".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny (Z3), F* (Z3)".into(),
                lumen_approach: "smt_solver.rs Z3ProcessSolver builds SMT-LIB2 scripts, spawns z3 -in -smt2, parses results and models".into(),
            },
            VerificationParityItem {
                id: "VP-032".into(),
                category: VerificationCategory::SmtIntegration,
                feature: "CVC5 process backend".into(),
                description: "Communicate with CVC5 via SMT-LIB2 over stdin/stdout".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Kind 2 (CVC5)".into(),
                lumen_approach: "smt_solver.rs Cvc5ProcessSolver mirrors Z3ProcessSolver with CVC5-specific flags".into(),
            },
            VerificationParityItem {
                id: "VP-033".into(),
                category: VerificationCategory::SmtIntegration,
                feature: "Builtin fallback solver".into(),
                description: "Always-available solver for QF_LIA/boolean without external deps".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "N/A (Lumen-specific)".into(),
                lumen_approach: "smt_solver.rs BuiltinSmtSolver with interval-based conjunction evaluation and model generation".into(),
            },
            VerificationParityItem {
                id: "VP-034".into(),
                category: VerificationCategory::SmtIntegration,
                feature: "Counter-example generation".into(),
                description: "Generate concrete violating inputs using boundary value analysis".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Dafny counterexamples, CBMC traces".into(),
                lumen_approach: "counterexample.rs parses constraints and applies boundary heuristics to find violating assignments".into(),
            },

            // ── 17. Fuzz Testing ───────────────────────────────
            VerificationParityItem {
                id: "VP-035".into(),
                category: VerificationCategory::FuzzTesting,
                feature: "Constraint-guided fuzz input generation".into(),
                description: "Generate random inputs that respect where-clause constraints for testing".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "AFL, libFuzzer".into(),
                lumen_approach: "Planned: use constraint IR to derive valid input domains, then sample randomly within those domains".into(),
            },

            // ── 18. Property-Based Testing ─────────────────────
            VerificationParityItem {
                id: "VP-036".into(),
                category: VerificationCategory::PropertyBasedTesting,
                feature: "Docs-as-tests extraction".into(),
                description: "Extract and execute code examples from markdown documentation as tests".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Rust doctest, Python doctest".into(),
                lumen_approach: "docs_as_tests.rs extracts fenced code blocks with expected output comments and verifies them".into(),
            },
            VerificationParityItem {
                id: "VP-037".into(),
                category: VerificationCategory::PropertyBasedTesting,
                feature: "QuickCheck-style property tests".into(),
                description: "Generate random inputs and check universal properties hold".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "Haskell QuickCheck, Hypothesis".into(),
                lumen_approach: "Planned: @property annotation on cells with automated shrinking".into(),
            },

            // ── Additional cross-cutting items ─────────────────
            VerificationParityItem {
                id: "VP-038".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Ownership / move semantics".into(),
                description: "Affine type tracking prevents use-after-move for owned types".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Rust borrow checker".into(),
                lumen_approach: "ownership.rs tracks Copy vs Owned categories and flags double-use of moved variables".into(),
            },
            VerificationParityItem {
                id: "VP-039".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Session type checking".into(),
                description: "Protocol-safe communication channels verified for duality".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Session types (Gay & Hole), Scribble".into(),
                lumen_approach: "session.rs SessionChecker verifies send/recv duality, choice branching, and recursion".into(),
            },
            VerificationParityItem {
                id: "VP-040".into(),
                category: VerificationCategory::TypeSafety,
                feature: "Typestate verification".into(),
                description: "Ensure method calls are valid in the current object state".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "Plaid, Fugue".into(),
                lumen_approach: "typestate.rs TypestateChecker tracks state transitions through cell bodies".into(),
            },
            VerificationParityItem {
                id: "VP-041".into(),
                category: VerificationCategory::RefinementTypes,
                feature: "Schema drift detection".into(),
                description: "Detect when record field types diverge from external schema expectations".into(),
                status: VerifParityStatus::Designed,
                comparable_to: "Prisma schema validation, Protobuf compatibility checks".into(),
                lumen_approach: "Planned: compare record definitions against declared external schemas and flag breaking changes".into(),
            },
            VerificationParityItem {
                id: "VP-042".into(),
                category: VerificationCategory::ResourceAccounting,
                feature: "Capability-based security checking".into(),
                description: "Cells can only invoke tools they have been granted access to via grant policies".into(),
                status: VerifParityStatus::Implemented,
                comparable_to: "E language, Wyvern capabilities".into(),
                lumen_approach: "Grant declarations scoped to cells; validate_tool_policy enforces constraints at dispatch time".into(),
            },
        ];

        Self { items }
    }

    /// Return items filtered by category.
    pub fn items_by_category(
        &self,
        category: VerificationCategory,
    ) -> Vec<&VerificationParityItem> {
        self.items
            .iter()
            .filter(|i| i.category == category)
            .collect()
    }

    /// Return items filtered by status.
    pub fn items_by_status(&self, status: &VerifParityStatus) -> Vec<&VerificationParityItem> {
        self.items.iter().filter(|i| &i.status == status).collect()
    }

    /// Return only items that are fully implemented.
    pub fn implemented_items(&self) -> Vec<&VerificationParityItem> {
        self.items
            .iter()
            .filter(|i| i.status == VerifParityStatus::Implemented)
            .collect()
    }

    /// Return items that are partially implemented or designed.
    pub fn pending_items(&self) -> Vec<&VerificationParityItem> {
        self.items
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    VerifParityStatus::Partial(_) | VerifParityStatus::Designed
                )
            })
            .collect()
    }

    /// Return items that are marked as not applicable.
    pub fn not_applicable_items(&self) -> Vec<&VerificationParityItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.status, VerifParityStatus::NotApplicable(_)))
            .collect()
    }

    /// Look up an item by its ID.
    pub fn find_by_id(&self, id: &str) -> Option<&VerificationParityItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Return the total number of items in the checklist.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check whether the checklist has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Return a summary report of the checklist status.
    pub fn summary(&self) -> ParitySummary {
        let total = self.items.len();
        let implemented = self.implemented_items().len();
        let partial = self
            .items
            .iter()
            .filter(|i| matches!(i.status, VerifParityStatus::Partial(_)))
            .count();
        let designed = self
            .items
            .iter()
            .filter(|i| i.status == VerifParityStatus::Designed)
            .count();
        let not_applicable = self.not_applicable_items().len();

        // Collect unique categories
        let mut categories = self.items.iter().map(|i| i.category).collect::<Vec<_>>();
        categories.sort_by_key(|c| format!("{:?}", c));
        categories.dedup();
        let category_count = categories.len();

        // Collect unique comparable systems
        let mut systems: Vec<String> = self
            .items
            .iter()
            .flat_map(|i| {
                i.comparable_to
                    .split(", ")
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        systems.sort();
        systems.dedup();
        let comparable_system_count = systems.len();

        ParitySummary {
            total,
            implemented,
            partial,
            designed,
            not_applicable,
            category_count,
            comparable_system_count,
        }
    }

    /// Validate the checklist for internal consistency.
    ///
    /// Checks:
    /// - All IDs are unique
    /// - All IDs follow the VP-NNN pattern
    /// - No empty feature names or descriptions
    /// - No empty comparable_to or lumen_approach fields
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for item in &self.items {
            // Unique IDs
            if !seen_ids.insert(&item.id) {
                errors.push(format!("Duplicate ID: {}", item.id));
            }

            // ID format: VP-NNN
            if !item.id.starts_with("VP-") || item.id.len() != 6 {
                errors.push(format!("ID '{}' does not match VP-NNN format", item.id));
            }

            // Non-empty fields
            if item.feature.is_empty() {
                errors.push(format!("{}: empty feature name", item.id));
            }
            if item.description.is_empty() {
                errors.push(format!("{}: empty description", item.id));
            }
            if item.comparable_to.is_empty() {
                errors.push(format!("{}: empty comparable_to", item.id));
            }
            if item.lumen_approach.is_empty() {
                errors.push(format!("{}: empty lumen_approach", item.id));
            }
        }

        errors
    }
}

impl Default for VerificationParityChecklist {
    fn default() -> Self {
        Self::build()
    }
}

// ── Summary ────────────────────────────────────────────────────────

/// Aggregate summary of the parity checklist status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParitySummary {
    /// Total number of items.
    pub total: usize,
    /// Number of fully implemented items.
    pub implemented: usize,
    /// Number of partially implemented items.
    pub partial: usize,
    /// Number of designed-only items.
    pub designed: usize,
    /// Number of not-applicable items.
    pub not_applicable: usize,
    /// Number of distinct categories covered.
    pub category_count: usize,
    /// Number of distinct comparable systems referenced.
    pub comparable_system_count: usize,
}

impl fmt::Display for ParitySummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Verification Parity: {}/{} implemented, {} partial, {} designed, {} N/A ({} categories, {} comparable systems)",
            self.implemented, self.total, self.partial, self.designed, self.not_applicable,
            self.category_count, self.comparable_system_count,
        )
    }
}

// ── Unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_builds_successfully() {
        let checklist = VerificationParityChecklist::build();
        assert!(!checklist.is_empty());
    }

    #[test]
    fn checklist_has_at_least_35_items() {
        let checklist = VerificationParityChecklist::build();
        assert!(
            checklist.len() >= 35,
            "expected >= 35 items, got {}",
            checklist.len()
        );
    }

    #[test]
    fn checklist_validation_passes() {
        let checklist = VerificationParityChecklist::build();
        let errors = checklist.validate();
        assert!(errors.is_empty(), "validation errors: {:?}", errors);
    }

    #[test]
    fn default_equals_build() {
        let default = VerificationParityChecklist::default();
        let built = VerificationParityChecklist::build();
        assert_eq!(default.len(), built.len());
    }
}
