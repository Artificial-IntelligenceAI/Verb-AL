use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inkwell::context::Context;
use verbal::codegen;
use verbal::diag::Source;
use verbal::interp;
use verbal::value::FAULT_EXIT;

const USAGE: &str = "\
verb-al — a language in which nothing is implicit

  verbal run <file>              interpret the program
  verbal jit <file>              compile in memory and run
  verbal build <file> [-o exe]   compile to a native executable
  verbal emit-ir <file>          print the LLVM IR
  verbal check <file>            report any errors and stop
";

fn main() -> ExitCode {
    // Rust ignores SIGPIPE; a compiled Verb-AL program does not. Restore the
    // default so `verbal run … | head` and the compiled binary die alike.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprint!("{}", USAGE);
        return ExitCode::from(2);
    };
    let Some(path) = args.get(1) else {
        eprintln!("verb-al: `{}` needs a file to work on", command);
        return ExitCode::from(2);
    };

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("verb-al: cannot read {}: {}", path, e);
            return ExitCode::from(2);
        }
    };
    let source = Source::new(path.clone(), text);

    let program = match verbal::front_end(&source) {
        Ok(p) => p,
        Err(rejection) => {
            // A program that did not permit error messages does not get one.
            if let Some(rendered) = rejection.rendered {
                eprint!("{}", rendered);
            }
            return ExitCode::from(1);
        }
    };

    match command.as_str() {
        "check" => {
            println!("{}: no errors", path);
            ExitCode::SUCCESS
        }

        "run" => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            match interp::run(&program, &mut out) {
                Ok(()) => {
                    let _ = out.flush();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    let _ = out.flush();
                    eprint!("{}", e.fault.message());
                    ExitCode::from(FAULT_EXIT as u8)
                }
            }
        }

        "emit-ir" => {
            let ctx = Context::create();
            let module = codegen::compile(&ctx, &program, &module_name(path));
            print!("{}", module.print_to_string().to_string());
            ExitCode::SUCCESS
        }

        "jit" => {
            let ctx = Context::create();
            let module = codegen::compile(&ctx, &program, &module_name(path));
            match codegen::jit(&module) {
                Ok(status) => ExitCode::from(status as u8),
                Err(e) => {
                    eprintln!("verb-al: {}", e);
                    ExitCode::from(2)
                }
            }
        }

        "build" => {
            let output = output_path(&args, path);
            let ctx = Context::create();
            let module = codegen::compile(&ctx, &program, &module_name(path));
            if let Err(e) = module.verify() {
                eprintln!("verb-al: the generated module is not well formed:\n{}", e.to_string());
                return ExitCode::from(2);
            }
            let object = output.with_extension("o");
            if let Err(e) = codegen::write_object(&module, &object) {
                eprintln!("verb-al: {}", e);
                return ExitCode::from(2);
            }
            let linked = std::process::Command::new("clang")
                .arg(&object)
                .arg("-o")
                .arg(&output)
                .status();
            let _ = std::fs::remove_file(&object);
            match linked {
                Ok(s) if s.success() => {
                    println!("{}", output.display());
                    ExitCode::SUCCESS
                }
                Ok(s) => {
                    eprintln!("verb-al: the linker failed ({})", s);
                    ExitCode::from(2)
                }
                Err(e) => {
                    eprintln!("verb-al: cannot run clang: {}", e);
                    ExitCode::from(2)
                }
            }
        }

        other => {
            eprintln!("verb-al: `{}` is not a command\n", other);
            eprint!("{}", USAGE);
            ExitCode::from(2)
        }
    }
}

fn module_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "program".into())
}

fn output_path(args: &[String], source: &str) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "-o") {
        if let Some(o) = args.get(i + 1) {
            return PathBuf::from(o);
        }
    }
    PathBuf::from(Path::new(source).file_stem().unwrap_or_default())
}
