use std::fs;
use std::path::PathBuf;

use lumen_compiler::compiler::resolve::ResolveError;
use lumen_compiler::{CompileError, compile};
use lumen_compiler::markdown::extract::extract_blocks;

fn repo_file(path_from_repo_root: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../").join(path_from_repo_root)
}

fn run_sweep(path_from_repo_root: &str, label: &str, expected_blocks: usize) {
    let path = repo_file(path_from_repo_root);
    let source = fs::read_to_string(&path).expect("spec file should be readable");
    let extracted = extract_blocks(&source);

    assert_eq!(
        extracted.code_blocks.len(),
        expected_blocks,
        "{} block count changed",
        label
    );

    for (idx, block) in extracted.code_blocks.iter().enumerate() {
        compile_block_with_type_stubs(&block.code).unwrap_or_else(|err| {
            panic!(
                "{} block {} failed (source line {})\n--- code ---\n{}\n--- error ---\n{}",
                label,
                idx + 1,
                block.code_start_line,
                block.code,
                err
            );
        });
    }
}

fn compile_block_with_type_stubs(code: &str) -> Result<(), String> {
    let mut stubs: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for _ in 0..8 {
        let prelude = stubs.join("\n");
        let full_code = if prelude.is_empty() {
            code.to_string()
        } else {
            format!("{}\n\n{}", prelude, code)
        };
        let md = format!("@doc_mode true\n\n# sweep\n\n```lumen\n{}\n```\n", full_code);

        match compile(&md) {
            Ok(_) => return Ok(()),
            Err(CompileError::Resolve(errors)) => {
                let mut added = 0usize;
                for err in errors {
                    if let ResolveError::UndefinedType { name, .. } = err {
                        if seen.insert(name.clone()) {
                            stubs.push(format!("record {}\nend", name));
                            added += 1;
                        }
                    } else {
                        return Err(format!("resolve errors: {:?}", err));
                    }
                }
                if added > 0 {
                    continue;
                }
                return Err("resolve failed after stubbing undefined types".to_string());
            }
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("hit max type-stub passes while compiling block".to_string())
}

#[test]
fn sweep_spec_markdown_blocks_compile() {
    run_sweep("SPEC.md", "SPEC", 125);
}

#[test]
fn sweep_spec_addendum_markdown_blocks_compile() {
    run_sweep("SPEC_ADDENDUM.md", "SPEC_ADDENDUM", 53);
}
