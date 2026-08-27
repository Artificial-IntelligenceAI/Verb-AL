//! Verb-AL ships two backends, and the point of the language is that they
//! agree. These tests run every program both ways and insist on the same
//! bytes, the same standard error and the same exit status.
//!
//! Diagnostics are compared against recorded transcripts. Re-record them with
//! `VERBAL_BLESS=1 cargo test`.

use std::path::{Path, PathBuf};
use std::process::Command;

const EXE: &str = env!("CARGO_BIN_EXE_verbal");

struct Outcome {
    stdout: String,
    stderr: String,
    code: i32,
}

fn invoke(command: &mut Command) -> Outcome {
    let out = command.output().expect("the verb-al binary runs");
    Outcome {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn interpret(program: &Path) -> Outcome {
    invoke(Command::new(EXE).arg("run").arg(program))
}

fn compile_and_run(program: &Path) -> Outcome {
    let stem = program.file_stem().unwrap().to_string_lossy().into_owned();
    let exe = std::env::temp_dir().join(format!("verbal-test-{}", stem));
    let built = invoke(Command::new(EXE).arg("build").arg(program).arg("-o").arg(&exe));
    assert_eq!(
        built.code, 0,
        "compiling {} failed:\n{}{}",
        program.display(),
        built.stdout,
        built.stderr
    );
    invoke(&mut Command::new(&exe))
}

fn programs_in(dir: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("reading {}: {}", root.display(), e))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "val"))
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no programs found in {}", root.display());
    found
}

/// The central claim: the two backends are one language.
#[test]
fn backends_agree() {
    let mut checked = 0;
    for program in programs_in("examples").into_iter().chain(programs_in("tests/programs")) {
        let interpreted = interpret(&program);
        let compiled = compile_and_run(&program);
        let where_ = program.display();
        assert_eq!(
            interpreted.stdout, compiled.stdout,
            "{}: the backends printed different things",
            where_
        );
        assert_eq!(
            interpreted.stderr, compiled.stderr,
            "{}: the backends reported different faults",
            where_
        );
        assert_eq!(
            interpreted.code, compiled.code,
            "{}: the backends exited differently",
            where_
        );
        checked += 1;
    }
    assert!(checked >= 13, "expected the whole corpus, ran only {}", checked);
}

/// A faulting program says so on standard error and exits 3, both ways.
#[test]
fn faults_are_reported_alike() {
    for name in ["divide-by-zero", "divide-overflow", "remainder-by-zero"] {
        let program =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs").join(format!("{}.val", name));
        let interpreted = interpret(&program);
        assert_eq!(interpreted.code, 3, "{} should fault", name);
        assert!(
            interpreted.stderr.starts_with("verb-al: "),
            "{} should explain itself: {:?}",
            name,
            interpreted.stderr
        );
        let compiled = compile_and_run(&program);
        assert_eq!(interpreted.stderr, compiled.stderr);
        assert_eq!(interpreted.code, compiled.code);
    }
}

/// A fault stops the program where it happens, not before or after.
#[test]
fn a_fault_stops_at_the_fault() {
    let program =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/divide-by-zero.val");
    for outcome in [interpret(&program), compile_and_run(&program)] {
        assert_eq!(outcome.stdout, "before\n");
    }
}

/// Diagnostics are part of the language, so they are recorded and compared.
#[test]
fn diagnostics_are_stable() {
    let blessing = std::env::var("VERBAL_BLESS").is_ok();
    let mut wrong = Vec::new();
    for program in programs_in("tests/errors") {
        // Run from the manifest directory with a relative path, so the
        // recorded transcripts do not depend on where the repository lives.
        let relative = program.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(&program);
        let outcome = invoke(
            Command::new(EXE)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .arg("check")
                .arg(relative),
        );
        assert_eq!(outcome.code, 1, "{} should not compile", program.display());
        let recorded = program.with_extension("expected");
        if blessing {
            std::fs::write(&recorded, &outcome.stderr).expect("recording the diagnostic");
            continue;
        }
        let expected = std::fs::read_to_string(&recorded).unwrap_or_default();
        if expected != outcome.stderr {
            wrong.push(format!(
                "--- {} ---\nexpected:\n{}\nfound:\n{}",
                program.display(),
                expected,
                outcome.stderr
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{}\nre-record with VERBAL_BLESS=1 cargo test",
        wrong.join("\n")
    );
}
