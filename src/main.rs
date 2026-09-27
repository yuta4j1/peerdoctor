use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use peerdoctor::{Lockfile, Report, TargetPackage, check};

/// Lists what blocks an npm project from moving a package to a given version.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The package and exact version you want, e.g. next@16.3.0
    target: String,

    /// The project directory containing package-lock.json
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(report) => {
            print!("{report}");
            if report.blockers().is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &Args) -> Result<Report, Box<dyn Error>> {
    let target = TargetPackage::parse(&args.target)?;
    let lockfile = Lockfile::read(&args.project)?;
    Ok(check(&lockfile, &target)?)
}
