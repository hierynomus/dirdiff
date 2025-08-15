use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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

fn print_diff(dir_a: &Path, dir_b: &Path) {
    let files_a = collect_files(dir_a);
    let files_b = collect_files(dir_b);

    let mut missing_in_b: Vec<_> = files_a.difference(&files_b).cloned().collect();
    missing_in_b.sort();

    let mut missing_in_a: Vec<_> = files_b.difference(&files_a).cloned().collect();
    missing_in_a.sort();

    if missing_in_a.is_empty() && missing_in_b.is_empty() {
        println!("  {GREEN}✅ identical file sets{RESET}");
        return;
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <directory_A> <directory_B>", args[0]);
        std::process::exit(1);
    }

    let dir_a = Path::new(&args[1]);
    let dir_b = Path::new(&args[2]);

    if !dir_a.is_dir() || !dir_b.is_dir() {
        eprintln!("Both arguments must be valid directories.");
        std::process::exit(1);
    }

    // Gather ALL unique direct subdirectories from both sides
    let all_subdirs: HashSet<PathBuf> = direct_subdirs(dir_a)
        .union(&direct_subdirs(dir_b))
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
            (true, true) => print_diff(&path_a, &path_b),
            (true, false) => println!("  {RED}Present in {} but MISSING entirely in {}{RESET}", dir_a.display(), dir_b.display()),
            (false, true) => println!("  {RED}Present in {} but MISSING entirely in {}{RESET}", dir_b.display(), dir_a.display()),
            _ => (),
        }
    }

    Ok(())
}
