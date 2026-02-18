//! Build Script for Lumen Runtime Stencil Generation
//!
//! This build script implements the stencil extraction phase of the Copy-and-Patch
//! compilation strategy. It:
//!
//! 1. Compiles `src/stencils/impl.rs` to a native object file (.o)
//! 2. Parses the object file using the `object` crate
//! 3. Extracts machine code bytes for each `stencil_op_*` function
//! 4. Identifies relocations (holes) where runtime values must be patched
//! 5. Generates `src/stencils/generated.rs` with byte constants and metadata
//!
//! ## Architecture Detection
//!
//! Currently targets x86_64 only. Future support for aarch64, riscv64.
//!
//! ## Output Format
//!
//! The generated file contains:
//! - `pub const STENCIL_<OP>: &[u8]` - Raw machine code bytes
//! - `pub const STENCIL_<OP>_HOLES: &[(usize, HoleKind)]` - Relocation metadata
//!
//! ## Dependencies
//!
//! - `cc` crate: Compiles the stencil source to an object file
//! - `object` crate: Parses the object file and extracts symbols/relocations

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Trigger rebuild if stencil source changes
    println!("cargo:rerun-if-changed=src/stencils/impl.rs");

    // Get output directory for build artifacts
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let stencil_src = PathBuf::from("src/stencils/impl.rs");
    let object_file = out_dir.join("stencils.o");
    let generated_file = PathBuf::from("src/stencils/generated.rs");

    // Detect target architecture
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    // Check if stencil source exists
    if !stencil_src.exists() {
        eprintln!("Note: src/stencils/impl.rs not found, generating placeholder");
        generate_placeholder(&generated_file);
        return;
    }

    if target_arch != "x86_64" {
        // For non-x86_64 targets, generate a placeholder file
        generate_placeholder(&generated_file);
        return;
    }

    // Compile stencil source to object file using rustc
    // We use rustc instead of cc because the source is Rust (no_std)
    compile_stencil_to_object(&stencil_src, &object_file, &target_arch, &target_os);

    // Parse object file and extract stencil bytes + relocations
    match extract_stencils(&object_file) {
        Ok(stencils) => {
            // Generate Rust source file with stencil constants
            generate_stencils_file(&generated_file, &stencils);
        }
        Err(e) => {
            eprintln!("Warning: Failed to extract stencils: {}", e);
            eprintln!("Generating placeholder stencils instead.");
            generate_placeholder(&generated_file);
        }
    }
}

/// Compile the stencil implementation to a native object file
fn compile_stencil_to_object(src: &PathBuf, out: &PathBuf, arch: &str, os: &str) {
    // Use rustc to compile to object file with optimizations
    // We need --emit=obj and specific flags for position-independent code
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());

    // Get the full target triple (e.g., x86_64-unknown-linux-gnu)
    let target_triple =
        env::var("TARGET").unwrap_or_else(|_| format!("{}-unknown-{}-gnu", arch, os));

    let mut cmd = Command::new(&rustc);
    cmd.arg(src)
        .arg("--crate-type=staticlib")
        .arg("--emit=obj")
        .arg("-o")
        .arg(out)
        .arg("--target")
        .arg(&target_triple)
        .arg("-C")
        .arg("opt-level=3")
        .arg("-C")
        .arg("debuginfo=0")
        .arg("-C")
        .arg("relocation-model=pic"); // Position-independent code

    let status = cmd.status().expect("Failed to invoke rustc");

    if !status.success() {
        panic!("rustc failed to compile stencil source");
    }
}

/// Extract machine code bytes and relocations from compiled object file
fn extract_stencils(object_path: &PathBuf) -> Result<Vec<StencilData>, Box<dyn std::error::Error>> {
    use object::{Object, ObjectSection, ObjectSymbol};

    // Read object file
    let file_data = fs::read(object_path)?;
    let obj_file = object::File::parse(&*file_data)?;

    // Find all stencil functions (symbols starting with "stencil_op_")
    let mut stencils = Vec::new();

    for symbol in obj_file.symbols() {
        let name = symbol.name()?;

        // Only process stencil_op_* functions
        if !name.starts_with("stencil_op_") {
            continue;
        }

        // Extract function name (strip "stencil_op_" prefix)
        let op_name = &name[11..]; // "stencil_op_".len() == 11

        // Get symbol address and size
        let address = symbol.address();
        let size = symbol.size();

        if size == 0 {
            eprintln!("Warning: Symbol {} has zero size, skipping", name);
            continue;
        }

        // Find the section containing this symbol
        let section_index = match symbol.section() {
            object::SymbolSection::Section(idx) => idx,
            _ => {
                eprintln!("Warning: Symbol {} not in a section, skipping", name);
                continue;
            }
        };

        let section = obj_file.section_by_index(section_index)?;
        let section_data = section.data()?;

        // Extract machine code bytes for this function
        // Symbol address is relative to section start
        let start = address as usize - section.address() as usize;
        let end = start + size as usize;

        if end > section_data.len() {
            eprintln!("Warning: Symbol {} extends beyond section, skipping", name);
            continue;
        }

        let code_bytes = section_data[start..end].to_vec();

        // Extract relocations (holes) for this function
        let holes = extract_relocations(&obj_file, section_index, address, size)?;

        stencils.push(StencilData {
            op_name: op_name.to_uppercase(),
            code_bytes,
            holes,
        });
    }

    Ok(stencils)
}

/// Extract relocation entries (holes) for a given function
fn extract_relocations<'data, 'file>(
    obj_file: &'file object::File<'data>,
    section_index: object::SectionIndex,
    fn_address: u64,
    fn_size: u64,
) -> Result<Vec<(usize, String)>, Box<dyn std::error::Error>>
where
    'data: 'file,
{
    use object::{Object, ObjectSection};

    let mut holes = Vec::new();
    let section = obj_file.section_by_index(section_index)?;

    // Iterate over relocations in this section
    for (offset, relocation) in section.relocations() {
        // Check if relocation is within the function's address range
        if offset >= fn_address && offset < fn_address + fn_size {
            let hole_offset = (offset - fn_address) as usize;

            // Get relocation kind (e.g., R_X86_64_PC32, R_X86_64_PLT32)
            let kind_value = relocation.kind();
            let kind = format!("{:?}", kind_value);

            holes.push((hole_offset, kind));
        }
    }

    Ok(holes)
}

/// Data structure for a single stencil
struct StencilData {
    op_name: String,
    code_bytes: Vec<u8>,
    holes: Vec<(usize, String)>,
}

/// Generate the Rust source file with stencil constants
fn generate_stencils_file(path: &PathBuf, stencils: &[StencilData]) {
    let mut output = String::new();

    output.push_str("//! Generated Stencil Constants\n");
    output.push_str("//!\n");
    output.push_str("//! This file is auto-generated by build.rs. DO NOT EDIT BY HAND.\n");
    output.push_str("//!\n");
    output.push_str("//! Each constant contains the machine code bytes for a single LIR opcode,\n");
    output.push_str("//! extracted from the compiled stencil implementation.\n\n");
    output.push_str("#![allow(dead_code)]\n\n");

    // Generate constants for each stencil
    for stencil in stencils {
        // Generate byte array constant
        output.push_str(&format!(
            "/// Machine code stencil for {} instruction\n",
            stencil.op_name
        ));
        output.push_str(&format!("/// Length: {} bytes\n", stencil.code_bytes.len()));
        output.push_str(&format!(
            "pub const STENCIL_{}: &[u8] = &[\n",
            stencil.op_name
        ));

        // Format bytes in rows of 16
        for (i, chunk) in stencil.code_bytes.chunks(16).enumerate() {
            output.push_str("    ");
            for byte in chunk {
                output.push_str(&format!("0x{:02x}, ", byte));
            }
            if i < stencil.code_bytes.len() / 16 {
                output.push_str("\n");
            }
        }
        output.push_str("\n];\n\n");

        // Generate hole metadata if any relocations exist
        if !stencil.holes.is_empty() {
            output.push_str(&format!(
                "/// Relocation holes for {} instruction\n",
                stencil.op_name
            ));
            output.push_str(&format!("/// Format: (byte_offset, relocation_kind)\n"));
            output.push_str(&format!(
                "pub const STENCIL_{}_HOLES: &[(usize, &str)] = &[\n",
                stencil.op_name
            ));

            for (offset, kind) in &stencil.holes {
                output.push_str(&format!("    ({}, \"{}\"),\n", offset, kind));
            }

            output.push_str("];\n\n");
        } else {
            output.push_str(&format!("/// No relocations for {}\n", stencil.op_name));
            output.push_str(&format!(
                "pub const STENCIL_{}_HOLES: &[(usize, &str)] = &[];\n\n",
                stencil.op_name
            ));
        }
    }

    // Add metadata about all available stencils
    output.push_str("/// All available stencil names\n");
    output.push_str("pub const AVAILABLE_STENCILS: &[&str] = &[\n");
    for stencil in stencils {
        output.push_str(&format!("    \"{}\",\n", stencil.op_name));
    }
    output.push_str("];\n");

    fs::write(path, output).expect("Failed to write generated stencils file");
}

/// Generate a placeholder file when stencil extraction fails or is unsupported
fn generate_placeholder(path: &PathBuf) {
    let placeholder = r#"//! Generated Stencil Constants (Placeholder)
//!
//! This file is auto-generated by build.rs. DO NOT EDIT BY HAND.
//!
//! Stencil extraction is not supported on this platform or failed during build.
//! All stencils are empty placeholders.

#![allow(dead_code)]

/// Placeholder: stencil extraction not supported on this platform
pub const STENCIL_ADD: &[u8] = &[];
pub const STENCIL_ADD_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_SUB: &[u8] = &[];
pub const STENCIL_SUB_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_MUL: &[u8] = &[];
pub const STENCIL_MUL_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_DIV: &[u8] = &[];
pub const STENCIL_DIV_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_LOADK: &[u8] = &[];
pub const STENCIL_LOADK_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_MOVE: &[u8] = &[];
pub const STENCIL_MOVE_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_JMP: &[u8] = &[];
pub const STENCIL_JMP_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_TEST: &[u8] = &[];
pub const STENCIL_TEST_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_RETURN: &[u8] = &[];
pub const STENCIL_RETURN_HOLES: &[(usize, &str)] = &[];

pub const STENCIL_CALL: &[u8] = &[];
pub const STENCIL_CALL_HOLES: &[(usize, &str)] = &[];

/// No stencils available on this platform
pub const AVAILABLE_STENCILS: &[&str] = &[];
"#;

    fs::write(path, placeholder).expect("Failed to write placeholder stencils file");
}
