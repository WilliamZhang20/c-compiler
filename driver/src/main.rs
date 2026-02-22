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
    /// Path(s) to the C source file(s)
    input_paths: Vec<String>,

    /// Output executable name
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Run lexer only
    #[arg(short, long)]
    lex: bool,

    /// Run lexer and parser only
    #[arg(short, long)]
    parse: bool,

    /// Run lexer, parser, and codegen only
    #[arg(long)]
    codegen: bool,

    /// Emit assembly but do not assemble or link
    #[arg(short = 'S', long)]
    emit_asm: bool,

    /// Compile and assemble but do not link (produce .o files)
    #[arg(short = 'c')]
    compile_only: bool,

    /// Keep intermediate files (.i, .s)
    #[arg(long, default_value_t = false)]
    keep_intermediates: bool,

    /// Enable debug output
    #[arg(short, long, default_value_t = false)]
    debug: bool,

    /// Preprocessor macro definitions (-DNAME or -DNAME=VALUE)
    #[arg(short = 'D', value_name = "MACRO")]
    defines: Vec<String>,

    /// Undefine preprocessor macros (-UNAME)
    #[arg(short = 'U', value_name = "MACRO")]
    undefines: Vec<String>,

    /// Additional include paths (-Ipath)
    #[arg(short = 'I', value_name = "PATH")]
    include_paths: Vec<String>,

    /// Force-include a header file (-include file)
    #[arg(long = "include", value_name = "FILE")]
    force_includes: Vec<String>,

    /// Build without standard library
    #[arg(long)]
    nostdlib: bool,

    /// Freestanding environment (no hosted assumptions)
    #[arg(long)]
    ffreestanding: bool,
}

fn main() {
    let args = Args::parse();
    DEBUG_ENABLED.set(args.debug).ok();
    
    log!("DEBUG: Driver started");
    log!("DEBUG: Args parsed");

    if args.input_paths.is_empty() {
        eprintln!("Error: No input files provided.");
        std::process::exit(1);
    }

    let stop_after_emit_asm = args.emit_asm;
    let stop_after_codegen = args.codegen;
    let stop_after_parse = args.parse;
    let stop_after_lex = args.lex;
    let compile_only = args.compile_only;
    let nostdlib = args.nostdlib;
    let ffreestanding = args.ffreestanding;
    let keep_intermediates = args.keep_intermediates || stop_after_emit_asm;

    // Build extra preprocessor flags from -D, -U, -I, -include
    let mut cpp_extra_args = Vec::new();
    for d in &args.defines {
        cpp_extra_args.push(format!("-D{}", d));
    }
    for u in &args.undefines {
        cpp_extra_args.push(format!("-U{}", u));
    }
    for i in &args.include_paths {
        cpp_extra_args.push(format!("-I{}", i));
    }
    for inc in &args.force_includes {
        cpp_extra_args.push("-include".to_string());
        cpp_extra_args.push(inc.clone());
    }
    if ffreestanding {
        cpp_extra_args.push("-ffreestanding".to_string());
    }

    log!("DEBUG: Checking gcc...");
    // Check for gcc
    if Command::new("gcc").arg("--version").output().is_err() {
        eprintln!("Error: 'gcc' not found in PATH. Please install GCC.");
        std::process::exit(1);
    }
    log!("DEBUG: GCC check passed");

    let cleanup = |path: &str| {
        if !keep_intermediates {
            let _ = std::fs::remove_file(path);
        }
    };

    let mut asm_paths = Vec::new();
    let mut preprocessed_paths = Vec::new();

    // Process each input file
    for input_path in &args.input_paths {
        let input_file = Path::new(&input_path);
        if !input_file.exists() {
             eprintln!("Error: Input file '{}' not found.", input_path);
             std::process::exit(1);
        }

        log!("Processing file: {}", input_path);
        log!("Step 1: Preprocessing...");
        let preprocessed_path = preprocess(&input_path, input_file, &cpp_extra_args);
        log!("Step 1: Done");

        let src = std::fs::read_to_string(&preprocessed_path).expect("failed to read preprocessed file");

        log!("Step 2: Lexing...");
        let tokens = lexer::lex(&src).expect("Lexing failed");
        log!("Step 2: Done");
        
        if stop_after_lex {
            println!("Tokens for {}: {:?}", input_path, tokens);
            preprocessed_paths.push(preprocessed_path);
            continue;
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
            println!("AST for {}: {:?}", input_path, program);
            preprocessed_paths.push(preprocessed_path);
            continue;
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
            println!("IR for {}: {:?}", input_path, ir_prog);
            preprocessed_paths.push(preprocessed_path);
            continue;
        }

        log!("Step 7: Code Generation...");
        let mut codegen = codegen::Codegen::new();
        let asm = codegen.gen_program(&ir_prog);
        log!("Step 7: Done");

        let mut asm_path = input_file.file_stem().unwrap().to_string_lossy().into_owned();
        asm_path.push_str(".s");
        std::fs::write(&asm_path, asm).expect("failed to write assembly file");

        asm_paths.push(asm_path);
        preprocessed_paths.push(preprocessed_path);
    }

    if stop_after_lex || stop_after_parse || stop_after_codegen {
        for path in preprocessed_paths {
            cleanup(&path);
        }
        return;
    }

    if stop_after_emit_asm {
        for path in preprocessed_paths {
            cleanup(&path);
        }
        return;
    }

    // -c: assemble each .s to .o, skip linking
    if compile_only {
        for asm_path in &asm_paths {
            let obj_path = if let Some(ref out) = args.output {
                // -o overrides output name (only valid for single file)
                out.clone()
            } else {
                asm_path.replace(".s", ".o")
            };
            assemble(asm_path, &obj_path);
        }
        for path in preprocessed_paths {
            cleanup(&path);
        }
        for path in asm_paths {
            cleanup(&path);
        }
        return;
    }

    // Determine output executable name
    let output_name = if let Some(name) = args.output {
        name
    } else {
        // Default: use first input file's stem
        let first_input = Path::new(&args.input_paths[0]);
        let platform = model::Platform::host();
        let mut name = first_input.file_stem().unwrap().to_string_lossy().into_owned();
        name.push_str(platform.executable_extension());
        name
    };

    log!("Step 8: Linking...");
    run_linker(&asm_paths, &output_name, nostdlib, ffreestanding);
    log!("Step 8: Done");
    println!("Compilation successful. Generated executable: {}", output_name);

    // Cleanup
    for path in preprocessed_paths {
        cleanup(&path);
    }
    for path in asm_paths {
        cleanup(&path);
    }
}

fn preprocess(input_path: &str, input_file: &Path, extra_args: &[String]) -> String {
    let mut preprocessed_path = input_file.file_stem().unwrap().to_string_lossy().to_string();
    preprocessed_path.push_str(".i");

    let mut cmd = Command::new("gcc");
    cmd.args(["-E", "-P", "-Iinclude"]);
    
    // Forward extra preprocessor flags (-D, -U, -I, -include)
    for arg in extra_args {
        cmd.arg(arg);
    }
    
    cmd.args([input_path, "-o", &preprocessed_path]);

    let exit_code = cmd
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

fn assemble(asm_path: &str, obj_path: &str) {
    let exit_code = Command::new("gcc")
        .args(["-c", asm_path, "-o", obj_path])
        .status()
        .expect("failed to run gcc assembler");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc assembly failed with exit code {}", code);
        }
        panic!("gcc assembly was terminated by a signal");
    }
}

fn run_linker(asm_paths: &[String], output_file: &str, nostdlib: bool, ffreestanding: bool) {
    let platform = model::Platform::host();

    let mut args = Vec::new();
    
    // Add all assembly files
    for asm_path in asm_paths {
        args.push(asm_path.clone());
    }
    
    args.push("-o".to_string());
    args.push(output_file.to_string());
    
    // Add platform-specific linker flags
    if platform.needs_console_flag() {
        args.push("-mconsole".to_string());
    }
    
    // Freestanding/nostdlib flags
    if nostdlib {
        args.push("-nostdlib".to_string());
    }
    if ffreestanding {
        args.push("-ffreestanding".to_string());
    }

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
}
