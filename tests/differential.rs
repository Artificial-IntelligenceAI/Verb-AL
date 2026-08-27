//! Verb-AL ships two backends, and the point of the language is that they
//! agree. These tests run every program both ways and insist on the same
//! bytes, the same standard error and the same exit status.
//!
//! Diagnostics are compared against recorded transcripts. Re-record them with
//! `VERBAL_BLESS=1 cargo test`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What this machine is. Verb-AL programs must say what they need of their
/// machine, so the fixtures these tests write have to say it too — which makes
/// the suite, like the corpus, specific to a 64-bit little-endian host.
const REQUIREMENT: &str =
    "requires:target.64-bit-pointers.little-endian.8-byte-maximum-alignment end";
/// The machine this suite builds for. Named rather than inferred, like every
/// other build — which makes the suite, like the corpus, specific to this host.
const MACHINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/machines/mac-arm64.machine");

const PREAMBLE: &str = concat!(
    "allow[compiler:error.error-message]end\n",
    "requires:target.64-bit-pointers.little-endian.8-byte-maximum-alignment end"
);

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
    let built = invoke(
        Command::new(EXE)
            .arg("build")
            .arg(program)
            .arg("-m")
            .arg(MACHINE)
            .arg("-o")
            .arg(&exe),
    );
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
    assert!(checked >= 15, "expected the whole corpus, ran only {}", checked);
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

/// A program that has not permitted error messages does not get error
/// messages. It fails, and says nothing at all about why.
#[test]
fn silence_is_the_default() {
    let program = std::env::temp_dir().join("verbal-unpermitted.val");
    std::fs::write(
        &program,
        &format!("{}\nprivacy:local, memory:static, type:truth.1-bit, name.string.end: \"well-formed\" = 'true'end\n", REQUIREMENT),
    )
    .unwrap();
    let outcome = invoke(Command::new(EXE).arg("check").arg(&program));
    assert_eq!(outcome.code, 1, "the program should still fail");
    assert_eq!(outcome.stderr, "", "but it never permitted an explanation");
    assert_eq!(outcome.stdout, "");

    // The same program, having asked, is told.
    let permitted = std::env::temp_dir().join("verbal-permitted.val");
    std::fs::write(
        &permitted,
        format!("allow[compiler:error.error-message]end\n{}", std::fs::read_to_string(&program).unwrap()),
    )
    .unwrap();
    let outcome = invoke(Command::new(EXE).arg("check").arg(&permitted));
    assert_eq!(outcome.code, 1);
    assert!(
        outcome.stderr.contains("rule broke & where:")
            && outcome.stderr.contains("suggested fix:"),
        "expected a full report, got {:?}",
        outcome.stderr
    );
}

/// Granting a branch grants everything beneath it.
#[test]
fn a_grant_covers_what_is_beneath_it() {
    let program = std::env::temp_dir().join("verbal-branch-grant.val");
    std::fs::write(
        &program,
        &format!("allow[compiler:error]end\n{}\nprivacy:local, memory:static, type:truth.1-bit, name.string.end: \"well-formed\" = 'true'end\n", REQUIREMENT),
    )
    .unwrap();
    let outcome = invoke(Command::new(EXE).arg("check").arg(&program));
    assert!(outcome.stderr.contains("rule broke & where:"), "got {:?}", outcome.stderr);
}

/// Every report carries the whole template, and every one suggests a fix —
/// a diagnostic that cannot say what to write instead is not finished.
#[test]
fn every_report_follows_the_template() {
    for program in programs_in("tests/errors") {
        let relative = program.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(&program);
        let outcome = invoke(
            Command::new(EXE)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .arg("check")
                .arg(relative),
        );
        let lines: Vec<&str> = outcome.stderr.lines().collect();
        let where_ = program.display();
        assert_eq!(lines.len(), 6, "{}: a report is six lines, got {:?}", where_, outcome.stderr);
        assert!(lines[0].matches(':').count() >= 2, "{}: no location link", where_);
        for (i, field) in
            ["file: ", "line: ", "column: ", "rule broke & where: ", "suggested fix: "]
                .iter()
                .enumerate()
        {
            assert!(
                lines[i + 1].starts_with(field),
                "{}: line {} should begin {:?}, got {:?}",
                where_,
                i + 2,
                field,
                lines[i + 1]
            );
            assert!(
                lines[i + 1].len() > field.len(),
                "{}: the {:?} field is empty",
                where_,
                field
            );
        }
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

/// The statement-extraction test can check that the documentation's examples
/// are still Verb-AL, but not that its prose describes the language — a claim
/// about the grammar is not an extractable statement. So the claims §5 makes
/// are pinned here instead, where the compiler answers rather than the page.
#[test]
fn a_write_names_a_variable_and_nothing_else() {
    let declaration = "privacy:local, memory:static, type:text.utf-8.pointer-and-length, \
                       name.string.end: \"greeting\" = 'hi'end";
    let cases = [
        // A restated declaration is the only thing the (…) position accepts.
        (format!("{}\nstandard-output:print.variable.newline-too.end:[({})]end", declaration, declaration), true),
        // Not an expression, however simple.
        (format!("{}\nstandard-output:print.variable.newline-too.end:[(\"greeting\")]end", declaration), false),
        // Not an allocating one either, so no temporary can escape into a write.
        (format!("{}\nstandard-output:print.variable.newline-too.end:[(\"greeting\" joined-with \"greeting\")]end", declaration), false),
        // Literal content is double-quoted, never single.
        (format!("standard-output:print.string.newline-too.end:['hi']end"), false),
    ];
    for (source, should_compile) in cases {
        let program = std::env::temp_dir().join("verbal-write-shape.val");
        std::fs::write(&program, format!("{}\n{}\n", PREAMBLE, source))
            .unwrap();
        let outcome = invoke(Command::new(EXE).arg("check").arg(&program));
        assert_eq!(
            outcome.code == 0,
            should_compile,
            "expected compiles={} for:\n{}\n{}",
            should_compile,
            source,
            outcome.stderr
        );
    }
}

/// Which commands may name a machine follows from whether they produce
/// something for one or run here, and `check` must say when it was not given
/// one — a clean exit meaning "clean among the claims I bothered to check" is
/// the defect this language is organised against.
#[test]
fn a_machine_is_named_where_it_matters() {
    let program = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello-world.val");

    let refused = invoke(Command::new(EXE).arg("build").arg(program));
    assert_ne!(refused.code, 0, "build without a machine should refuse");
    assert!(refused.stderr.contains("needs one"), "got {:?}", refused.stderr);

    let refused = invoke(Command::new(EXE).arg("run").arg(program).arg("-m").arg(MACHINE));
    assert_ne!(refused.code, 0, "run with a machine should refuse");
    assert!(refused.stderr.contains("executes on this machine"), "got {:?}", refused.stderr);

    let bare = invoke(Command::new(EXE).arg("check").arg(program));
    assert_eq!(bare.code, 0);
    assert!(
        bare.stdout.contains("checked against this host"),
        "check must say it had no machine: {:?}",
        bare.stdout
    );

    let named = invoke(Command::new(EXE).arg("check").arg(program).arg("-m").arg(MACHINE));
    assert_eq!(named.code, 0);
    assert!(
        named.stdout.contains("checked for aarch64-apple-darwin25-macho"),
        "check must say which machine: {:?}",
        named.stdout
    );
}

/// Two unlexable characters in one file once crashed the permission scan,
/// which runs before the compiler is even allowed to speak.
#[test]
fn unlexable_text_is_reported_not_fatal() {
    let program = std::env::temp_dir().join("verbal-unlexable.val");
    std::fs::write(&program, &format!("{}\nprivacy:<a>, memory:<b>end\n", PREAMBLE))
        .unwrap();
    let outcome = invoke(Command::new(EXE).arg("check").arg(&program));
    assert_eq!(outcome.code, 1, "it should fail");
    assert!(
        outcome.stderr.contains("rule broke & where:"),
        "it should explain itself, not crash: {:?}",
        outcome.stderr
    );
}

/// The specification drifted once: a string replacement silently matched
/// nothing, and §5 went on describing a syntax the compiler had stopped
/// accepting. Every statement written in the documentation is now parsed.
#[test]
fn the_documentation_still_speaks_verbal() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for doc in ["SPEC.md", "README.md"] {
        let text = std::fs::read_to_string(root.join(doc)).unwrap();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            let is_statement = ["privacy:", "standard-output:", "action:", "allow["]
                .iter()
                .any(|start| line.starts_with(start));
            // Prose quotes fragments, and the grammar sketches use <angled>
            // metavariables; only whole, literal statements are parseable.
            if !is_statement || !line.ends_with("end") || line.contains('<') {
                continue;
            }
            let program = root.join("target").join("doc-line.val");
            std::fs::write(&program, format!("{}\n{}\n", PREAMBLE, line))
                .unwrap();
            let outcome = invoke(Command::new(EXE).arg("check").arg(&program));
            // A statement quoted on its own may name something the surrounding
            // prose declared. That is not drift; anything else is.
            let self_contained =
                !outcome.stderr.contains("must be declared before it is used");
            assert!(
                outcome.code == 0 || !self_contained,
                "{}:{} is no longer Verb-AL:\n{}\n{}",
                doc,
                number + 1,
                line,
                outcome.stderr
            );
            checked += 1;
        }
    }
    assert!(checked >= 15, "expected the documentation's statements, found {}", checked);
}
