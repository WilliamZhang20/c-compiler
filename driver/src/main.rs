use clap::Parser; // clap crate for CLI argument parsing
use std::{path::Path, process::Command};
use std::sync::OnceLock;

static DEBUG_ENABLED: OnceLock<bool> = OnceLock::new();

macro_rules! log {
    ($($arg:tt)*) => {{
        if *DEBUG_ENABLED.get().unwrap_or(&false) {
            let msg = format!($($arg)*);
            eprintln!("{}", msg);
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("debug_driver.log") {
                let _ = writeln!(file, "{}", msg);
            }
        }
    }};
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the C source file
    input_path: String,

    /// Run lexer only
    #[arg(short, long)]
    lex: bool,

    /// Run lexer and parser only
    #[arg(short, long)]
    parse: bool,

    /// Run lexer, parser, and codegen only
    #[arg(short, long)]
    codegen: bool,

    /// Emit assembly but do not assemble or link
    #[arg(short = 'S', long)]
    emit_asm: bool,

    /// Keep intermediate files (.i, .s)
    #[arg(long, default_value_t = false)]
    keep_intermediates: bool,

    /// Use safe malloc runtime (detects buffer overflows, use-after-free, etc.)
    #[arg(long, default_value_t = false)]
    safe_malloc: bool,

    /// Enable debug output
    #[arg(short, long, default_value_t = false)]
    debug: bool,
}

const MALLOC_C_SOURCE: &str = include_str!("../../runtime/malloc.c");

fn main() {
    let args = Args::parse();
    DEBUG_ENABLED.set(args.debug).ok();
    
    log!("DEBUG: Driver started");
    log!("DEBUG: Args parsed");

    let stop_after_emit_asm = args.emit_asm;
    let stop_after_codegen = args.codegen;
    let stop_after_parse = args.parse;
    let stop_after_lex = args.lex;
    let keep_intermediates = args.keep_intermediates || stop_after_emit_asm;

    log!("DEBUG: Checking gcc...");
    // Check for gcc
    if Command::new("gcc").arg("--version").output().is_err() {
        eprintln!("Error: 'gcc' not found in PATH. Please install GCC.");
        std::process::exit(1);
    }
    log!("DEBUG: GCC check passed");

    let input_path = args.input_path.clone();
    let input_file = Path::new(&input_path);
    if !input_file.exists() {
         log!("Error: Input file '{}' not found.", input_path);
         std::process::exit(1);
    }

    log!("Step 1: Preprocessing...");
    let preprocessed_path = preprocess(&input_path, input_file);
    log!("Step 1: Done");

    let cleanup = |path: &str| {
        if !keep_intermediates {
            let _ = std::fs::remove_file(path);
        }
    };

    let src = std::fs::read_to_string(&preprocessed_path).expect("failed to read preprocessed file");

    log!("Step 2: Lexing...");
    let tokens = lexer::lex(&src).expect("Lexing failed");
    log!("Step 2: Done");
    
    if stop_after_lex {
        println!("Tokens: {:?}", tokens);
        cleanup(&preprocessed_path);
        return;
    }

    log!("Step 3: Parsing...");
    let mut program = parser::parse_tokens(&tokens).expect("Parsing failed");
    log!("Step 3: Done");
    
    // Deduplicate global variables (common with extern declarations)
    {
        let mut seen = std::collections::HashSet::new();
        program.globals.retain(|g| seen.insert(g.name.clone()));
    }
    
    if stop_after_parse {
        println!("AST: {:?}", program);
        cleanup(&preprocessed_path);
        return;
    }

    log!("Step 4: Semantic Analysis...");
    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&program).expect("Semantic analysis failed");
    log!("Step 4: Done");

    log!("Step 5: IR Lowering...");
    let mut lowerer = ir::Lowerer::new();
    let ir_prog = lowerer.lower_program(&program).expect("IR lowering failed");
    log!("Step 5: Done");
    
    log!("Step 6: Optimization...");
    let ir_prog = optimizer::optimize(ir_prog);
    log!("Step 6: Done");

    if stop_after_codegen {
        println!("IR: {:?}", ir_prog);
        cleanup(&preprocessed_path);
        return;
    }

    log!("Step 7: Code Generation...");
    let mut codegen = codegen::Codegen::new();
    let asm = codegen.gen_program(&ir_prog);
    log!("Step 7: Done");

    let mut asm_path = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    asm_path.push_str(".s");
    std::fs::write(&asm_path, asm).expect("failed to write assembly file");

    if stop_after_emit_asm {
        cleanup(&preprocessed_path);
        return;
    }

    log!("Step 8: Linking...");
    run_linker(&input_file, &asm_path, args.safe_malloc);
    log!("Step 8: Done");
    println!("Compilation successful. Generated executable: {}", input_file.file_stem().unwrap().to_string_lossy());

    // Cleanup
    cleanup(&preprocessed_path);
    cleanup(&asm_path);
}

fn preprocess(input_path: &str, input_file: &Path) -> String {
    let mut preprocessed_path = input_file.file_stem().unwrap().to_string_lossy().to_string();
    preprocessed_path.push_str(".i");

    let exit_code = Command::new("gcc")
        .args(["-E", "-P", "-Iinclude", input_path, "-o", &preprocessed_path])
        .status()
        .expect("failed to execute process");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc preprocess command failed with exit code {}", code);
        }
        panic!("gcc preprocess command was terminated by a signal");
    }
    preprocessed_path
}

fn run_linker(input_file: &Path, asm_path: &str, use_safe_malloc: bool) {
    let mut executable_file = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    executable_file.push_str(".exe");

    let mut args = vec![asm_path.to_string()];
    let mut temp_malloc_o = None;
    let mut temp_malloc_c = None;
    
    // Optionally link with safe malloc runtime
    if use_safe_malloc {
        // Try to find local runtime/malloc.o first (development mode)
        let local_malloc = Path::new("runtime/malloc.o");
        if local_malloc.exists() {
            args.push("runtime/malloc.o".to_string());
        } else {
            // "Run anywhere" mode: write embedded source to temp file and compile
            let temp_dir = std::env::temp_dir();
            let unique_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
            
            let c_path = temp_dir.join(format!("malloc_{}.c", unique_id));
            let o_path = temp_dir.join(format!("malloc_{}.o", unique_id));
            
            std::fs::write(&c_path, MALLOC_C_SOURCE).expect("Failed to write temp malloc.c");
            
            // Compile malloc.c -> malloc.o
            let status = Command::new("gcc")
                .args(["-c", c_path.to_str().unwrap(), "-o", o_path.to_str().unwrap()])
                .status()
                .expect("Failed to compile embedded malloc.c");
                
            if !status.success() {
                panic!("Failed to compile embedded malloc runtime");
            }
            
            args.push(o_path.to_str().unwrap().to_string());
            temp_malloc_c = Some(c_path);
            temp_malloc_o = Some(o_path);
        }
    }
    
    args.push("-o".to_string());
    args.push(executable_file.clone());
    args.push("-mconsole".to_string());

    let exit_code = Command::new("gcc")
        .args(&args)
        .status()
        .expect("executable generated sucessfully");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc compilation failed with exit code {}", code);
        }
        panic!("gcc compilation was terminated by a signal");
    }
    
    // Clean up temporary malloc files
    if let Some(p) = temp_malloc_c { let _ = std::fs::remove_file(p); }
    if let Some(p) = temp_malloc_o { let _ = std::fs::remove_file(p); }
}
