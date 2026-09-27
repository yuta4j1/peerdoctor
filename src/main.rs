use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use peerdoctor::{HttpRegistry, Overrides, Project, Report, TargetPackage, check};

/// Lists what blocks an npm project from moving a package to a given version.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The package and exact version you want, e.g. next@16.3.0
    target: String,

    /// The project directory containing package-lock.json
    #[arg(long, default_value = ".")]
    project: PathBuf,

    /// The registry to fetch package versions from
    #[arg(long, default_value = "https://registry.npmjs.org")]
    registry: String,

    /// Only detect blockers, without fetching versions that would resolve them
    #[arg(long)]
    no_suggest: bool,
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
    let mut report = check(project.lockfile(), &target)?;
    if !args.no_suggest {
        report.suggest(&HttpRegistry::new(&args.registry));
    }
    Ok(report)
}
