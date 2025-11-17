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

    // --lex: we should only lex
    // --parse: we should lex and parse
    // --codegen: we should lex, parse, and generate code, but not create assemly file
    // --emit-asm: we should lex, parse, generate code, and create assembly file
    // no option: we should lex, parse, generate code, create assembly file, and link

    let stop_after_emit_asm = args.emit_asm;
    let stop_after_codegen = args.codegen;
    let stop_after_parse = args.parse;
    let stop_after_lex = args.lex;

    let input_path = args.input_path.clone();
    let input_file = Path::new(&input_path);

    preprocess(&input_path, input_file);

    lex();

    if stop_after_lex {
        return;
    }

    parse();

    if stop_after_parse {
        return;
    }

    code_gen();

    if stop_after_codegen {
        return;
    }

    let mut asm_path = input_file.file_stem().unwrap().to_string_lossy();
    asm_path.to_mut().push_str(".s");

    emit_asm(&input_path, &asm_path);
    println!("Ran assembler");

    if stop_after_emit_asm {
        return;
    }

    run_linker(&input_file, &asm_path);
    println!("Ran linker");
}

fn preprocess(input_path: &str, input_file: &Path) {
    let mut preprocessed_path = input_file.file_stem().unwrap().to_string_lossy();
    preprocessed_path.to_mut().push_str(".i");

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

fn lex() {
    println!("No lexer yet");
}

fn parse() {
    println!("No parser yet");
}

fn code_gen() {
    println!("No code generator yet");
}

fn emit_asm(input_path: &str, output_path: &str) {
    let exit_code = Command::new("gcc")
        .args(["-S", {input_path}, "-o", {&output_path}])
        .status()
        .expect("preprocessed file should compile successfully");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc compilation failed with exit code {}", code);
        }
        panic!("gcc compilation was terminated by a signal");
    }
}

fn run_linker(input_file: &Path, asm_path: &str) {
    let executable_file = input_file.file_stem().unwrap().to_string_lossy();

    let exit_code = Command::new("gcc")
        .args([{&asm_path}, "-o", &executable_file])
        .status()
        .expect("executable generated sucessfully");

    if !exit_code.success() {
        if let Some(code) = exit_code.code() {
            panic!("gcc compilation failed with exit code {}", code);
        }
        panic!("gcc compilation was terminated by a signal");
    }
}
