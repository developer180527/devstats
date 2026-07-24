//! Traversal and measurement.
//!
//! Phase 1 walks the tree collecting paths and sizes (cheap, gitignore-aware).
//! Phase 2 classifies and counts in parallel. Splitting the two keeps the
//! classifier's memoised nested-repo lookups useful and lets rayon saturate
//! the machine on the expensive half.

use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::classify::{Classifier, Role};
use crate::count::{self, Counts};
use crate::lang::{self, Language};

pub struct FileStat {
    /// Path relative to the scan root.
    pub path: PathBuf,
    pub size: u64,
    pub role: Role,
    pub reason: &'static str,
    pub lang: Option<&'static Language>,
    pub counts: Counts,
    /// True when lines were not counted (binary, unreadable, too large, or
    /// deliberately skipped under --fast).
    pub uncounted: bool,
}

pub struct Scan {
    pub root: PathBuf,
    pub files: Vec<FileStat>,
    /// Paths that could not be read, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
    pub walk_errors: usize,
    pub submodules: usize,
}

pub struct ScanOptions {
    pub hidden: bool,
    pub respect_ignore: bool,
    pub follow_links: bool,
    pub fast: bool,
    pub max_file_bytes: u64,
    pub detect_markers: bool,
}

/// Bytes of a file read purely to look for a generator banner.
const HEAD_PROBE: usize = 512;

pub fn scan(root: &Path, classifier: &Classifier, opts: &ScanOptions) -> Scan {
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(!opts.hidden)
        .parents(opts.respect_ignore)
        .git_ignore(opts.respect_ignore)
        .git_global(opts.respect_ignore)
        .git_exclude(opts.respect_ignore)
        .ignore(opts.respect_ignore)
        .follow_links(opts.follow_links);

    // The scan root's own .git is never interesting, and skipping it early
    // saves walking thousands of loose objects.
    walker.filter_entry(|e| e.file_name() != ".git");

    let mut candidates: Vec<(PathBuf, u64)> = Vec::new();
    let mut walk_errors = 0usize;

    for result in walker.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => {
                walk_errors += 1;
                continue;
            }
        };
        let Some(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        if rel.as_os_str().is_empty() || classifier.is_filtered_out(rel) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        candidates.push((rel.to_path_buf(), size));
    }

    let results: Vec<(FileStat, Option<(PathBuf, String)>)> = candidates
        .into_par_iter()
        .map(|(rel, size)| measure(root, rel, size, classifier, opts))
        .collect();

    let mut files = Vec::with_capacity(results.len());
    let mut skipped = Vec::new();
    for (stat, skip) in results {
        if let Some(s) = skip {
            skipped.push(s);
        }
        files.push(stat);
    }

    Scan {
        root: root.to_path_buf(),
        files,
        skipped,
        walk_errors,
        submodules: classifier.submodule_count(),
    }
}

fn measure(
    root: &Path,
    rel: PathBuf,
    size: u64,
    classifier: &Classifier,
    opts: &ScanOptions,
) -> (FileStat, Option<(PathBuf, String)>) {
    let language = lang::detect(&rel);

    // Classify from the path alone first. If that is already enough to rule the
    // file out of the line counts, the file is never opened — which is what
    // keeps a 6 GB build directory cheap to report on.
    let provisional = classifier.classify(&rel, None);
    let mut stat = FileStat {
        path: rel,
        size,
        role: provisional.role,
        reason: provisional.reason,
        lang: language,
        counts: Counts::default(),
        uncounted: true,
    };

    if !wants_lines(stat.role, language, opts) {
        return (stat, None);
    }
    if size > opts.max_file_bytes {
        let note = format!("{} exceeds --max-file-size", crate::report::format_bytes(size));
        let skipped = stat.path.clone();
        return (stat, Some((skipped, note)));
    }

    let bytes = match fs::read(root.join(&stat.path)) {
        Ok(b) => b,
        Err(e) => {
            let note = e.to_string();
            let skipped = stat.path.clone();
            return (stat, Some((skipped, note)));
        }
    };
    if count::is_binary(&bytes) {
        let skipped = stat.path.clone();
        return (stat, Some((skipped, "binary content".to_string())));
    }
    let text = String::from_utf8_lossy(&bytes);

    // The content is in hand, so re-run classification with the file head: a
    // generator banner can still demote a file out of the project total.
    if opts.detect_markers {
        let head_end = text.char_indices().map(|(i, _)| i).nth(HEAD_PROBE).unwrap_or(text.len());
        let reclassified = classifier.classify(&stat.path, Some(&text[..head_end]));
        stat.role = reclassified.role;
        stat.reason = reclassified.reason;
        if !wants_lines(stat.role, language, opts) {
            return (stat, None);
        }
    }

    stat.counts = count::count(&text, language.expect("a countable file always has a language"));
    stat.uncounted = false;
    (stat, None)
}

/// Whether this file's lines should be counted at all. Docs and config are
/// counted (they are lines someone maintains); assets, unrecognised types and
/// build output are measured by size only.
fn wants_lines(role: Role, language: Option<&'static Language>, opts: &ScanOptions) -> bool {
    let Some(l) = language else { return false };
    if l.kind == lang::FileKind::Asset || !role.worth_counting() {
        return false;
    }
    !(opts.fast && matches!(role, Role::Vendor | Role::Generated))
}
