use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use std::fmt::{Display, Formatter};

// use crate::syntax::{tokenize, parse_sexp_from_tokens, parse_exp_from_sexp, Span, Expr};
use crate::compile;
use crate::compile::{compile_to_string, CompileErr};
use crate::interp;
use crate::interp::InterpErr;
use crate::lexer;
use crate::lexer::{FileInfo, Span1, Span2};
use crate::parser::ProgParser;
use crate::syntax::SurfProg;

#[derive(Debug, PartialEq, Eq)]
pub enum RunnerErr<Span> {
    FileOpen(String),
    Lex(String),
    Parse(String),
    CodeGen(CompileErr<Span>),
    Link(String),
    Interp(InterpErr),
    Run(String),
}

impl<Span> Display for RunnerErr<Span>
where
    Span: Display,
{
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            RunnerErr::FileOpen(s) => write!(f, "Error reading file: {}", s),
            RunnerErr::Lex(s) => write!(f, "Error lexing input: {}", s),
            RunnerErr::Parse(s) => write!(f, "Error parsing input: {}", s),
            RunnerErr::CodeGen(ce) => write!(f, "Error generating assembly: {}", ce),
            RunnerErr::Link(s) => write!(f, "Error linking generated assembly with runtime: {}", s),
            RunnerErr::Interp(s) => write!(f, "Error in interpreter: {}", s),
            RunnerErr::Run(s) => write!(f, "Error running your compiled output: {}", s),
        }
    }
}

// impl<Span> RunnerErr<Span> {
//     fn map_span<F, SpanPrime>(&self, f: F) -> RunnerErr<SpanPrime>
// 	where F: FnOnce(&Span) -> SpanPrime
//     {
// 	match self {
// 	    RunnerErr::CodeGen(s) => RunnerErr::CodeGen(s.map_span(f)),

// 	    RunnerErr::FileOpen(s) => RunnerErr::FileOpen(s.clone()),
// 	    RunnerErr::Lex(s) => RunnerErr::Lex(s.clone()),
// 	    RunnerErr::Read(s) => RunnerErr::Read(s.clone()),
// 	    RunnerErr::Parse(s) => RunnerErr::Parse(s.clone()),
// 	    RunnerErr::Link(s) => RunnerErr::Link(s.clone()),
// 	    RunnerErr::Interp(s) => RunnerErr::Interp(s.clone()),
// 	}
//     }
// }

fn fail<Span>(e: RunnerErr<Span>)
where
    Span: Display,
{
    eprintln!("{}", e);
    std::process::exit(1);
}

fn handle_errs<Span>(r: Result<String, RunnerErr<Span>>)
where
    Span: Display,
{
    match r {
        Ok(s) => println!("{}", s),
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

pub fn emit_assembly(p: &Path) {
    handle_errs(compile_file(p))
}

pub fn run(p: &Path) {
    if let Err(e) = compile_and_run_file(p, Path::new("runtime"), &mut std::io::stdout()) {
        fail(e)
    }
}

pub fn interp<W>(p: &Path, w: &mut W)
where
    W: std::io::Write,
{
    if let Err(e) = interpret_file(p, w) {
        fail(e)
    }
}

pub fn interpret_file<W>(p: &Path, w: &mut W) -> Result<(), RunnerErr<Span2>>
where
    W: std::io::Write,
{
    let (info, prog) = parse_file(p)?;
    let () = compile::check_prog(&prog)
        .map_err(|e| RunnerErr::CodeGen(e.map_span(|s| lexer::span1_to_span2(&info, *s))))?;

    let v = interp::prog(&prog, w).map_err(RunnerErr::Interp)?;
    writeln!(w, "{}", v).map_err(|e| {
        RunnerErr::Interp(InterpErr::Write {
            msg: format!("{}", e),
        })
    })?;
    Ok(())
}

pub fn compile_and_run_file<W>(p: &Path, dir: &Path, out: &mut W) -> Result<(), RunnerErr<Span2>>
where
    W: std::io::Write,
{
    let asm = compile_file(p)?;
    link_and_run(&asm, dir, out)
}

fn compile_file(p: &Path) -> Result<String, RunnerErr<Span2>> {
    let (info, prog) = parse_file(p)?;
    compile_to_string(&prog)
        .map_err(|e| RunnerErr::CodeGen(e.map_span(|s| lexer::span1_to_span2(&info, *s))))
}

fn read_file<Span>(p: &Path) -> Result<String, RunnerErr<Span>> {
    let mut f = File::open(p).map_err(|e| RunnerErr::FileOpen(e.to_string()))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)
        .map_err(|e| RunnerErr::FileOpen(e.to_string()))?;
    Ok(buf)
}

fn parse_file(p: &Path) -> Result<(FileInfo, SurfProg<Span1>), RunnerErr<Span2>> {
    let s = read_file(p)?;
    let e = ProgParser::new()
        .parse(&s)
        .map_err(|e| RunnerErr::Parse(e.to_string()))?;
    Ok((lexer::file_info(&s), e))
    // let toks = tokenize(&s).map_err(|e| RunnerErr::Lex(e))?;
    // let sexp  = parse_sexp_from_tokens(&toks).map_err(|e| RunnerErr::Read(e))?;
    // parse_exp_from_sexp(&sexp).map_err(|e| RunnerErr::Parse(e))
}

fn link_and_run<W>(assembly: &str, dir: &Path, out: &mut W) -> Result<(), RunnerErr<Span2>>
where
    W: std::io::Write,
{
    let (nasm_format, lib_name) = if cfg!(target_os = "linux") {
        ("elf64", "libcompiled_code.a")
    } else if cfg!(target_os = "macos") {
        ("macho64", "libcompiled_code.a")
    } else if cfg!(target_os = "windows") {
        ("win64", "compiled_code.lib")
    } else {
        panic!("Runner script only works on linux, macos and windows")
    };

    let asm_fname = dir.join("compiled_code.s");
    let obj_fname = dir.join("compiled_code.o");
    let lib_fname = dir.join(lib_name);
    let exe_fname = dir.join("stub.exe");

    // first put the assembly in a new file compiled_code.s
    let mut asm_file = File::create(&asm_fname).map_err(|e| RunnerErr::Link(e.to_string()))?;
    asm_file
        .write(assembly.as_bytes())
        .map_err(|e| RunnerErr::Link(e.to_string()))?;
    asm_file
        .flush()
        .map_err(|e| RunnerErr::Link(e.to_string()))?;

    // nasm -fFORMAT -o compiled_code.o compiled_code.s
    let nasm_out = Command::new("nasm")
        .arg("-f")
        .arg(nasm_format)
        .arg("-o")
        .arg(&obj_fname)
        .arg(&asm_fname)
        .output()
        .map_err(|e| RunnerErr::Link(format!("nasm err: {}", e)))?;
    if !nasm_out.status.success() {
        return Err(RunnerErr::Link(format!(
            "Failure in nasm call: {}\n{}",
            nasm_out.status,
            std::str::from_utf8(&nasm_out.stderr).expect("nasm produced invalid UTF-8")
        )));
    }

    // ar r libcompiled_code.a compiled_code.o
    let ar_out = Command::new("ar")
        .arg("rus")
        .arg(lib_fname)
        .arg(&obj_fname)
        .output()
        .map_err(|e| RunnerErr::Link(format!("ar err: {}", e)))?;
    if !ar_out.status.success() {
        return Err(RunnerErr::Link(format!(
            "Failure in ar call:\n{}\n{}",
            ar_out.status,
            std::str::from_utf8(&ar_out.stderr).expect("ar produced invalid UTF-8")
        )));
    }

    // rustc stub.rs -L tmp
    let rustc_out = if cfg!(target_os = "macos") {
        Command::new("rustc")
            .arg("runtime/stub.rs")
            .arg("--target")
            .arg("x86_64-apple-darwin")
            .arg("-L")
            .arg(dir)
            .arg("-o")
            .arg(&exe_fname)
            .output()
            .map_err(|e| RunnerErr::Link(format!("rustc err: {}", e)))?
    } else {
        Command::new("rustc")
            .arg("runtime/stub.rs")
            .arg("-L")
            .arg(dir)
            .arg("-o")
            .arg(&exe_fname)
            .output()
            .map_err(|e| RunnerErr::Link(format!("rustc err: {}", e)))?
    };
    if !rustc_out.status.success() {
        return Err(RunnerErr::Link(format!(
            "Failure in rustc call: {}\n{}",
            rustc_out.status,
            std::str::from_utf8(&rustc_out.stderr).expect("rustc produced invalid UTF-8")
        )));
    }

    let mut child = Command::new(&exe_fname)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunnerErr::Run(format!("{}", e)))?;
    let compiled_out = BufReader::new(
        child
            .stdout
            .take()
            .expect("Failed to capture compiled code's stdout"),
    );
    let compiled_err = BufReader::new(
        child
            .stderr
            .take()
            .expect("Failed to capture compiled code's stderr"),
    );

    for line in compiled_out.lines() {
        let line = line.map_err(|e| RunnerErr::Run(format!("{}", e)))?;
        writeln!(out, "{}", line).map_err(|e| RunnerErr::Run(format!("I/O error: {}", e)))?;
    }

    let status = child
        .wait()
        .map_err(|e| RunnerErr::Run(format!("Error waiting for child process {}", e)))?;
    if !status.success() {
        let mut stderr = String::new();
        for line in compiled_err.lines() {
            stderr.push_str(&format!("{}\n", line.unwrap()));
        }
        return Err(RunnerErr::Run(format!(
            "Error code {} when running compiled code Stderr:\n{}",
            status, stderr
        )));
    }
    Ok(())
}
