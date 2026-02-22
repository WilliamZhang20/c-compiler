/// In-process integration tests that run the full compiler pipeline
/// (lex → parse → semantic → IR → optimize → codegen → assemble → run)
/// within the test binary, so tarpaulin can trace line coverage through
/// every crate.
///
/// Tests that require #include are preprocessed via gcc -E first.
/// Tests without #include are compiled directly from source.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Run the full compiler pipeline in-process on a C source string,
/// returning the generated assembly.
fn compile_source(src: &str) -> Result<String, String> {
    let tokens = lexer::lex(src).map_err(|e| format!("Lex error: {:?}", e))?;
    let mut program = parser::parse_tokens(&tokens).map_err(|e| format!("Parse error: {:?}", e))?;

    // Deduplicate globals (same as driver/src/main.rs)
    {
        let mut seen = std::collections::HashSet::new();
        program.globals.retain(|g| seen.insert(g.name.clone()));
    }

    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&program).map_err(|e| format!("Semantic error: {:?}", e))?;

    let mut lowerer = ir::Lowerer::new();
    let ir_prog = lowerer.lower_program(&program).map_err(|e| format!("IR error: {:?}", e))?;

    let ir_prog = optimizer::optimize(ir_prog);

    let mut cg = codegen::Codegen::new();
    let asm = cg.gen_program(&ir_prog);
    Ok(asm)
}

/// Compile C source to an executable, run it, and return exit code.
fn compile_and_run(src: &str, test_name: &str) -> Result<i32, String> {
    let asm = compile_source(src)?;

    let tmp_dir = std::env::temp_dir();
    let asm_path = tmp_dir.join(format!("{}.s", test_name));
    let exe_path = tmp_dir.join(test_name);

    fs::write(&asm_path, &asm).map_err(|e| format!("Write asm: {}", e))?;

    // Assemble and link with gcc
    let status = Command::new("gcc")
        .args(&[
            asm_path.to_str().unwrap(),
            "-o",
            exe_path.to_str().unwrap(),
            "-no-pie",
        ])
        .status()
        .map_err(|e| format!("gcc: {}", e))?;

    if !status.success() {
        return Err(format!("gcc assemble/link failed for {}", test_name));
    }

    // Run
    let run_status = Command::new(&exe_path)
        .status()
        .map_err(|e| format!("run: {}", e))?;

    // Cleanup
    let _ = fs::remove_file(&asm_path);
    let _ = fs::remove_file(&exe_path);

    Ok(run_status.code().unwrap_or(-1))
}

/// Parse `// EXPECT: <code>` from source
fn parse_expected(src: &str) -> Option<i32> {
    for line in src.lines() {
        if let Some(rest) = line.trim().strip_prefix("// EXPECT:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

/// Preprocess a C file with gcc -E, returning the preprocessed source
fn preprocess(path: &Path) -> Result<String, String> {
    let output = Command::new("gcc")
        .args(&["-E", "-P", path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("gcc -E failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Preprocessing failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Files that require preprocessing (#include) — these go through gcc -E
const NEEDS_PREPROCESS: &[&str] = &[
    "test_escape_sequences.c",
    "test_function_pointer_debug.c",
    "test_include.c",
    "test_increment.c",
    "test_interrupt_handler.c",
    "test_malloc.c",
    "test_signal.c",
];

/// Files to skip entirely (need features not supported in-process)
const SKIP_FILES: &[&str] = &[
    "test_variadic_intrinsics.c", // va_list/va_start from <stdarg.h> not fully supported after gcc -E
];

#[test]
fn run_all_c_tests_in_process() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let workspace_root = Path::new(&manifest_dir)
        .parent()
        .expect("Failed to get workspace root");
    let testing_dir = workspace_root.join("testing");

    let mut passed = 0;
    let mut skipped = 0;
    let mut failed = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(&testing_dir)
        .expect("Failed to read testing dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                == Some("c")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_str().unwrap();

        // Skip files that can't be tested in-process
        if SKIP_FILES.contains(&file_name) {
            skipped += 1;
            continue;
        }

        // Read source
        let raw_src = fs::read_to_string(&path).expect("Failed to read source");

        // Need EXPECT annotation
        let expected_code = match parse_expected(&raw_src) {
            Some(code) => code,
            None => {
                skipped += 1;
                continue;
            }
        };

        // Get the source to compile (preprocess if needed)
        let src = if NEEDS_PREPROCESS.contains(&file_name) {
            match preprocess(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  Skip {} (preprocess failed: {})", file_name, e);
                    skipped += 1;
                    continue;
                }
            }
        } else {
            raw_src.clone()
        };

        let test_name = path.file_stem().unwrap().to_str().unwrap();
        match compile_and_run(&src, test_name) {
            Ok(exit_code) if exit_code == expected_code => {
                passed += 1;
            }
            Ok(exit_code) => {
                eprintln!(
                    "  FAIL {}: expected exit {}, got {}",
                    file_name, expected_code, exit_code
                );
                failed.push(file_name.to_string());
            }
            Err(e) => {
                eprintln!("  FAIL {}: {}", file_name, e);
                failed.push(file_name.to_string());
            }
        }
    }

    println!(
        "\nIn-process integration: {} passed, {} skipped, {} failed",
        passed,
        skipped,
        failed.len()
    );

    if !failed.is_empty() {
        panic!(
            "{} tests failed:\n  {}",
            failed.len(),
            failed.join("\n  ")
        );
    }
}
