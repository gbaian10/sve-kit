//! Discover repository runtime scenarios without a shared module registry.

use core::error::Error;
use core::fmt::Write as _;
use std::env;
use std::fs;
use std::path::PathBuf;

#[expect(
    clippy::use_debug,
    clippy::unnecessary_debug_formatting,
    reason = "Generated path attributes need quoted and escaped Rust string literals."
)]
fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let directory = root.join("tests/current_repository/runtime");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut paths = fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "rs"));
    paths.sort();
    let mut modules = String::new();
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("Invalid runtime module filename")?;
        writeln!(modules, "#[path = {path:?}] mod r#{name};")?;
    }
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("runtime_modules.rs"),
        modules,
    )?;
    Ok(())
}
