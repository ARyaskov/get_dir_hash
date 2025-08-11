# get_dir_hash

[![CI](https://github.com/ARyaskov/get_dir_hash/actions/workflows/ci.yml/badge.svg)](https://github.com/ARyaskov/get_dir_hash/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/get_dir_hash.svg)](https://crates.io/crates/get_dir_hash)
![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/get_dir_hash)

> Deterministic **directory hashing** with glob ignores and optional metadata — powered by **BLAKE3**.  
> Tiny, fast, and predictable. Great for cache keys, change detection, CI, and reproducible builds.

---

## Features

- ✅ **Deterministic**: stable walk order & path framing → identical trees → identical digests
- 🚀 **Fast**: streams file contents; BLAKE3 under the hood
- 🧹 **Ignores**: simple `.gitignore`-like **glob** rules (via `globset`)
- 🧾 **Metadata (opt-in)**: include file mode (Unix) & mtime (secs/nanos)
- 🖇️ **Symlinks**: optionally follow symlinks during traversal
- 🧰 **Tiny**: zero heavy deps (just `blake3`, `globset`, `walkdir`, tiny CLI parser)

---

## Install

```bash
# CLI
cargo install get_dir_hash

# Library
cargo add get_dir_hash
````

---

## CLI usage

```bash
# hash current directory
get_dir_hash

# pick a dir
get_dir_hash ./my-project

# ignore patterns (can be repeated)
get_dir_hash --ignore "target/**" --ignore "**/*.log"

# load patterns from a file
get_dir_hash --ignore-file .get_dir_hashignore

# follow symlinks and include basic metadata (mode + mtime)
get_dir_hash --follow-symlinks --include-metadata

# disable auto-loading of .get_dir_hashignore in root
get_dir_hash --no-dotfile
```

`get_dir_hash` also **auto-loads `.get_dir_hash_ignore`** from the root directory unless `--no-dotfile` is passed.

**Example `.get_dir_hash_ignore`:**

```
# ignore build artifacts and logs
target/**
**/*.log
*.tmp
```

**Output format**:

```
<hex-digest>  <path>
```

---

## Library usage

```rust
use get_dir_hash::{Options, get_dir_hash};
use std::path::Path;

fn main() -> std::io::Result<()> {
    let mut opts = Options::default();
    opts.ignore_patterns = vec!["target/**".into(), "**/*.tmp".into()];
    // opts.include_metadata = true;        // opt-in
    // opts.follow_symlinks = true;         // opt-in
    let digest = get_dir_hash(Path::new("."), &opts)?;
    println!("{digest}");
    Ok(())
}
```

---

## What exactly is hashed?

For every regular file (after ignore rules):

* **Framing**: we feed the outer BLAKE3 hasher with a domain tag `b"get_dir_hash-v1\0"` and, per file, a record:

  ```
  b"F\0" + <normalized-relative-path> + b"\0" + <BLAKE3(content)>
  ```
* **Optional metadata** (`--include-metadata` / `Options::include_metadata`):

    * Unix: file **mode** is included.
    * All platforms: **mtime** as `(secs, nanos)` is included.

Relative paths are normalized to Unix-style separators (`/`).
Ordering is stable (sorted by normalized path). You can also opt into case-insensitive path ordering via `Options` if needed for Windows-like behavior in caches.

---

## Ignore rules

* Syntax provided by [`globset`](https://docs.rs/globset): supports `**`, `*`, `?`, etc.
* Patterns are evaluated **relative to the root**.
* **Not supported**: `!`-negations.
* Sources of patterns:

    1. Inline via `--ignore` / `Options::ignore_patterns`
    2. Files via `--ignore-file` / `Options::ignore_files`
    3. Auto-loaded `.get_dir_hash_ignore` in root (unless `--no-dotfile`)

---

## Why BLAKE3?

* **Cryptographically strong** and **very fast**
* Designed for parallelism and modern CPUs
* Widely used in the Rust ecosystem (`blake3` crate)

---

## Determinism

* Path normalization and **sorted** relative paths ensure stable input order.
* Hash framing with domain tags and zero byte separators removes ambiguity.
* Ignores and metadata flags must be identical across runs for equal outputs.

---

## Notes & caveats

* Only **regular files** are hashed. Directories and device nodes are skipped.
* **Symlinks** are not followed by default (`Options::follow_symlinks = false`).
* **Metadata** inclusion is optional. If enabled, the digest can change even when contents stay the same (e.g., mtime updates).
* Paths are normalized to use `/` as a separator in the digest framing.

---

## Rust Version

Tested with Rust v1.88

---

## CI & Releases

* CI runs on Linux/macOS/Windows (build, test, clippy, fmt).
* GitHub Releases attach prebuilt binaries for common targets when pushing a tag like `v0.1.0`.

---

## License

Licensed under **MIT**.

---

## Contributing

Issues and PRs are welcome!
Please keep changes minimal and deterministic, and avoid heavy dependencies.
Cheers!
