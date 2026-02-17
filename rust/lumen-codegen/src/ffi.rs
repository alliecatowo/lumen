//! FFI support for extern "C" calling convention.
//!
//! Provides the infrastructure needed to declare and call external C functions
//! from Lumen codegen. Extern cells (e.g. `extern cell malloc(size: Int) -> addr[Byte]`)
//! are compiled as imported functions with the platform-appropriate C calling
//! convention.
//!
//! ## Architecture
//!
//! The [`ExternFunction`] struct captures the signature of a C function in terms
//! that can be lowered to Cranelift IR. [`declare_extern`] registers the function
//! in a Cranelift [`ObjectModule`] with `Linkage::Import` and the correct
//! [`CallConv`]. [`emit_extern_call`] emits the actual `call` instruction in a
//! [`FunctionBuilder`].
//!
//! Type marshalling maps Lumen types to C-compatible Cranelift types:
//!
//! | Lumen type    | C / Cranelift type |
//! |---------------|--------------------|
//! | `Int`         | `i64`              |
//! | `Float`       | `f64`              |
//! | `Bool`        | `i8`               |
//! | `addr[T]`     | pointer (`i64`)    |
//! | `String`      | pointer (`i64`)    |
//! | everything else | pointer (`i64`)  |

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Type as ClifType, Value};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use crate::emit::CodegenError;

/// Calling convention selector for extern functions.
///
/// Maps to Cranelift's [`CallConv`] variants that correspond to real platform
/// ABIs. The `Auto` variant picks the right convention for the target triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    /// Automatically select the platform-default C calling convention.
    /// Resolves to `SystemV` on Linux/macOS-x86_64, `WindowsFastcall` on
    /// Windows, `AppleAarch64` on macOS-arm64.
    Auto,
    /// System V AMD64 ABI (Linux, FreeBSD, macOS x86_64).
    SystemV,
    /// Windows x64 calling convention.
    WindowsFastcall,
    /// Apple ARM64 calling convention (macOS/iOS arm64).
    AppleAarch64,
}

impl CallingConvention {
    /// Resolve this convention to a concrete Cranelift [`CallConv`] for the
    /// given target triple.
    pub fn to_call_conv(self, triple: &Triple) -> CallConv {
        match self {
            CallingConvention::Auto => CallConv::triple_default(triple),
            CallingConvention::SystemV => CallConv::SystemV,
            CallingConvention::WindowsFastcall => CallConv::WindowsFastcall,
            CallingConvention::AppleAarch64 => CallConv::AppleAarch64,
        }
    }
}

/// A C-type descriptor used in extern function signatures.
///
/// These correspond to the Lumen types that can appear in `extern cell`
/// parameter and return positions, mapped to their C ABI representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CType {
    /// 64-bit signed integer (`int64_t` / Lumen `Int`).
    I64,
    /// 64-bit IEEE 754 float (`double` / Lumen `Float`).
    F64,
    /// 8-bit integer used for booleans (`_Bool` / Lumen `Bool`).
    I8,
    /// Pointer-width integer representing an address (`void *`, `addr[T]`,
    /// `String` as `const char *`).
    Pointer,
    /// No return value (`void`).
    Void,
}

impl CType {
    /// Convert this C-type to the corresponding Cranelift IR type.
    ///
    /// The `pointer_type` argument supplies the platform's pointer width
    /// (typically `I64` on 64-bit targets).
    pub fn to_clif_type(self, pointer_type: ClifType) -> ClifType {
        match self {
            CType::I64 => types::I64,
            CType::F64 => types::F64,
            CType::I8 => types::I8,
            CType::Pointer => pointer_type,
            // Void is only valid for return types. We represent it as I64
            // for the rare case where the caller ignores the result. Callers
            // should check `return_type == CType::Void` and skip pushing
            // a return AbiParam.
            CType::Void => types::I64,
        }
    }
}

/// Map a Lumen type string (as stored in LIR metadata) to a [`CType`].
///
/// This is the marshalling layer between Lumen's type system and C FFI types.
///
/// Rules:
/// - `"Int"` → `CType::I64`
/// - `"Float"` → `CType::F64`
/// - `"Bool"` → `CType::I8`
/// - `"Null"` / `"Void"` → `CType::Void`
/// - Anything starting with `"addr["` → `CType::Pointer`
/// - `"String"` → `CType::Pointer` (null-terminated UTF-8)
/// - Everything else → `CType::Pointer` (opaque heap pointer)
pub fn marshal_lumen_type(ty_str: &str) -> CType {
    match ty_str {
        "Int" => CType::I64,
        "Float" => CType::F64,
        "Bool" => CType::I8,
        "Null" | "Void" => CType::Void,
        s if s.starts_with("addr[") => CType::Pointer,
        _ => CType::Pointer,
    }
}

/// Description of an extern (imported) C function.
///
/// Captures everything needed to declare the function in a Cranelift module
/// and emit calls to it.
#[derive(Debug, Clone)]
pub struct ExternFunction {
    /// The symbol name as it appears in the C library (e.g. `"malloc"`).
    pub name: String,
    /// Parameter types in declaration order.
    pub param_types: Vec<CType>,
    /// Return type. `CType::Void` means no return value.
    pub return_type: CType,
    /// Calling convention to use.
    pub calling_convention: CallingConvention,
}

impl ExternFunction {
    /// Create a new extern function descriptor.
    pub fn new(
        name: impl Into<String>,
        param_types: Vec<CType>,
        return_type: CType,
        calling_convention: CallingConvention,
    ) -> Self {
        Self {
            name: name.into(),
            param_types,
            return_type,
            calling_convention,
        }
    }

    /// Build the Cranelift [`Signature`](cranelift_codegen::ir::Signature) for
    /// this extern function.
    pub fn build_signature(
        &self,
        triple: &Triple,
        pointer_type: ClifType,
    ) -> cranelift_codegen::ir::Signature {
        let call_conv = self.calling_convention.to_call_conv(triple);
        let mut sig = cranelift_codegen::ir::Signature::new(call_conv);

        for param in &self.param_types {
            sig.params
                .push(AbiParam::new(param.to_clif_type(pointer_type)));
        }

        if self.return_type != CType::Void {
            sig.returns
                .push(AbiParam::new(self.return_type.to_clif_type(pointer_type)));
        }

        sig
    }
}

/// Declare an extern function in the Cranelift [`ObjectModule`].
///
/// The function is registered with `Linkage::Import`, meaning it will be
/// resolved by the linker at link time (from a shared library or static
/// archive).
///
/// Returns the [`FuncId`] that can later be used with
/// [`emit_extern_call`] to generate call-site code.
pub fn declare_extern(
    module: &mut ObjectModule,
    triple: &Triple,
    pointer_type: ClifType,
    ext_fn: &ExternFunction,
) -> Result<FuncId, CodegenError> {
    let sig = ext_fn.build_signature(triple, pointer_type);
    let func_id = module
        .declare_function(&ext_fn.name, Linkage::Import, &sig)
        .map_err(|e| {
            CodegenError::LoweringError(format!(
                "failed to declare extern function '{}': {e}",
                ext_fn.name
            ))
        })?;
    Ok(func_id)
}

/// Emit a call to a previously declared extern function.
///
/// The caller must have already:
/// 1. Called [`declare_extern`] to get a `FuncId`.
/// 2. Called `module.declare_func_in_func(func_id, &mut func)` to get a
///    `FuncRef` local to the current function being built.
///
/// `args` are the Cranelift [`Value`]s to pass. Their types must match the
/// extern function's parameter types.
///
/// Returns the result value, or `None` if the extern function returns void.
pub fn emit_extern_call(
    builder: &mut FunctionBuilder,
    func_ref: cranelift_codegen::ir::FuncRef,
    args: &[Value],
    returns_void: bool,
) -> Option<Value> {
    let call = builder.ins().call(func_ref, args);
    if returns_void {
        None
    } else {
        let results = builder.inst_results(call);
        Some(results[0])
    }
}

/// Batch-declare multiple extern functions. Returns a map from function name
/// to [`FuncId`].
///
/// This is a convenience wrapper around [`declare_extern`] for modules with
/// many extern declarations.
pub fn declare_externs(
    module: &mut ObjectModule,
    triple: &Triple,
    pointer_type: ClifType,
    ext_fns: &[ExternFunction],
) -> Result<Vec<(String, FuncId)>, CodegenError> {
    let mut results = Vec::with_capacity(ext_fns.len());
    for ext_fn in ext_fns {
        let func_id = declare_extern(module, triple, pointer_type, ext_fn)?;
        results.push((ext_fn.name.clone(), func_id));
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CodegenContext;
    use cranelift_codegen::ir::types;
    use cranelift_frontend::FunctionBuilderContext;
    use cranelift_module::Module;

    // -----------------------------------------------------------------------
    // 1. CallingConvention resolution
    // -----------------------------------------------------------------------

    #[test]
    fn calling_convention_auto_resolves_to_platform_default() {
        let triple = Triple::host();
        let cc = CallingConvention::Auto.to_call_conv(&triple);
        // On any CI/dev machine this should resolve to a valid CallConv.
        let expected = CallConv::triple_default(&triple);
        assert_eq!(cc, expected);
    }

    #[test]
    fn calling_convention_explicit_systemv() {
        let triple: Triple = "x86_64-unknown-linux-gnu".parse().unwrap();
        let cc = CallingConvention::SystemV.to_call_conv(&triple);
        assert_eq!(cc, CallConv::SystemV);
    }

    #[test]
    fn calling_convention_explicit_windows_fastcall() {
        let triple: Triple = "x86_64-pc-windows-msvc".parse().unwrap();
        let cc = CallingConvention::WindowsFastcall.to_call_conv(&triple);
        assert_eq!(cc, CallConv::WindowsFastcall);
    }

    #[test]
    fn calling_convention_explicit_apple_aarch64() {
        let triple: Triple = "aarch64-apple-darwin".parse().unwrap();
        let cc = CallingConvention::AppleAarch64.to_call_conv(&triple);
        assert_eq!(cc, CallConv::AppleAarch64);
    }

    // -----------------------------------------------------------------------
    // 2. Type marshalling
    // -----------------------------------------------------------------------

    #[test]
    fn marshal_primitive_types() {
        assert_eq!(marshal_lumen_type("Int"), CType::I64);
        assert_eq!(marshal_lumen_type("Float"), CType::F64);
        assert_eq!(marshal_lumen_type("Bool"), CType::I8);
    }

    #[test]
    fn marshal_pointer_types() {
        assert_eq!(marshal_lumen_type("addr[Byte]"), CType::Pointer);
        assert_eq!(marshal_lumen_type("addr[Int]"), CType::Pointer);
        assert_eq!(marshal_lumen_type("String"), CType::Pointer);
    }

    #[test]
    fn marshal_void_types() {
        assert_eq!(marshal_lumen_type("Null"), CType::Void);
        assert_eq!(marshal_lumen_type("Void"), CType::Void);
    }

    #[test]
    fn marshal_complex_types_are_pointers() {
        // Records, lists, maps, etc. all become opaque pointers.
        assert_eq!(marshal_lumen_type("MyRecord"), CType::Pointer);
        assert_eq!(marshal_lumen_type("List[Int]"), CType::Pointer);
        assert_eq!(marshal_lumen_type("Map[String, Int]"), CType::Pointer);
    }

    #[test]
    fn ctype_to_clif_type_mapping() {
        let ptr = types::I64;
        assert_eq!(CType::I64.to_clif_type(ptr), types::I64);
        assert_eq!(CType::F64.to_clif_type(ptr), types::F64);
        assert_eq!(CType::I8.to_clif_type(ptr), types::I8);
        assert_eq!(CType::Pointer.to_clif_type(ptr), types::I64);
    }

    // -----------------------------------------------------------------------
    // 3. ExternFunction signature building
    // -----------------------------------------------------------------------

    #[test]
    fn extern_function_signature_no_params_int_return() {
        let triple: Triple = "x86_64-unknown-linux-gnu".parse().unwrap();
        let ext = ExternFunction::new("get_value", vec![], CType::I64, CallingConvention::SystemV);
        let sig = ext.build_signature(&triple, types::I64);

        assert_eq!(sig.call_conv, CallConv::SystemV);
        assert!(sig.params.is_empty());
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0].value_type, types::I64);
    }

    #[test]
    fn extern_function_signature_multiple_params() {
        let triple: Triple = "x86_64-unknown-linux-gnu".parse().unwrap();
        let ext = ExternFunction::new(
            "do_stuff",
            vec![CType::I64, CType::F64, CType::I8, CType::Pointer],
            CType::Pointer,
            CallingConvention::SystemV,
        );
        let sig = ext.build_signature(&triple, types::I64);

        assert_eq!(sig.params.len(), 4);
        assert_eq!(sig.params[0].value_type, types::I64);
        assert_eq!(sig.params[1].value_type, types::F64);
        assert_eq!(sig.params[2].value_type, types::I8);
        assert_eq!(sig.params[3].value_type, types::I64); // pointer
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0].value_type, types::I64); // pointer
    }

    #[test]
    fn extern_function_signature_void_return() {
        let triple: Triple = "x86_64-unknown-linux-gnu".parse().unwrap();
        let ext = ExternFunction::new(
            "free",
            vec![CType::Pointer],
            CType::Void,
            CallingConvention::SystemV,
        );
        let sig = ext.build_signature(&triple, types::I64);

        assert_eq!(sig.params.len(), 1);
        assert!(
            sig.returns.is_empty(),
            "void return should have no return params"
        );
    }

    // -----------------------------------------------------------------------
    // 4. declare_extern generates correct Cranelift declarations
    // -----------------------------------------------------------------------

    #[test]
    fn declare_extern_malloc() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        let malloc = ExternFunction::new(
            "malloc",
            vec![CType::I64],
            CType::Pointer,
            CallingConvention::Auto,
        );

        let func_id = declare_extern(&mut ctx.module, &triple, ptr_ty, &malloc);
        assert!(
            func_id.is_ok(),
            "declaring malloc should succeed: {:?}",
            func_id.err()
        );
    }

    #[test]
    fn declare_extern_free() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        let free = ExternFunction::new(
            "free",
            vec![CType::Pointer],
            CType::Void,
            CallingConvention::Auto,
        );

        let func_id = declare_extern(&mut ctx.module, &triple, ptr_ty, &free);
        assert!(
            func_id.is_ok(),
            "declaring free should succeed: {:?}",
            func_id.err()
        );
    }

    #[test]
    fn declare_multiple_externs() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        let externs = vec![
            ExternFunction::new(
                "malloc",
                vec![CType::I64],
                CType::Pointer,
                CallingConvention::Auto,
            ),
            ExternFunction::new(
                "free",
                vec![CType::Pointer],
                CType::Void,
                CallingConvention::Auto,
            ),
            ExternFunction::new(
                "strlen",
                vec![CType::Pointer],
                CType::I64,
                CallingConvention::Auto,
            ),
        ];

        let result = declare_externs(&mut ctx.module, &triple, ptr_ty, &externs);
        assert!(
            result.is_ok(),
            "batch declare should succeed: {:?}",
            result.err()
        );
        let declared = result.unwrap();
        assert_eq!(declared.len(), 3);
        assert_eq!(declared[0].0, "malloc");
        assert_eq!(declared[1].0, "free");
        assert_eq!(declared[2].0, "strlen");
    }

    // -----------------------------------------------------------------------
    // 5. Calling convention correctness on Linux target
    // -----------------------------------------------------------------------

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn systemv_calling_convention_on_linux_target() {
        let ctx = CodegenContext::new_with_target("x86_64-unknown-linux-gnu")
            .expect("linux target context");
        let triple: Triple = "x86_64-unknown-linux-gnu".parse().unwrap();
        let ptr_ty = ctx.pointer_type();

        let ext = ExternFunction::new(
            "puts",
            vec![CType::Pointer],
            CType::I64,
            CallingConvention::Auto,
        );

        let sig = ext.build_signature(&triple, ptr_ty);
        assert_eq!(
            sig.call_conv,
            CallConv::SystemV,
            "Auto on Linux x86_64 should resolve to SystemV"
        );
    }

    // -----------------------------------------------------------------------
    // 6. Full end-to-end: declare extern + emit call in a Lumen function
    // -----------------------------------------------------------------------

    #[test]
    fn emit_extern_call_in_function() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        // Declare extern: int64_t square(int64_t x)
        let square = ExternFunction::new(
            "square",
            vec![CType::I64],
            CType::I64,
            CallingConvention::Auto,
        );
        let ext_func_id =
            declare_extern(&mut ctx.module, &triple, ptr_ty, &square).expect("declare extern");

        // Declare a Lumen wrapper function that calls square(42).
        let mut wrapper_sig = ctx.module.make_signature();
        wrapper_sig.returns.push(AbiParam::new(types::I64));
        let wrapper_id = ctx
            .module
            .declare_function("call_square", Linkage::Export, &wrapper_sig)
            .expect("declare wrapper");

        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, wrapper_id.as_u32()),
            wrapper_sig,
        );

        // Import the extern function into this function's namespace.
        let ext_func_ref = ctx.module.declare_func_in_func(ext_func_id, &mut func);

        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fb_ctx);

        let entry = builder.create_block();
        builder.switch_to_block(entry);

        // Load constant 42 and call square(42).
        let arg = builder.ins().iconst(types::I64, 42);
        let result =
            emit_extern_call(&mut builder, ext_func_ref, &[arg], false).expect("non-void return");

        builder.ins().return_(&[result]);
        builder.seal_all_blocks();
        builder.finalize();

        // Compile and define the function.
        let mut comp_ctx = cranelift_codegen::Context::for_function(func);
        ctx.module
            .define_function(wrapper_id, &mut comp_ctx)
            .expect("define wrapper");

        // Emit the object — the extern `square` should appear as an undefined
        // symbol that the linker would resolve.
        let product = ctx.module.finish();
        let bytes = product.emit().expect("emit object");
        assert!(!bytes.is_empty(), "object file should not be empty");
        assert!(bytes.len() > 16, "object should have reasonable size");
    }

    // -----------------------------------------------------------------------
    // 7. Void extern call returns None
    // -----------------------------------------------------------------------

    #[test]
    fn emit_void_extern_call() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        // Declare extern: void free(void *ptr)
        let free = ExternFunction::new(
            "free",
            vec![CType::Pointer],
            CType::Void,
            CallingConvention::Auto,
        );
        let ext_func_id =
            declare_extern(&mut ctx.module, &triple, ptr_ty, &free).expect("declare extern");

        // Build a wrapper that calls free(0).
        let mut wrapper_sig = ctx.module.make_signature();
        wrapper_sig.returns.push(AbiParam::new(types::I64));
        let wrapper_id = ctx
            .module
            .declare_function("call_free", Linkage::Export, &wrapper_sig)
            .expect("declare wrapper");

        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, wrapper_id.as_u32()),
            wrapper_sig,
        );
        let ext_func_ref = ctx.module.declare_func_in_func(ext_func_id, &mut func);

        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fb_ctx);

        let entry = builder.create_block();
        builder.switch_to_block(entry);

        let null_ptr = builder.ins().iconst(types::I64, 0);
        let result = emit_extern_call(&mut builder, ext_func_ref, &[null_ptr], true);
        assert!(result.is_none(), "void call should return None");

        // Return a dummy value since our wrapper has a return type.
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[zero]);
        builder.seal_all_blocks();
        builder.finalize();

        let mut comp_ctx = cranelift_codegen::Context::for_function(func);
        ctx.module
            .define_function(wrapper_id, &mut comp_ctx)
            .expect("define wrapper");

        let product = ctx.module.finish();
        let bytes = product.emit().expect("emit object");
        assert!(!bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // 8. Duplicate declaration is rejected
    // -----------------------------------------------------------------------

    #[test]
    fn declare_same_extern_twice_with_same_sig_succeeds() {
        // Cranelift allows re-declaring the same function with the same
        // signature — this should not error.
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        let ext = ExternFunction::new("getpid", vec![], CType::I64, CallingConvention::Auto);

        let id1 = declare_extern(&mut ctx.module, &triple, ptr_ty, &ext).expect("first declare");
        let id2 = declare_extern(&mut ctx.module, &triple, ptr_ty, &ext).expect("second declare");
        assert_eq!(id1, id2, "same signature should yield same FuncId");
    }

    // -----------------------------------------------------------------------
    // 9. Multi-param extern with mixed types
    // -----------------------------------------------------------------------

    #[test]
    fn declare_extern_mixed_types() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        // int snprintf(char *buf, size_t size, const char *fmt, ...)
        // Simplified to: (ptr, i64, ptr) -> i64
        let snprintf = ExternFunction::new(
            "snprintf",
            vec![CType::Pointer, CType::I64, CType::Pointer],
            CType::I64,
            CallingConvention::Auto,
        );

        let func_id = declare_extern(&mut ctx.module, &triple, ptr_ty, &snprintf);
        assert!(func_id.is_ok(), "mixed-type extern should succeed");
    }

    // -----------------------------------------------------------------------
    // 10. ExternFunction from LIR param type strings
    // -----------------------------------------------------------------------

    #[test]
    fn extern_from_lir_param_strings() {
        // Simulate building an ExternFunction from LIR-style type strings,
        // as would happen when lowering an `extern cell` from LIR metadata.
        let param_types: Vec<CType> = ["Int", "Float", "Bool", "addr[Byte]", "String"]
            .iter()
            .map(|s| marshal_lumen_type(s))
            .collect();

        let return_type = marshal_lumen_type("addr[Byte]");

        let ext = ExternFunction::new(
            "process_data",
            param_types.clone(),
            return_type,
            CallingConvention::Auto,
        );

        assert_eq!(ext.param_types.len(), 5);
        assert_eq!(ext.param_types[0], CType::I64);
        assert_eq!(ext.param_types[1], CType::F64);
        assert_eq!(ext.param_types[2], CType::I8);
        assert_eq!(ext.param_types[3], CType::Pointer);
        assert_eq!(ext.param_types[4], CType::Pointer);
        assert_eq!(ext.return_type, CType::Pointer);
    }

    // -----------------------------------------------------------------------
    // 11. Extern call with float parameters
    // -----------------------------------------------------------------------

    #[test]
    fn emit_extern_call_with_float_params() {
        let mut ctx = CodegenContext::new().expect("host context");
        let triple = Triple::host();
        let ptr_ty = ctx.pointer_type();

        // double pow(double base, double exp)
        let pow = ExternFunction::new(
            "pow",
            vec![CType::F64, CType::F64],
            CType::F64,
            CallingConvention::Auto,
        );
        let ext_func_id =
            declare_extern(&mut ctx.module, &triple, ptr_ty, &pow).expect("declare pow");

        let mut wrapper_sig = ctx.module.make_signature();
        wrapper_sig.returns.push(AbiParam::new(types::F64));
        let wrapper_id = ctx
            .module
            .declare_function("call_pow", Linkage::Export, &wrapper_sig)
            .expect("declare wrapper");

        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, wrapper_id.as_u32()),
            wrapper_sig,
        );
        let ext_func_ref = ctx.module.declare_func_in_func(ext_func_id, &mut func);

        let mut fb_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fb_ctx);

        let entry = builder.create_block();
        builder.switch_to_block(entry);

        let base = builder.ins().f64const(2.0);
        let exp = builder.ins().f64const(10.0);
        let result = emit_extern_call(&mut builder, ext_func_ref, &[base, exp], false)
            .expect("non-void return");

        builder.ins().return_(&[result]);
        builder.seal_all_blocks();
        builder.finalize();

        let mut comp_ctx = cranelift_codegen::Context::for_function(func);
        ctx.module
            .define_function(wrapper_id, &mut comp_ctx)
            .expect("define wrapper");

        let product = ctx.module.finish();
        let bytes = product.emit().expect("emit object");
        assert!(!bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // 12. Auto calling convention on Windows target resolves to WindowsFastcall
    // -----------------------------------------------------------------------

    #[test]
    fn auto_calling_convention_on_windows_target() {
        let triple: Triple = "x86_64-pc-windows-msvc".parse().unwrap();
        let cc = CallingConvention::Auto.to_call_conv(&triple);
        assert_eq!(
            cc,
            CallConv::WindowsFastcall,
            "Auto on Windows x86_64 should resolve to WindowsFastcall"
        );
    }
}
