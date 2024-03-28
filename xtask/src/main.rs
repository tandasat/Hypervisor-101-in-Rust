//! A build and test assist program. To show the usage, run
//!
//! ```shell
//! cargo xtask
//! ```

#![allow(clippy::multiple_crate_versions)]

use bochs::{Bochs, Cpu};
use clap::{Parser, Subcommand};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use vmware::Vmware;

mod bochs;
mod vmware;

type DynError = Box<dyn std::error::Error>;

#[derive(Parser)]
#[command(author, about, long_about = None)]
struct Cli {
    /// Build the hypervisor with the release profile
    #[arg(short, long)]
    release: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a Bochs VM with an Intel processor
    BochsIntel,
    /// Start a Bochs VM with an AMD processor
    BochsAmd,
    /// Start a `VMware` VM
    Vmware,
}

fn main() {
    let cli = Cli::parse();
    let result = match &cli.command {
        Commands::BochsIntel => start_vm(&Bochs { cpu: Cpu::Intel }, cli.release),
        Commands::BochsAmd => start_vm(&Bochs { cpu: Cpu::Amd }, cli.release),
        Commands::Vmware => start_vm(&Vmware {}, cli.release),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(-1);
    }
}

trait TestVm {
    fn deploy(&self, release: bool) -> Result<(), DynError>;
    fn run(&self) -> Result<(), DynError>;
}

fn start_vm<T: TestVm>(vm: &T, release: bool) -> Result<(), DynError> {
    build_hypervisor(release)?;
    extract_samples()?;
    vm.deploy(release)?;
    vm.run()
}

fn build_hypervisor(release: bool) -> Result<(), DynError> {
    // Building rhv only is important because we are running xtask, which cannot
    // be overwritten while running.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    let _ = command.args(["build", "--package", "rhv"]);
    if release {
        let _ = command.arg("--release");
    }
    let ok = command.current_dir(project_root_dir()).status()?.success();
    if !ok {
        Err("cargo build failed")?;
    }
    Ok(())
}

fn project_root_dir() -> PathBuf {
    // Get the path to rhv/xtask directory and resolve its parent directory.
    let root_dir = Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf();
    fs::canonicalize(root_dir).unwrap()
}

fn extract_samples() -> Result<(), DynError> {
    if !Path::new("./tests/samples/").exists() {
        println!("Extracting sample files...");
        let output = UnixCommand::new("7z")
            .args(["x", "-o./tests/", "./tests/samples.7z"])
            .output()?;
        if !output.status.success() {
            Err(format!("7z failed: {output:#?}"))?;
        }
    }
    Ok(())
}

fn copy_artifacts_to(image: &str, release: bool) -> Result<(), DynError> {
    fn output_dir(release: bool) -> PathBuf {
        let mut out_dir = project_root_dir();
        out_dir.extend(&["target", "x86_64-unknown-uefi"]);
        out_dir.extend(if release { &["release"] } else { &["debug"] });
        fs::canonicalize(&out_dir).unwrap()
    }

    let rhv_efi = unix_path(&output_dir(release)) + "/rhv.efi";
    let startup_nsh = unix_path(&project_root_dir()) + "/tests/startup.nsh";
    let files = [rhv_efi, startup_nsh];
    for file in &files {
        let output = UnixCommand::new("mcopy")
            .args(["-o", "-i", image, file, "::/"])
            .output()?;
        if !output.status.success() {
            Err(format!("mcopy failed: {output:#?}"))?;
        }
    }
    Ok(())
}

fn unix_path(path: &Path) -> String {
    if cfg!(target_os = "windows") {
        let path_str = path.to_str().unwrap().replace('\\', "\\\\");
        let output = UnixCommand::new("wslpath")
            .args(["-a", &path_str])
            .output()
            .unwrap();
        std::str::from_utf8(&output.stdout)
            .unwrap()
            .trim()
            .to_string()
    } else {
        path.to_str().unwrap().to_string()
    }
}

// Defines [`UnixCommand`] that wraps [`Command`] with `wsl` command on Windows.
// On non-Windows platforms, it is an alias of [`Command`].
cfg_if::cfg_if! {
    if #[cfg(windows)] {
        struct UnixCommand {
            wsl: Command,
            program: String,
        }

        impl UnixCommand {
            fn new(program: &str) -> Self {
                Self {
                    wsl: Command::new("wsl"),
                    program: program.to_string(),
                }
            }

            pub(crate) fn args<I, S>(&mut self, args: I) -> &mut Command
            where
                I: IntoIterator<Item = S>,
                S: AsRef<std::ffi::OsStr>,
            {
                self.wsl.arg(self.program.clone()).args(args)
            }
        }
    } else {
        type UnixCommand = Command;
    }
}

#[cfg(test)]
mod tests {
    use crate::unix_path;
    use std::path::Path;

    #[test]
    fn test_unix_path() {
        if cfg!(target_os = "windows") {
            assert_eq!(unix_path(Path::new(r"C:\")), "/mnt/c/");
            assert_eq!(unix_path(Path::new("/tmp")), "/mnt/c/tmp");
        } else {
            assert_eq!(unix_path(Path::new(r"C:\")), r"C:\");
            assert_eq!(unix_path(Path::new("/tmp")), "/tmp");
        }
    }
}
