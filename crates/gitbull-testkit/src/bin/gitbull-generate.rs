//! Generates a repository for benchmarks:
//! `gitbull-generate <folder> [commits]`, one million commits by default.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use gitbull_testkit::generator::{Shape, generate};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(folder) = args.next().map(PathBuf::from) else {
        eprintln!("usage: gitbull-generate <folder> [commits]");
        return ExitCode::FAILURE;
    };
    let commits = match args.next().map(|n| n.parse::<usize>()) {
        None => 1_000_000,
        Some(Ok(n)) if n > 0 => n,
        Some(_) => {
            eprintln!("commits must be a positive number");
            return ExitCode::FAILURE;
        }
    };
    if folder.exists()
        && folder
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
    {
        eprintln!("{} is not empty", folder.display());
        return ExitCode::FAILURE;
    }
    let started = Instant::now();
    match generate(&folder, &Shape::benchmark(commits)) {
        Ok(()) => {
            println!(
                "{commits} commits in {} after {:.1} s",
                folder.display(),
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not generate the repository: {error}");
            ExitCode::FAILURE
        }
    }
}
