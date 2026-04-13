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

1. Recursively walks directories looking for `Cargo.toml` + `target/` pairs
2. Stops recursing once a Rust project is found (workspace members share the root `target/`)
3. Skips irrelevant directories (`.git`, `node_modules`, `cache`, `Library`, etc.)
4. Uses system `du` for fast size calculation
5. Shows progress with spinners and progress bars

## License

MIT
