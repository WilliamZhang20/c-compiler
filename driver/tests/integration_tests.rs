use std::process::Command;
use std::path::Path;
use std::fs;

#[test]
fn run_all_c_tests() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let workspace_root = Path::new(&manifest_dir).parent().expect("Failed to get workspace root");
    let testing_dir = workspace_root.join("testing");
    let driver_path = workspace_root.join("target").join("debug").join("driver.exe");

    // Ensure driver is built
    let status = Command::new("cargo")
        .args(&["build", "--bin", "driver"])
        .current_dir(&workspace_root)
        .status()
        .expect("Failed to build driver");
    assert!(status.success(), "Driver build failed");

    let mut tests_failed = 0;
    let mut tests_run = 0;

    for entry in fs::read_dir(&testing_dir).expect("Failed to read testing dir") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("c") {
            let file_name = path.file_name().unwrap().to_str().unwrap();

            // Read source to find expected exit code
            let source = fs::read_to_string(&path).expect("Failed to read source");
            let expected_code = match parse_expected_code(&source) {
                Some(code) => code,
                None => {
                    println!("⏭️ Skipping {} (no // EXPECT annotation)", file_name);
                    continue;
                }
            };

            tests_run += 1;
            println!("Running test: {}", file_name);

            // Compile (run from workspace root so runtime/malloc.o is found)
            let compile_status = Command::new(&driver_path)
                .arg(&path)
                .current_dir(&workspace_root)
                .status()
                .expect("Failed to run driver");
            
            if !compile_status.success() {
                println!("❌ Compilation failed for {}", file_name);
                tests_failed += 1;
                // Halt on first failure
                break;
            }

            // Run executable (generated in workspace root)
            let exe_name = path.file_stem().unwrap().to_str().unwrap().to_string() + ".exe";
            let exe_path = workspace_root.join(&exe_name);
            
            let run_status = Command::new(&exe_path)
                .status()
                .expect("Failed to run generated executable");

            let exit_code = run_status.code().unwrap_or(0);

            if exit_code == expected_code {
                println!("✅ Passed: {} (Exit code {})", file_name, exit_code);
            } else {
                println!("❌ Failed: {} (Expected {}, Got {})", file_name, expected_code, exit_code);
                tests_failed += 1;
                // Halt on first failure
                break;
            }
            
            // Clean up exe
            let _ = fs::remove_file(exe_path);
        }
    }

    println!("\n{} tests run, {} passed, {} failed", tests_run, tests_run - tests_failed, tests_failed);
    assert_eq!(tests_failed, 0, "{} tests failed", tests_failed);
}

fn parse_expected_code(source: &str) -> Option<i32> {
    for line in source.lines() {
        if let Some(rest) = line.trim().strip_prefix("// EXPECT:") {
            return rest.trim().parse().ok();
        }
    }
    None
}
