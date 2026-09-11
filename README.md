# disk-clean

A fast CLI tool to find and clean up Rust project `target/` directories that eat up disk space.

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Scan your home directory (dry-run, just lists what it finds)
disk-clean

# Scan a specific directory
disk-clean ~/projects

# Actually delete the target directories (asks for confirmation)
disk-clean --clean

# Delete without confirmation
disk-clean --clean -y

# Limit search depth
disk-clean --max-depth 5
```

## Example output

```
SIZE       PATH
----       ----
3.7 GB     /home/user/projects/arrow-rs/target
1.5 GB     /home/user/projects/datafusion/target
118.9 MB   /home/user/projects/moka/target

Found 3 target directories, total: 5.3 GB

Run with --clean to delete these directories.
```

## How it works

1. Recursively walks directories looking for `Cargo.toml` + `target/` pairs, and custom Cargo output directories containing both a regular `.rustc_info.json` file and a regular `CACHEDIR.TAG` file with a valid cache signature. Directories containing `Cargo.toml` are never selected as custom output directories.
2. Searches `.herdr` and `.context`, including nested worktrees and custom outputs such as `.context/ordered-target`, even when the enclosing project already has a `target/`. Recognized build output directories are not searched further.
3. Skips symlinks, other hidden directories, and irrelevant directories (`node_modules`, `cache`, `Library`, etc.)
4. Uses system `du` for fast size calculation
5. Shows progress with spinners and progress bars

## License

MIT
