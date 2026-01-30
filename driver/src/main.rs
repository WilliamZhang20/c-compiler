use clap::Parser; // clap crate for CLI argument parsing
use std::{path::Path, process::Command};

/*
"short" means the field can be specified using a short,
single-character option on the command line, typically
a single dash (e.g., -n).

If you just write short without a character (like #[arg(short = 'n')]),
clap will infer the short flag from the first letter of the field name,
which in this case is n.
*/

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
}

fn main() {
    let args = Args::parse();

    let stop_after_emit_asm = args.emit_asm;
    let stop_after_codegen = args.codegen;
    let stop_after_parse = args.parse;
    let stop_after_lex = args.lex;

    let input_path = args.input_path.clone();
    let input_file = Path::new(&input_path);

    preprocess(&input_path, input_file);

    let preprocessed_path = input_file.file_stem().unwrap().to_string_lossy().to_string() + ".i";
    let src = std::fs::read_to_string(&preprocessed_path).expect("failed to read preprocessed file");

    let tokens = lexer::lex(&src).expect("Lexing failed");
    if stop_after_lex {
        println!("Tokens: {:?}", tokens);
        return;
    }

    let program = parser::parse_tokens(&tokens).expect("Parsing failed");
    if stop_after_parse {
        println!("AST: {:?}", program);
        return;
    }

    let mut analyzer = semantic::SemanticAnalyzer::new();
    analyzer.analyze(&program).expect("Semantic analysis failed");

    let mut lowerer = ir::Lowerer::new();
    let ir_prog = lowerer.lower_program(&program).expect("IR lowering failed");
    
    let ir_prog = optimizer::optimize(ir_prog);

    if stop_after_codegen {
        println!("IR: {:?}", ir_prog);
        return;
    }

    let mut cg = codegen::Codegen::new();
    let asm = cg.gen_program(&ir_prog);

    let mut asm_path = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    asm_path.push_str(".s");
    std::fs::write(&asm_path, asm).expect("failed to write assembly file");

    if stop_after_emit_asm {
        return;
    }

    run_linker(&input_file, &asm_path);
    println!("Compilation successful. Generated executable: {}", input_file.file_stem().unwrap().to_string_lossy());
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

fn run_linker(input_file: &Path, asm_path: &str) {
    let mut executable_file = input_file.file_stem().unwrap().to_string_lossy().into_owned();
    executable_file.push_str(".exe");

    let exit_code = Command::new("gcc")
        .args([{&asm_path}, "runtime/malloc.o", "-o", &executable_file, "-mconsole"])
        .status()
        .expect("executable generated sucessfully");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc compilation failed with exit code {}", code);
        }
        panic!("gcc compilation was terminated by a signal");
    }
}
