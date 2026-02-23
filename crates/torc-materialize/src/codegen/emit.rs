//! Object file emission and executable linking.

use std::path::{Path, PathBuf};

use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target};
use inkwell::OptimizationLevel;

use crate::error::MaterializationError;

/// Emit an object file from an LLVM module.
///
/// Initializes the appropriate LLVM target based on the triple, creates a
/// TargetMachine, and writes the module to an object file.
///
/// Returns the size in bytes of the emitted object file.
pub fn emit_object(
    module: &Module<'_>,
    triple: &str,
    cpu: &str,
    features: &str,
    opt_level: OptimizationLevel,
    output_path: &Path,
) -> Result<u64, MaterializationError> {
    // Initialize appropriate target
    init_target(triple)?;

    let target_triple = inkwell::targets::TargetTriple::create(triple);
    let target = Target::from_triple(&target_triple).map_err(|e| {
        MaterializationError::TargetInitFailed {
            target: format!("{triple}: {e}"),
        }
    })?;

    let target_machine = target
        .create_target_machine(
            &target_triple,
            cpu,
            features,
            opt_level,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| MaterializationError::TargetInitFailed {
            target: format!("failed to create TargetMachine for {triple}"),
        })?;

    // Set data layout on the module
    module.set_data_layout(&target_machine.get_target_data().get_data_layout());
    module.set_triple(&target_triple);

    target_machine
        .write_to_file(module, FileType::Object, output_path)
        .map_err(|e| MaterializationError::CodegenFailed {
            stage: "emit_object".into(),
            message: format!("failed to write object file: {e}"),
        })?;

    let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

    Ok(size)
}

/// Emit LLVM IR as a string from the module.
pub fn emit_llvm_ir(module: &Module<'_>) -> String {
    module.print_to_string().to_string()
}

/// Emit LLVM bitcode to a file.
pub fn emit_bitcode(module: &Module<'_>, output_path: &Path) -> Result<(), MaterializationError> {
    if !module.write_bitcode_to_path(output_path) {
        return Err(MaterializationError::CodegenFailed {
            stage: "emit_bitcode".into(),
            message: format!("failed to write bitcode to {}", output_path.display()),
        });
    }
    Ok(())
}

/// Link an object file into an executable using the system C compiler.
///
/// Invokes `cc -o <output> <object>` to produce a linked ELF binary.
pub fn link_executable(object_path: &Path, output_path: &Path) -> Result<(), MaterializationError> {
    let status = std::process::Command::new("cc")
        .arg("-o")
        .arg(output_path)
        .arg(object_path)
        .output()
        .map_err(|e| MaterializationError::LinkFailed {
            message: format!("failed to invoke linker (cc): {e}"),
        })?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(MaterializationError::LinkFailed {
            message: format!("linker failed: {stderr}"),
        });
    }

    Ok(())
}

/// Derive the LLVM target triple from a platform name.
pub fn platform_triple(platform_name: &str) -> &str {
    match platform_name {
        "generic-linux-x86_64" | "linux-x86_64" => "x86_64-unknown-linux-gnu",
        "linux-aarch64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu", // default fallback for Pass 2
    }
}

/// Derive the CPU name for LLVM from a platform name.
pub fn platform_cpu(platform_name: &str) -> &str {
    match platform_name {
        "generic-linux-x86_64" | "linux-x86_64" => "generic",
        "linux-aarch64" => "generic",
        _ => "generic",
    }
}

/// Initialize the LLVM target for the given triple.
fn init_target(triple: &str) -> Result<(), MaterializationError> {
    let config = InitializationConfig::default();

    if triple.starts_with("x86_64") || triple.starts_with("i686") || triple.starts_with("i386") {
        Target::initialize_x86(&config);
    } else if triple.starts_with("aarch64") || triple.starts_with("arm") {
        Target::initialize_aarch64(&config);
    } else if triple.starts_with("thumb") {
        Target::initialize_arm(&config);
    } else if triple.starts_with("riscv") {
        Target::initialize_riscv(&config);
    } else {
        // Try all targets as fallback
        Target::initialize_all(&config);
    }

    Ok(())
}

/// Helper: resolve output paths from an output directory and function name.
pub fn resolve_paths(
    output_dir: &Path,
    function_name: &str,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let object_path = output_dir.join(format!("{function_name}.o"));
    let executable_path = output_dir.join(function_name);
    let ir_path = output_dir.join(format!("{function_name}.ll"));
    let bc_path = output_dir.join(format!("{function_name}.bc"));
    (object_path, executable_path, ir_path, bc_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn emit_llvm_ir_string() {
        let context = Context::create();
        let module = context.create_module("test");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(42, false)))
            .unwrap();

        let ir = emit_llvm_ir(&module);
        assert!(ir.contains("define"));
        assert!(ir.contains("ret i32 42"));
    }

    #[test]
    fn emit_object_file() {
        let context = Context::create();
        let module = context.create_module("test_obj");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test.o");
        let size = emit_object(
            &module,
            "x86_64-unknown-linux-gnu",
            "generic",
            "",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        assert!(obj_path.exists());
        assert!(size > 0);
    }

    #[test]
    fn platform_triple_mapping() {
        assert_eq!(
            platform_triple("generic-linux-x86_64"),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(platform_triple("linux-x86_64"), "x86_64-unknown-linux-gnu");
        assert_eq!(
            platform_triple("linux-aarch64"),
            "aarch64-unknown-linux-gnu"
        );
    }

    #[test]
    fn resolve_output_paths() {
        let dir = Path::new("/tmp/test");
        let (obj, exe, ir, bc) = resolve_paths(dir, "main");
        assert_eq!(obj, Path::new("/tmp/test/main.o"));
        assert_eq!(exe, Path::new("/tmp/test/main"));
        assert_eq!(ir, Path::new("/tmp/test/main.ll"));
        assert_eq!(bc, Path::new("/tmp/test/main.bc"));
    }
}
