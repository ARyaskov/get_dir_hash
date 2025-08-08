//! Examples:
//!   get_dir_hash
//!   get_dir_hash ./mydir --ignore "target/**" --ignore-file .get_dir_hash_ignore --include-metadata

use get_dir_hash::{Options, get_dir_hash};
use pico_args::Arguments;
use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};
use time::OffsetDateTime;

fn print_help() {
    eprintln!(
        "\
get_dir_hash v{}
Usage: get_dir_hash [DIR] [--ignore PATTERN]... [--ignore-file FILE]... [--follow-symlinks] [--include-metadata] [--no-dotfile]
Options:
  DIR                   Directory to hash (default: .)
  --ignore PATTERN      Glob pattern to ignore (can repeat)
  --ignore-file FILE    Load patterns from a file (can repeat)
  --follow-symlinks     Follow symlinks while walking
  --include-metadata    Include basic metadata (mode + mtime) in the hash
  --no-dotfile          Do not auto-load .get_dir_hash_ignore from DIR
  -h, --help            Show help
",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> ExitCode {
    let mut pargs = Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print_help();
        return ExitCode::SUCCESS;
    }

    // Positional directory argument (default to ".")
    let dir: PathBuf = pargs.free_from_str().unwrap_or_else(|_| PathBuf::from("."));

    // Collect repeated options
    let ignores: Vec<String> = pargs.values_from_str("--ignore").unwrap_or_default();
    let ignore_files: Vec<PathBuf> = pargs.values_from_str("--ignore-file").unwrap_or_default();
    let follow = pargs.contains("--follow-symlinks");
    let include_meta = pargs.contains("--include-metadata");
    let no_dot = pargs.contains("--no-dotfile");

    let leftover: Vec<OsString> = pargs.finish();
    if !leftover.is_empty() {
        eprintln!("get_dir_hash: unexpected argument(s): {leftover:?}");
        return ExitCode::from(2);
    }

    // Build options in one go (no field reassignments)
    let opts = Options {
        follow_symlinks: follow,
        include_metadata: include_meta,
        ignore_patterns: ignores,
        ignore_files,
        load_dot_get_dir_hash_ignore: !no_dot,
        ..Default::default() // keep other defaults (e.g., case_sensitive_paths)
    };

    match get_dir_hash(&dir, &opts) {
        Ok(digest) => {
            let ts = OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            println!("{digest}  {}", dir.display());
            eprintln!("ok  {ts}  {}", dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("get_dir_hash: error: {e}");
            ExitCode::from(1)
        }
    }
}
