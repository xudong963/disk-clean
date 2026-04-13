use clap::Parser;
use rayon::prelude::*;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Parser)]
#[command(name = "disk-clean", about = "Find and clean up Rust project target directories")]
struct Cli {
    /// Root directory to scan (defaults to home directory)
    #[arg(default_value_t = default_scan_path())]
    path: String,

    /// Actually delete the target directories (default is dry-run)
    #[arg(long)]
    clean: bool,

    /// Skip confirmation prompt (use with --clean)
    #[arg(long, short = 'y')]
    yes: bool,

    /// Maximum directory depth to search
    #[arg(long, default_value_t = 10)]
    max_depth: usize,
}

fn default_scan_path() -> String {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .unwrap_or_else(|| ".".into())
}

fn main() {
    let cli = Cli::parse();
    let root = PathBuf::from(&cli.path);

    if !root.is_dir() {
        eprintln!("Error: '{}' is not a directory", cli.path);
        std::process::exit(1);
    }

    let found = AtomicUsize::new(0);

    let mut targets = Vec::new();
    find_rust_targets(&root, 0, cli.max_depth, &mut targets, &found);

    eprintln!("\rScanning ... found {} target directories", targets.len());
    eprint!("Calculating sizes ...");

    // Calculate sizes in parallel using rayon
    let mut entries: Vec<(PathBuf, u64)> = targets
        .into_par_iter()
        .map(|p| {
            let size = dir_size(&p);
            (p, size)
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    eprintln!(" done");
    println!();

    if entries.is_empty() {
        println!("No Rust target directories found.");
        return;
    }

    let total: u64 = entries.iter().map(|(_, s)| *s).sum();

    println!("{:<10} {}", "SIZE", "PATH");
    println!("{:<10} {}", "----", "----");
    for (path, size) in &entries {
        println!("{:<10} {}", human_size(*size), path.display());
    }
    println!();
    println!(
        "Found {} target directories, total: {}",
        entries.len(),
        human_size(total)
    );

    if !cli.clean {
        println!();
        println!("Run with --clean to delete these directories.");
        return;
    }

    if !cli.yes {
        print!("\nDelete all {} directories? [y/N] ", entries.len());
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return;
        }
    }

    let mut freed: u64 = 0;
    let mut errors = 0;
    for (path, size) in &entries {
        match fs::remove_dir_all(path) {
            Ok(()) => {
                println!("Deleted: {} ({})", path.display(), human_size(*size));
                freed += size;
            }
            Err(e) => {
                eprintln!("Failed to delete {}: {}", path.display(), e);
                errors += 1;
            }
        }
    }

    println!();
    println!("Freed: {}", human_size(freed));
    if errors > 0 {
        println!("Failed: {} directories", errors);
    }
}

/// Recursively find `target/` directories that sit next to a `Cargo.toml`.
fn find_rust_targets(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    results: &mut Vec<PathBuf>,
    found: &AtomicUsize,
) {
    if depth > max_depth {
        return;
    }

    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name == "node_modules" || name == "Library" {
            return;
        }
    }

    let has_cargo_toml = dir.join("Cargo.toml").is_file();
    let target_dir = dir.join("target");

    if has_cargo_toml && target_dir.is_dir() {
        let n = found.fetch_add(1, Ordering::Relaxed) + 1;
        eprint!("\rScanning ... found {:<6}", n);
        results.push(target_dir);
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "target" {
                continue;
            }
            find_rust_targets(&path, depth + 1, max_depth, results, found);
        }
    }
}

/// Calculate total size of a directory using jwalk for parallel traversal.
fn dir_size(path: &Path) -> u64 {
    jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
