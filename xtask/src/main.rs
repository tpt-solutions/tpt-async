// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `cargo xtask` — workspace automation.
//!
//! `cargo xtask ci` reproduces the full CI matrix locally: fmt, clippy,
//! tests, docs, the three cross-target builds, and cargo-deny.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    match task.as_str() {
        "ci" => run_all(),
        "fmt" => run("cargo", &["fmt", "--all", "--check"]),
        "clippy" => run(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
        "test" => run("cargo", &["test", "--workspace", "--all-features"]),
        "docs" => run_with_env(
            "cargo",
            &["doc", "--workspace", "--no-deps", "--all-features"],
            &[("RUSTDOCFLAGS", "-D warnings")],
        ),
        "cross" => run_many(&[
            ("cargo", &["check-embedded"]),
            ("cargo", &["check-riscv"]),
            ("cargo", &["check-wasm"]),
        ]),
        "deny" => run("cargo", &["deny", "check"]),
        other => {
            eprintln!("unknown task: {other}\n\ntasks: ci, fmt, clippy, test, docs, cross, deny");
            ExitCode::FAILURE
        }
    }
}

fn run_all() -> ExitCode {
    for step in [
        ("cargo", vec!["fmt", "--all", "--check"]),
        (
            "cargo",
            vec![
                "clippy",
                "--workspace",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("cargo", vec!["test", "--workspace", "--all-features"]),
    ] {
        if run(step.0, &step.1) != ExitCode::SUCCESS {
            return ExitCode::FAILURE;
        }
    }
    if run_with_env(
        "cargo",
        &["doc", "--workspace", "--no-deps", "--all-features"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    ) != ExitCode::SUCCESS
    {
        return ExitCode::FAILURE;
    }
    if run("cargo", &["check-embedded"]) != ExitCode::SUCCESS
        || run("cargo", &["check-riscv"]) != ExitCode::SUCCESS
        || run("cargo", &["check-wasm"]) != ExitCode::SUCCESS
    {
        return ExitCode::FAILURE;
    }
    run("cargo", &["deny", "check"])
}

fn run(program: &str, args: &[&str]) -> ExitCode {
    println!("+ {program} {}", args.join(" "));
    let status = Command::new(program).args(args).status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

fn run_with_env(program: &str, args: &[&str], env: &[(&str, &str)]) -> ExitCode {
    println!("+ {program} {} (env: {env:?})", args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

fn run_many(steps: &[(&str, &[&str])]) -> ExitCode {
    for (program, args) in steps {
        if run(program, args) != ExitCode::SUCCESS {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
