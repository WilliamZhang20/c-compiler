use clap::Parser; // clap crate for CLI argument parsing
use std::{path::Path, process::Command};



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
}

fn main() {
    let args = Args::parse();

    let stop_after_emit_asm = args.emit_asm;
    let stop_after_codegen = args.codegen;
    let stop_after_parse = args.parse;
    let stop_after_lex = args.lex;
    let keep_intermediates = args.keep_intermediates || stop_after_emit_asm;

    // Check for gcc
    if Command::new("gcc").arg("--version").output().is_err() {
        eprintln!("Error: 'gcc' not found in PATH. Please install GCC.");
        std::process::exit(1);
    }

    let input_path = args.input_path.clone();
    let input_file = Path::new(&input_path);
    if !input_file.exists() {
         eprintln!("Error: Input file '{}' not found.", input_path);
         std::process::exit(1);
    }

    preprocess(&input_path, input_file);

    let preprocessed_path = input_file.file_stem().unwrap().to_string_lossy().to_string() + ".i";
    let src = std::fs::read_to_string(&preprocessed_path).expect("failed to read preprocessed file");

    let tokens = lexer::lex(&src).expect("Lexing failed");
    if stop_after_lex {
        println!("Tokens: {:?}", tokens);
        if !keep_intermediates {
            let _ = std::fs::remove_file(&preprocessed_path);
        }
        return;
    }

    let program = parser::parse_tokens(&tokens).expect("Parsing failed");
    if stop_after_parse {
        println!("AST: {:?}", program);
        if !keep_intermediates {
            let _ = std::fs::remove_file(&preprocessed_path);
        }
        return;
    }

    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&program).expect("Semantic analysis failed");

    let mut lowerer = ir::Lowerer::new();
    let ir_prog = lowerer.lower_program(&program).expect("IR lowering failed");
    
    let ir_prog = optimizer::optimize(ir_prog);

    if stop_after_codegen {
        println!("IR: {:?}", ir_prog);
        if !keep_intermediates {
            let _ = std::fs::remove_file(&preprocessed_path);
        }
        return;
    }

    let mut cg = codegen::Codegen::new();
    let asm = cg.gen_program(&ir_prog);

    let mut asm_path = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    asm_path.push_str(".s");
    std::fs::write(&asm_path, asm).expect("failed to write assembly file");

    if stop_after_emit_asm {
        if !keep_intermediates {
             let _ = std::fs::remove_file(&preprocessed_path);
        }
        return;
    }

    run_linker(&input_file, &asm_path, args.safe_malloc);
    println!("Compilation successful. Generated executable: {}", input_file.file_stem().unwrap().to_string_lossy());

    // Cleanup
    if !keep_intermediates {
        let _ = std::fs::remove_file(&preprocessed_path);
        let _ = std::fs::remove_file(&asm_path);
    }
}

fn preprocess(input_path: &str, input_file: &Path) {
    let mut preprocessed_path = input_file.file_stem().unwrap().to_string_lossy().to_string();
    preprocessed_path.push_str(".i");

    let exit_code = Command::new("gcc")
        .args(["-E", "-P", {input_path}, "-o", {&preprocessed_path}])
        .status()
        .expect("failed to execute process");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc preprocess command failed with exit code {}", code);
        }
        panic!("gcc preprocess command was terminated by a signal");
    }
}

fn run_linker(input_file: &Path, asm_path: &str, use_safe_malloc: bool) {
    let mut executable_file = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    executable_file.push_str(".exe");

    let mut args = vec![asm_path];
    
    // Optionally link with safe malloc runtime
    if use_safe_malloc {
        args.push("runtime/malloc.o");
    }
    
    args.push("-o");
    args.push(&executable_file);
    args.push("-mconsole");

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
