//! Deterministic directory hashing with simple glob ignores.
//!
//! Design:
//! - Build a file list by walking `root` and filtering via `globset`.
//! - Sort files by normalized relative path to guarantee stable order.
//! - For each file: stream its content into an *inner* blake3 hasher,
//!   then feed the outer hasher with record-framed data:
//!     b"F\0" + path + b"\0" + content_digest + [metadata?].
//! - Finally, return the outer digest as lowercase hex.
//!
//! This crate intentionally keeps ignore semantics minimal (no `!` negations).

use blake3::{Hash as Blake3Hash, Hasher as Blake3};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

/// Options controlling hashing behavior.
#[derive(Debug, Clone)]
pub struct Options {
    /// Follow symlinks when walking the tree.
    pub follow_symlinks: bool,
    /// Include basic metadata (mode on Unix, and (secs,nanos) mtime on all).
    pub include_metadata: bool,
    /// Treat path comparison as case-sensitive. If `false`, we lowercase paths
    /// before sorting and framing (helps Windows).
    pub case_sensitive_paths: bool,
    /// Extra ignore patterns (applied relative to the root).
    pub ignore_patterns: Vec<String>,
    /// Paths to files with ignore patterns (line-based, `#` comments).
    pub ignore_files: Vec<PathBuf>,
    /// Whether to auto-load `.get_dir_hash_ignore` from root.
    pub load_dot_get_dir_hash_ignore: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            include_metadata: false,
            case_sensitive_paths: true,
            ignore_patterns: Vec::new(),
            ignore_files: Vec::new(),
            load_dot_get_dir_hash_ignore: true,
        }
    }
}

/// Compute dir hash for `root` using `opts`, returning a lowercase hex digest.
pub fn get_dir_hash(root: &Path, opts: &Options) -> io::Result<String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let globset = build_globset(&root, opts)?;

    // Collect files (not directories) first.
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    let walker = WalkDir::new(&root)
        .follow_links(opts.follow_symlinks)
        .into_iter();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                // Skip unreadable entries, but keep going.
                // Professional tradeoff: you can decide to error instead.
                eprintln!("get_dir_hash: warn: skipping entry: {e}");
                continue;
            }
        };
        let path = entry.path();

        if !entry.file_type().is_file() {
            continue;
        }
        // Normalize & relativize path.
        let rel = match make_rel_unix(&root, path) {
            Some(s) => s,
            None => continue, // shouldn't happen
        };

        // Apply ignore patterns relative to root.
        if globset.is_match(&rel) {
            continue;
        }

        files.push((rel, path.to_path_buf()));
    }

    // Stable order (by normalized relative path).
    files.sort_by(|a, b| {
        if opts.case_sensitive_paths {
            a.0.cmp(&b.0)
        } else {
            cmp_case_insensitive(&a.0, &b.0)
        }
    });

    // Outer stream hasher.
    let mut out = Blake3::new();
    out.update(b"get_dir_hash-v1\0");

    for (rel, path) in files {
        let mut inner = Blake3::new();
        stream_file(&path, &mut inner)?;
        let content_digest = inner.finalize();

        out.update(b"F\0");
        if opts.case_sensitive_paths {
            out.update(rel.as_bytes());
        } else {
            out.update(rel.to_lowercase().as_bytes());
        }
        out.update(b"\0");
        out.update(content_digest.as_bytes());

        if opts.include_metadata {
            if let Ok(md) = fs::metadata(&path) {
                feed_metadata(&mut out, &md);
            }
        }
    }

    let digest = out.finalize();
    Ok(hex_lower(digest.as_bytes()))
}

/// Build a GlobSet from patterns in `opts` and optional `.get_dir_hash_ignore`.
fn build_globset(root: &Path, opts: &Options) -> io::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();

    // Load .get_dir_hash_ignore if requested.
    if opts.load_dot_get_dir_hash_ignore {
        let f = root.join(".get_dir_hash_ignore");
        if f.is_file() {
            load_patterns_file(&f, &mut builder)?;
        }
    }

    // Load any additional ignore files.
    for file in &opts.ignore_files {
        if file.is_file() {
            load_patterns_file(file, &mut builder)?;
        }
    }

    // Add inline patterns.
    for p in &opts.ignore_patterns {
        // Patterns are relative to root; we normalize separators to '/'.
        let pat = p.replace('\\', "/");
        builder.add(Glob::new(&pat).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?);
    }

    Ok(builder
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?)
}

/// Load ignore patterns from file (one per line, '#' comments).
fn load_patterns_file(path: &Path, builder: &mut GlobSetBuilder) -> io::Result<()> {
    let txt = fs::read_to_string(path)?;
    for raw in txt.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // We do not support leading '!' negation (keep the crate tiny).
        if line.starts_with('!') {
            // Ignore silently for now;
            continue;
        }
        let pat = line.replace('\\', "/");
        let g = Glob::new(&pat).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        builder.add(g);
    }
    Ok(())
}

/// Stream a file into `hasher` using a fixed-size buffer.
fn stream_file(path: &Path, hasher: &mut Blake3) -> io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(())
}

/// Feed a minimal, platform-neutral metadata frame.
fn feed_metadata(out: &mut Blake3, md: &Metadata) {
    out.update(b"\0M\0");
    // Mode (Unix) or readonly bit (cross-platform fallback).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = md.permissions().mode();
        out.update(&mode.to_le_bytes());
    }
    #[cfg(not(unix))]
    {
        let ro = md.permissions().readonly();
        out.update(&[ro as u8]);
    }

    // mtime (secs, nanos) — if available.
    if let Ok(mt) = md.modified() {
        if let Ok(dur) = mt.duration_since(std::time::UNIX_EPOCH) {
            out.update(&dur.as_secs().to_le_bytes());
            out.update(&(dur.subsec_nanos()).to_le_bytes());
        }
    }
}

/// Make a Unix-style relative path (with `/` separators).
fn make_rel_unix(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(path_to_unix_string(rel))
}

/// Convert path to a Unix-ish string (no `.`/`..`, `/` as sep).
fn path_to_unix_string(p: &std::path::Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = parts.pop();
            }
            std::path::Component::Normal(s) => {
                parts.push(s.to_string_lossy().into_owned());
            }
            _ => {}
        }
    }
    parts.join("/")
}

/// Case-insensitive comparison (ASCII fast-path; OK for ordering only).
fn cmp_case_insensitive(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
}

/// Hex-encode to lowercase without allocation churn.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}
