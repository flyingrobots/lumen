//! The edge of the compiler layer's check: finds the Clippy configuration files, runs Clippy on
//! the fixture and for its group table, and reads the fixture's sources.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

use super::{ban_groups, diagnostics, entries, forms, judge, std_items, FIXTURE_SRC, STD_ITEMS};

/// The check over the repository at `root`; true when it passes. Prints each failure.
pub(crate) fn gate(root: &Path) -> io::Result<bool> {
    let mut failures = Vec::new();
    let mut configs = Vec::new();
    config_files(root, root, &mut configs)?;
    if configs != ["clippy.toml"] {
        failures.push(format!(
            "Clippy configuration files {configs:?}: exactly one, the root clippy.toml, may exist \
             (a crate's own replaces the root one for that crate)"
        ));
    }
    if std::env::var_os("CLIPPY_CONF_DIR").is_some() {
        failures.push("CLIPPY_CONF_DIR is set: it replaces the root clippy.toml".to_owned());
    }
    let toml = fs::read_to_string(root.join("clippy.toml"))?;
    // `--color=never`: CI sets `CARGO_TERM_COLOR=always`, and coloured lines do not parse.
    let fixture = [
        "clippy",
        "-p",
        "lumen-clippy-fixture",
        "--locked",
        "--color=never",
        "--message-format=short",
    ];
    let found = cargo(root, &[&fixture[..], &["--", "--cfg", "lumen_clippy_fixture"]].concat())?;
    let help = cargo(root, &[&fixture[..], &["--", "-W", "help"]].concat())?;
    let mut all_forms = Vec::new();
    for entry in fs::read_dir(root.join(FIXTURE_SRC))? {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.extension().is_some_and(|ext| ext == "rs") {
            all_forms.extend(forms(&format!("{FIXTURE_SRC}/{name}"), &fs::read_to_string(&path)?));
        }
    }
    let entries = entries(&toml);
    failures.extend(judge(&entries, &all_forms, &diagnostics(&found), &ban_groups(&help)));
    let list = fs::read_to_string(root.join(STD_ITEMS))?;
    let toolchain = fs::read_to_string(root.join("rust-toolchain.toml"))?;
    failures.extend(std_items::toolchain_matches(&list, &toolchain));
    failures.extend(std_items::check(&std_items::parse(&list), &entries));
    for failure in &failures {
        println!("::error::[clippy-bans] {failure}");
    }
    println!(
        "clippy-bans: {} entries, {} seeded forms and look-alikes, {} failing",
        entries.len(),
        all_forms.len(),
        failures.len()
    );
    Ok(failures.is_empty())
}

/// Runs cargo in `root` with `args`; its standard output and error, together.
fn cargo(root: &Path, args: &[&str]) -> io::Result<String> {
    let cargo = std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let output = Command::new(cargo).args(args).current_dir(root).output()?;
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Adds the path, relative to `root`, of every `clippy.toml` and `.clippy.toml` under `dir` to
/// `out`, skipping `target` and `.git`.
fn config_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if name != "target" && name != ".git" {
                config_files(root, &path, out)?;
            }
        } else if name == "clippy.toml" || name == ".clippy.toml" {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    Ok(())
}

/// The std item list for the docs at `std_docs` (the `share/doc/rust/html/std` directory of
/// rust-docs), for `toolchain`, as `xtask/bans/std-items.txt` holds it.
pub(crate) fn std_items_list(std_docs: &Path, toolchain: &str) -> io::Result<String> {
    let all = fs::read_to_string(std_docs.join("all.html"))?;
    let items = std_items::extract(&all, &|href| fs::read_to_string(std_docs.join(href)).ok());
    Ok(std_items::render(&items, toolchain))
}
