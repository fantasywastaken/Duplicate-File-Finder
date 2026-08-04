# Duplicate-File-Finder

Multithreaded Rust CLI that finds byte-identical duplicate files with SHA-256 and Rayon.

### ⚙️ How It Works

`dupfind` walks the target directory tree with `walkdir` and buckets every file it sees by its exact byte size. Only buckets that contain more than one file are ever hashed, so unique-size files are eliminated for free. Each surviving bucket is then fanned out across the CPU with a Rayon work-stealing pool, and files are streamed through a SHA-256 hasher in 64 KiB chunks so memory stays flat regardless of file size. Any hash that lands more than once inside the same size bucket produces a duplicate group. Groups are sorted by wasted space so the biggest offenders come first, and an optional interactive delete mode lets you keep one file per group and delete the rest.

## 📁 Setup

```bash
git clone <this-repo>
cd Duplicate-File-Finder
cargo build --release
```

The release binary lands at `target/release/dupfind` (`.exe` on Windows).

### 🚀 Usage

```bash
dupfind ./target-folder
dupfind ./target-folder --min-size 1MB
dupfind /data --min-size 100MB --delete-interactive
```

| Flag | Description |
|------|-------------|
| `[PATH]` | Root directory to scan (default: `.`) |
| `--min-size <SIZE>` | Skip files smaller than this. Accepts `B`, `KB`, `MB`, `GB` suffixes |
| `--delete-interactive` | For each duplicate group, prompt to keep one file and delete the rest |

Progress messages are written to `stderr` so `stdout` stays clean for piping.

### ✨ Features

- Size-bucketing pre-filter so unique-size files are never opened
- Cryptographic SHA-256 hashing streamed in 64 KiB chunks (constant memory)
- Fully multithreaded via Rayon's work-stealing pool
- Human-readable size parsing (`500KB`, `1MB`, `2GB`) and pretty output
- Interactive delete mode with per-group prompts
- Progress reporting on `stderr` so pipes stay clean
- Cross-platform: Windows, Linux, macOS
