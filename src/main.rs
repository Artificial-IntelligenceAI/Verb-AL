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

  verbal run <file>                    interpret the program, here
  verbal jit <file>                    compile in memory and run, here
  verbal build <file> -m <machine>     compile for a machine
                      [-o exe]
  verbal emit-ir <file> -m <machine>   print the LLVM IR for a machine
  verbal check <file> [-m <machine>]   report any errors and stop

A .machine file names the machine an artefact is produced for. `build` and
`emit-ir` require one, because they produce something for a machine. `run` and
`jit` refuse one, because they execute here. `check` will take one, and says so
when it has not been given one.
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

    let named_machine = args
        .iter()
        .position(|a| a == "-m")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // Which commands may name a machine is not a matter of taste: it follows
    // from whether the command produces something for a machine or runs here.
    match (command.as_str(), named_machine.is_some()) {
        ("build" | "emit-ir", false) => {
            eprintln!(
                "verb-al: `{}` produces an artefact for a machine, so it needs one: -m <file>.machine",
                command
            );
            return ExitCode::from(2);
        }
        ("run" | "jit", true) => {
            eprintln!(
                "verb-al: `{}` executes on this machine, so it cannot be given another one",
                command
            );
            return ExitCode::from(2);
        }
        _ => {}
    }

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("verb-al: cannot read {}: {}", path, e);
            return ExitCode::from(2);
        }
    };
    let source = Source::new(path.clone(), text);

    // A named machine, or this one when the command runs here.
    let built = match &named_machine {
        Some(file) => match verbal::machine_file(file, verbal::may_report(&source)) {
            Ok(built) => Some(built),
            Err(rejection) => {
                match rejection.rendered {
                    Some(rendered) => eprint!("{}", rendered),
                    None => eprintln!("verb-al: cannot read {}", file),
                }
                return ExitCode::from(1);
            }
        },
        None => None,
    };
    let machine = match &built {
        Some(built) => built.properties.clone(),
        None => match verbal::machine::host() {
            Ok(machine) => machine,
            Err(why) => {
                eprintln!("verb-al: {}", why);
                return ExitCode::from(2);
            }
        },
    };
    let program = match verbal::front_end(&source, &machine) {
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
            // A clean exit that quietly meant "clean among the claims I
            // bothered to check" would be this language's own defect.
            match &built {
                Some(built) => println!("{}: no errors, checked for {}", path, built.triple),
                None => println!(
                    "{}: no errors, checked against this host ({}) — pass -m <file>.machine \
                     to check against the machine you are building for",
                    path, machine.triple
                ),
            }
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
            let built = built.as_ref().expect("emit-ir requires a machine");
            let ctx = Context::create();
            let module = codegen::compile(&ctx, &program, &module_name(path));
            module.set_triple(&built.target.get_triple());
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
            let built = built.as_ref().expect("build requires a machine");
            let output = output_path(&args, path);
            let ctx = Context::create();
            let module = codegen::compile(&ctx, &program, &module_name(path));
            if let Err(e) = module.verify() {
                eprintln!("verb-al: the generated module is not well formed:\n{}", e.to_string());
                return ExitCode::from(2);
            }
            let object = output.with_extension("o");
            if let Err(e) = codegen::write_object(&module, &built.target, &object) {
                eprintln!("verb-al: {}", e);
                return ExitCode::from(2);
            }
            let linked = std::process::Command::new("clang")
                .arg("-target")
                .arg(&built.triple)
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
