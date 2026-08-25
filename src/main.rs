use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "disk-clean",
    about = "Find and clean up Rust project target directories"
)]
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

    let spin = ProgressBar::new_spinner();
    spin.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spin.enable_steady_tick(Duration::from_millis(80));
    spin.set_message("Scanning ...");

    let mut targets = Vec::new();
    find_rust_targets(&root, 0, cli.max_depth, &mut targets, &spin);

    if targets.is_empty() {
        spin.finish_with_message("No Rust target directories found.");
        return;
    }

    spin.finish_and_clear();

    // Size calculation phase
    let pb = ProgressBar::new(targets.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} Calculating sizes [{bar:30.cyan/dim}] {pos}/{len}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(80));

    let sizes = du_sizes_with_progress(&targets, &pb);
    pb.finish_and_clear();

    let mut entries: Vec<(PathBuf, u64)> = targets.into_iter().zip(sizes).collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

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

    let del_pb = ProgressBar::new(entries.len() as u64);
    del_pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.red} Deleting [{bar:30.red/dim}] {pos}/{len} ({msg})")
            .unwrap()
            .progress_chars("=> "),
    );
    del_pb.enable_steady_tick(Duration::from_millis(80));

    let mut freed: u64 = 0;
    let mut errors = 0;
    for (path, size) in &entries {
        del_pb.set_message(format!("{}", human_size(freed)));
        match fs::remove_dir_all(path) {
            Ok(()) => freed += size,
            Err(e) => {
                del_pb.suspend(|| {
                    eprintln!("Failed to delete {}: {}", path.display(), e);
                });
                errors += 1;
            }
        }
        del_pb.inc(1);
    }

    del_pb.finish_and_clear();
    println!();
    println!("Freed: {}", human_size(freed));
    if errors > 0 {
        println!("Failed: {} directories", errors);
    }
}

/// Walk directories looking for Cargo.toml + target/ pairs.
/// Only reads directory listings — never stats files, never descends into target/.
fn find_rust_targets(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    results: &mut Vec<PathBuf>,
    spinner: &ProgressBar,
) {
    if depth > max_depth {
        return;
    }

    // Show the directory currently being scanned
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        let found = results.len();
        if found > 0 {
            spinner.set_message(format!("scanning {name} ... found {found}"));
        } else {
            spinner.set_message(format!("scanning {name} ..."));
        }
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut has_cargo_toml = false;
    let mut has_target = false;
    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        // Skip symlinks entirely
        if ft.is_symlink() {
            continue;
        }

        let name = entry.file_name();

        if ft.is_dir() {
            if name == "target" {
                has_target = true;
            } else {
                // Skip directories that will never contain Rust projects
                let n = name.to_string_lossy();
                if !should_skip(&n) {
                    subdirs.push(entry.path());
                }
            }
        } else if name == "Cargo.toml" {
            has_cargo_toml = true;
        }
    }

    if has_cargo_toml && has_target {
        results.push(dir.join("target"));
        // Don't recurse deeper — workspace members share the root target/
        return;
    }

    for subdir in subdirs {
        find_rust_targets(&subdir, depth + 1, max_depth, results, spinner);
    }
}

fn should_skip(name: &str) -> bool {
    // Most hidden directories contain caches or application data. Herdr is an
    // exception: it stores project worktrees that can contain large Rust targets.
    if name.starts_with('.') && name != ".herdr" {
        return true;
    }
    matches!(
        name,
        "node_modules"
            | "Library"
            | "cache"
            | "Cache"
            | "Caches"
            | "__pycache__"
            | "venv"
            | ".venv"
            | "dist"
            | "build"
            | "vendor"
            | "Pods"
            | "DerivedData"
    )
}

fn du_sizes_with_progress(paths: &[PathBuf], pb: &ProgressBar) -> Vec<u64> {
    paths
        .iter()
        .map(|p| {
            let size = du_size(p);
            pb.inc(1);
            size
        })
        .collect()
}

fn du_size(path: &PathBuf) -> u64 {
    Command::new("du")
        .arg("-sk")
        .arg(path)
        .output()
        .ok()
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout);
            text.split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .map(|kb| kb * 1024)
        })
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("disk-clean-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_target_inside_herdr_worktree() {
        let root = TestDir::new();
        let project = root
            .path()
            .join(".herdr/worktrees/rust-app-atlas/bug-finding");
        fs::create_dir_all(project.join("target")).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let mut targets = Vec::new();
        find_rust_targets(root.path(), 0, 10, &mut targets, &ProgressBar::hidden());

        assert_eq!(targets, vec![project.join("target")]);
    }

    #[test]
    fn still_skips_other_hidden_directories() {
        assert!(!should_skip(".herdr"));
        assert!(should_skip(".git"));
        assert!(should_skip(".cache"));
    }
}
