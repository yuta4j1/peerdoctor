use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use peerdoctor::{Overrides, Project, Report, TargetPackage, check};

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
    let project = Project::read(&args.project)?;
    match project.overrides() {
        Overrides::Declared => eprintln!(
            "warning: package.json declares overrides; the result is based on the locked tree \
             and may differ from what npm resolves with the overrides applied"
        ),
        Overrides::Unknown => {
            eprintln!("warning: could not read package.json, so overrides were not checked")
        }
        Overrides::NotDeclared => {}
    }
    Ok(check(project.lockfile(), &target)?)
}
