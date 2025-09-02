use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use clap::Parser;
use sha2::{Digest, Sha256};

// ANSI color escape codes (no external crate needed)
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

/// Recursively collects **file** paths (relative to `root`) into a `HashSet`.
fn collect_files(root: &Path) -> HashSet<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = HashSet::new();

    while let Some(current) = stack.pop() {
        if current.is_dir() {
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            }
        } else if current.is_file() {
            if let Ok(relative) = current.strip_prefix(root) {
                files.insert(relative.to_path_buf());
            }
        }
    }

    files
}

/// Returns the set of **direct** subdirectories (relative to `root`).
fn direct_subdirs(root: &Path) -> HashSet<PathBuf> {
    let mut dirs = HashSet::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(rel) = path.strip_prefix(root) {
                    dirs.insert(rel.to_path_buf());
                }
            }
        }
    }
    dirs
}

/// Stream a file and return its SHA-256 digest.
fn hash_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }

    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

/// Returns `Ok(true)` if file contents differ. Uses size check first, then SHA-256.
fn contents_differ(a: &Path, b: &Path) -> io::Result<bool> {
    let ma = fs::metadata(a)?;
    let mb = fs::metadata(b)?;
    if ma.len() != mb.len() {
        return Ok(true);
    }
    Ok(hash_file(a)? != hash_file(b)?)
}

fn print_diff(dir_a: &Path, dir_b: &Path, check_hash: bool) {
    let files_a = collect_files(dir_a);
    let files_b = collect_files(dir_b);

    // Missing files
    let mut missing_in_b: Vec<_> = files_a.difference(&files_b).cloned().collect();
    missing_in_b.sort();

    let mut missing_in_a: Vec<_> = files_b.difference(&files_a).cloned().collect();
    missing_in_a.sort();

    // Common files (present in both) to check content equality (optional)
    let mut changed: Vec<PathBuf> = Vec::new();
    let mut errored: Vec<(PathBuf, String)> = Vec::new();

    if check_hash {
        let mut common: Vec<_> = files_a.intersection(&files_b).cloned().collect();
        common.sort();
        for rel in &common {
            let pa = dir_a.join(rel);
            let pb = dir_b.join(rel);
            match contents_differ(&pa, &pb) {
                Ok(true) => changed.push(rel.clone()),
                Ok(false) => {},
                Err(e) => errored.push((rel.clone(), e.to_string())),
            }
        }
    }

    let only_structure_equal = missing_in_a.is_empty() && missing_in_b.is_empty();

    if !check_hash {
        if only_structure_equal {
            println!("  {GREEN}✅ identical file sets (skipped content check){RESET}");
        }
    } else if only_structure_equal && changed.is_empty() && errored.is_empty() {
        println!("  {GREEN}✅ identical files and contents{RESET}");
    }

    if !missing_in_b.is_empty() {
        println!(
            "  {YELLOW}Files present in {a} but MISSING in {b}:{RESET}",
            a = dir_a.display(),
            b = dir_b.display()
        );
        for p in &missing_in_b {
            println!("    {RED}{}{RESET}", p.display());
        }
    }

    if !missing_in_a.is_empty() {
        println!(
            "  {YELLOW}Files present in {b} but MISSING in {a}:{RESET}",
            a = dir_a.display(),
            b = dir_b.display()
        );
        for p in &missing_in_a {
            println!("    {RED}{}{RESET}", p.display());
        }
    }

    if check_hash && !changed.is_empty() {
        println!("  {YELLOW}Files present in BOTH but with DIFFERENT CONTENT:{RESET}");
        for p in &changed {
            println!("    {RED}{}{RESET}", p.display());
        }
    }

    if check_hash && !errored.is_empty() {
        println!("  {YELLOW}Files that could not be compared (errors):{RESET}");
        for (p, e) in &errored {
            println!("    {RED}{} — {}{RESET}", p.display(), e);
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "dir_compare", version, about = "Compare directory structures (and optionally contents) by subdirectory.")]
struct Cli {
    /// First directory to compare
    #[arg(value_name = "DIRECTORY_A")]
    dir_a: PathBuf,
    /// Second directory to compare
    #[arg(value_name = "DIRECTORY_B")]
    dir_b: PathBuf,
    /// Also compare file contents using SHA-256
    #[arg(long)]
    hash: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let dir_a = cli.dir_a;
    let dir_b = cli.dir_b;
    let check_hash = cli.hash;

    if !dir_a.is_dir() || !dir_b.is_dir() {
        eprintln!("Both arguments must be valid directories.");
        std::process::exit(1);
    }

    // Gather ALL unique direct subdirectories from both sides
    let all_subdirs: HashSet<PathBuf> = direct_subdirs(&dir_a)
        .union(&direct_subdirs(&dir_b))
        .cloned()
        .collect();

    // NOTE: we no longer include the root – user asked to skip it

    // Sort for deterministic order
    let mut subdirs: Vec<_> = all_subdirs.into_iter().collect();
    subdirs.sort();

    for sub in &subdirs {
        let path_a = dir_a.join(sub);
        let path_b = dir_b.join(sub);
        let label = sub.display();

        println!("\n{CYAN}=== Subdirectory: {} ==={RESET}", label);

        match (path_a.is_dir(), path_b.is_dir()) {
            (true, true) => print_diff(&path_a, &path_b, check_hash),
            (true, false) => println!("  {RED}Present in {} but MISSING entirely in {}{RESET}", dir_a.display(), dir_b.display()),
            (false, true) => println!("  {RED}Present in {} but MISSING entirely in {}{RESET}", dir_b.display(), dir_a.display()),
            _ => (),
        }
    }

    Ok(())
}
