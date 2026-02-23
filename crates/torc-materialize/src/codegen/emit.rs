//! Object file emission and executable linking.

use std::path::{Path, PathBuf};

use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target};
use inkwell::OptimizationLevel;

use torc_targets::{EnvironmentModel, EnvironmentType, MemoryRegion};

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

/// Derive the LLVM target triple from a platform name.
pub fn platform_triple(platform_name: &str) -> &str {
    match platform_name {
        "generic-linux-x86_64" | "linux-x86_64" => "x86_64-unknown-linux-gnu",
        "linux-aarch64" => "aarch64-unknown-linux-gnu",
        "stm32f407-discovery" => "thumbv7em-none-eabihf",
        other => {
            eprintln!(
                "warning: unknown platform '{other}', defaulting to x86_64-unknown-linux-gnu"
            );
            "x86_64-unknown-linux-gnu"
        }
    }
}

/// Derive the CPU name for LLVM from a platform name.
pub fn platform_cpu(platform_name: &str) -> &str {
    match platform_name {
        "generic-linux-x86_64" | "linux-x86_64" => "generic",
        "linux-aarch64" => "generic",
        "stm32f407-discovery" => "cortex-m4",
        _ => "generic", // unknown platforms warned in platform_triple()
    }
}

/// Derive the LLVM feature string from a platform name and ISA extensions.
///
/// Known platforms get hardcoded feature strings. For unknown platforms,
/// ISA extension names are mapped to LLVM feature flags where possible.
pub fn platform_features(platform_name: &str, isa_extensions: &[String]) -> String {
    // Known platforms with hardcoded feature strings
    match platform_name {
        "stm32f407-discovery" => return "+vfp4sp-d16,+thumb-mode".to_string(),
        _ => {}
    }

    // Generic fallback: map ISA extension names to LLVM flags
    let flags: Vec<&str> = isa_extensions
        .iter()
        .filter_map(|ext| match ext.as_str() {
            "NEON" => Some("+neon"),
            "FP" => Some("+fp-armv8"),
            "SSE2" => Some("+sse2"),
            "SSE4.2" => Some("+sse4.2"),
            "Thumb2" => Some("+thumb-mode"),
            "FPv5" => Some("+vfp4sp-d16"),
            _ => None, // unknown extensions silently skipped
        })
        .collect();

    flags.join(",")
}

/// Link an object file into an executable, selecting the appropriate cross-linker.
///
/// For bare-metal targets, generates a linker script from the environment's memory
/// regions and passes `-nostdlib -T <script>` to the linker.
pub fn link_executable(
    object_path: &Path,
    output_path: &Path,
    platform_name: &str,
    environment: &EnvironmentModel,
) -> Result<(), MaterializationError> {
    let linker = select_cross_linker(platform_name, &environment.env_type);
    let is_bare_metal = environment.env_type == EnvironmentType::BareMetal;

    let mut cmd = std::process::Command::new(&linker);
    cmd.arg("-o").arg(output_path);

    if is_bare_metal {
        // Generate linker script next to the object file
        let script_path = object_path.with_extension("ld");
        let script = generate_linker_script(&environment.memory_regions);
        std::fs::write(&script_path, &script).map_err(|e| MaterializationError::LinkFailed {
            message: format!("failed to write linker script: {e}"),
        })?;
        cmd.arg("-nostdlib").arg("-T").arg(&script_path);
    }

    cmd.arg(object_path);

    let output = cmd.output().map_err(|e| MaterializationError::LinkFailed {
        message: format!("failed to invoke linker ({linker}): {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MaterializationError::LinkFailed {
            message: format!("linker failed: {stderr}"),
        });
    }

    Ok(())
}

/// Select the cross-compiler/linker for a given platform.
///
/// Returns `"cc"` for native targets, and appropriate cross-compilers for
/// known cross-compilation targets. Falls back to `"cc"` if the preferred
/// cross-compiler is not found on PATH.
pub fn select_cross_linker(platform_name: &str, env_type: &EnvironmentType) -> String {
    let preferred = match platform_name {
        "linux-aarch64" => "aarch64-linux-gnu-gcc",
        "stm32f407-discovery" => "arm-none-eabi-gcc",
        _ if *env_type == EnvironmentType::BareMetal => "arm-none-eabi-gcc",
        _ => return "cc".to_string(),
    };

    if which_exists(preferred) {
        preferred.to_string()
    } else {
        "cc".to_string()
    }
}

/// Check if a command exists on PATH using a portable Rust implementation.
fn which_exists(cmd: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(separator) {
            let candidate = Path::new(dir).join(cmd);
            if candidate.is_file() {
                return true;
            }
            // On Windows, also check with .exe extension
            if cfg!(windows) {
                let candidate_exe = Path::new(dir).join(format!("{cmd}.exe"));
                if candidate_exe.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

/// Generate a linker script from memory regions for bare-metal targets.
///
/// Produces a standard GNU LD script with MEMORY and SECTIONS blocks.
pub fn generate_linker_script(memory_regions: &[MemoryRegion]) -> String {
    let mut script = String::new();
    script.push_str("ENTRY(main)\n\n");

    // MEMORY block
    script.push_str("MEMORY\n{\n");
    for region in memory_regions {
        // Validate region name contains only valid linker script identifier characters
        let safe_name: String = region
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let safe_name = if safe_name.is_empty() {
            "REGION".to_string()
        } else {
            safe_name
        };
        let mut attrs = String::new();
        if region.readable {
            attrs.push('r');
        }
        if region.writable {
            attrs.push('w');
        }
        if region.executable {
            attrs.push('x');
        }
        script.push_str(&format!(
            "    {} ({}) : ORIGIN = 0x{:08X}, LENGTH = 0x{:X}\n",
            safe_name, attrs, region.base_address, region.size_bytes
        ));
    }
    script.push_str("}\n\n");

    // Find first executable region and first writable region (sanitized names)
    let sanitize = |name: &str| -> String {
        let s: String = name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if s.is_empty() {
            "REGION".to_string()
        } else {
            s
        }
    };
    let flash_region = memory_regions
        .iter()
        .find(|r| r.executable)
        .map(|r| sanitize(&r.name))
        .unwrap_or_else(|| "FLASH".to_string());
    let ram_region = memory_regions
        .iter()
        .find(|r| r.writable && !r.executable)
        .map(|r| sanitize(&r.name))
        .unwrap_or_else(|| "SRAM".to_string());

    // SECTIONS block
    script.push_str("SECTIONS\n{\n");
    script.push_str(&format!("    .text : {{ *(.text*) }} > {flash_region}\n"));
    script.push_str(&format!(
        "    .rodata : {{ *(.rodata*) }} > {flash_region}\n"
    ));
    script.push_str(&format!(
        "    .data : {{ *(.data*) }} > {ram_region} AT > {flash_region}\n"
    ));
    script.push_str(&format!("    .bss : {{ *(.bss*) }} > {ram_region}\n"));
    script.push_str(&format!(
        "    .stack : {{ . = ALIGN(8); _stack_top = .; }} > {ram_region}\n"
    ));
    script.push_str("}\n");

    script
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

    #[test]
    fn platform_triple_stm32() {
        assert_eq!(
            platform_triple("stm32f407-discovery"),
            "thumbv7em-none-eabihf"
        );
    }

    #[test]
    fn platform_cpu_stm32() {
        assert_eq!(platform_cpu("stm32f407-discovery"), "cortex-m4");
    }

    #[test]
    fn platform_features_stm32() {
        let exts: Vec<String> = vec!["Thumb2".into(), "FPv5".into()];
        assert_eq!(
            platform_features("stm32f407-discovery", &exts),
            "+vfp4sp-d16,+thumb-mode"
        );
    }

    #[test]
    fn platform_features_aarch64() {
        let exts: Vec<String> = vec!["NEON".into(), "FP".into()];
        assert_eq!(platform_features("linux-aarch64", &exts), "+neon,+fp-armv8");
    }

    #[test]
    fn platform_features_x86_64() {
        let exts: Vec<String> = vec!["SSE2".into(), "SSE4.2".into()];
        assert_eq!(
            platform_features("generic-linux-x86_64", &exts),
            "+sse2,+sse4.2"
        );
    }

    #[test]
    fn platform_features_unknown_skipped() {
        let exts: Vec<String> = vec!["UnknownExt".into(), "AnotherUnknown".into()];
        assert_eq!(platform_features("some-platform", &exts), "");
    }

    #[test]
    fn emit_object_aarch64() {
        let context = Context::create();
        let module = context.create_module("test_aarch64");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_aarch64.o");
        let size = emit_object(
            &module,
            "aarch64-unknown-linux-gnu",
            "generic",
            "+neon,+fp-armv8",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        assert!(obj_path.exists());
        assert!(size > 0);
    }

    #[test]
    fn emit_object_thumbv7em() {
        let context = Context::create();
        let module = context.create_module("test_stm32");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_stm32.o");
        let size = emit_object(
            &module,
            "thumbv7em-none-eabihf",
            "cortex-m4",
            "+vfp4sp-d16,+thumb-mode",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        assert!(obj_path.exists());
        assert!(size > 0);
    }

    #[test]
    fn emit_object_aarch64_is_elf() {
        let context = Context::create();
        let module = context.create_module("test_elf_aarch64");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_elf_aarch64.o");
        emit_object(
            &module,
            "aarch64-unknown-linux-gnu",
            "generic",
            "+neon,+fp-armv8",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        let bytes = std::fs::read(&obj_path).unwrap();
        // ELF magic: 0x7f 'E' 'L' 'F'
        assert_eq!(&bytes[..4], &[0x7f, 0x45, 0x4c, 0x46]);
    }

    #[test]
    fn emit_object_stm32_is_elf32() {
        let context = Context::create();
        let module = context.create_module("test_elf_stm32");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_elf_stm32.o");
        emit_object(
            &module,
            "thumbv7em-none-eabihf",
            "cortex-m4",
            "+vfp4sp-d16,+thumb-mode",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        let bytes = std::fs::read(&obj_path).unwrap();
        // ELF magic
        assert_eq!(&bytes[..4], &[0x7f, 0x45, 0x4c, 0x46]);
        // ELF class byte == 1 (32-bit)
        assert_eq!(bytes[4], 1);
    }

    #[test]
    fn generate_linker_script_stm32() {
        let regions = vec![
            MemoryRegion {
                name: "FLASH".into(),
                base_address: 0x0800_0000,
                size_bytes: 1024 * 1024,
                readable: true,
                writable: false,
                executable: true,
            },
            MemoryRegion {
                name: "SRAM".into(),
                base_address: 0x2000_0000,
                size_bytes: 128 * 1024,
                readable: true,
                writable: true,
                executable: false,
            },
        ];

        let script = generate_linker_script(&regions);
        assert!(script.contains("ENTRY(main)"));
        assert!(script.contains("MEMORY"));
        assert!(script.contains("SECTIONS"));
        assert!(script.contains("FLASH"));
        assert!(script.contains("SRAM"));
        assert!(script.contains("0x08000000"));
        assert!(script.contains("0x20000000"));
        assert!(script.contains(".text"));
        assert!(script.contains(".data"));
        assert!(script.contains(".bss"));
        assert!(script.contains(".stack"));
        assert!(script.contains("> FLASH"));
        assert!(script.contains("> SRAM"));
    }

    #[test]
    fn generate_linker_script_all_regions() {
        let regions = vec![
            MemoryRegion {
                name: "FLASH".into(),
                base_address: 0x0800_0000,
                size_bytes: 1024 * 1024,
                readable: true,
                writable: false,
                executable: true,
            },
            MemoryRegion {
                name: "SRAM".into(),
                base_address: 0x2000_0000,
                size_bytes: 128 * 1024,
                readable: true,
                writable: true,
                executable: false,
            },
            MemoryRegion {
                name: "CCMRAM".into(),
                base_address: 0x1000_0000,
                size_bytes: 64 * 1024,
                readable: true,
                writable: true,
                executable: false,
            },
        ];

        let script = generate_linker_script(&regions);
        assert!(script.contains("CCMRAM"));
        assert!(script.contains("0x10000000"));
    }

    #[test]
    fn select_cross_linker_native() {
        assert_eq!(
            select_cross_linker("generic-linux-x86_64", &EnvironmentType::Linux),
            "cc"
        );
    }

    #[test]
    #[ignore] // Requires aarch64-linux-gnu-gcc toolchain
    fn link_executable_aarch64_cross() {
        let context = Context::create();
        let module = context.create_module("test_link_aarch64");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_link_aarch64.o");
        let exe_path = dir.path().join("test_link_aarch64");
        emit_object(
            &module,
            "aarch64-unknown-linux-gnu",
            "generic",
            "+neon,+fp-armv8",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        let env = torc_targets::EnvironmentModel::linux_x86_64();
        let mut aarch64_env = env;
        aarch64_env.env_type = EnvironmentType::Linux;
        link_executable(&obj_path, &exe_path, "linux-aarch64", &aarch64_env).unwrap();
        assert!(exe_path.exists());
    }

    #[test]
    #[ignore] // Requires arm-none-eabi-gcc toolchain
    fn link_executable_stm32_bare_metal() {
        let context = Context::create();
        let module = context.create_module("test_link_stm32");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("test_link_stm32.o");
        let exe_path = dir.path().join("test_link_stm32");
        emit_object(
            &module,
            "thumbv7em-none-eabihf",
            "cortex-m4",
            "+vfp4sp-d16,+thumb-mode",
            OptimizationLevel::None,
            &obj_path,
        )
        .unwrap();

        let env = torc_targets::EnvironmentModel::bare_metal_arm();
        link_executable(&obj_path, &exe_path, "stm32f407-discovery", &env).unwrap();
        assert!(exe_path.exists());
    }
}
