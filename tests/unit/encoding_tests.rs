use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Scans tracked source and prose files for the byte sequence `0xC3 0xA2`,
/// the leading bytes of every Windows-1252-mojibaked UTF-8 character (`U+00E2`
/// followed by another high byte). That pattern is the fingerprint of UTF-8
/// text once decoded as cp1252 and re-encoded as UTF-8 — what users see as
/// scrambled sparkles, arrows, em-dashes, and triangles in the TUI. If
/// anything matches, the test prints the offending file:line so the operator
/// can fix the source bytes instead of relying on terminal rendering to
/// surface the regression.
#[test]
fn source_tree_has_no_windows_1252_mojibake() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Restrict to source and prose paths that surface in the TUI or in
    // docs the user reads. `tests/` is intentionally excluded — fixtures
    // there legitimately exercise non-ASCII characters like `àáâãäå`.
    let scan_roots = ["src", "docs", "README.md", "CHANGELOG.md"];

    let mut hits = Vec::new();

    for entry in scan_roots {
        let path = root.join(entry);
        if !path.exists() {
            continue;
        }
        let walker = WalkBuilder::new(&path)
            .hidden(false)
            .git_ignore(true)
            .build();
        for result in walker {
            let dent = match result {
                Ok(d) => d,
                Err(_) => continue,
            };
            if !dent.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            scan_file(dent.path(), &root, &mut hits);
        }
    }

    assert!(
        hits.is_empty(),
        "Windows-1252 mojibake detected (byte sequence C3 A2) at:\n  {}\n\
         This is the leftover from a UTF-8 string that was once decoded as \
         cp1252 and re-encoded as UTF-8. Re-save the file with the intended \
         codepoints (em-dash, arrows, sparkles, etc.).",
        hits.join("\n  ")
    );
}

fn scan_file(path: &Path, root: &Path, hits: &mut Vec<String>) {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    // Skip files that look binary: a NUL byte in the first 8 KiB is a strong
    // hint and matches how `git diff` decides binary.
    let head_len = bytes.len().min(8192);
    if bytes[..head_len].contains(&0) {
        return;
    }

    let mut line = 1usize;
    let mut col = 1usize;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0xC3 && bytes[i + 1] == 0xA2 {
            let rel = path.strip_prefix(root).unwrap_or(path);
            hits.push(format!("{}:{}:{}", rel.display(), line, col));
            // Skip past this hit so a single bad string is reported once.
            i += 2;
            col += 2;
            continue;
        }
        if bytes[i] == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        i += 1;
    }
}
