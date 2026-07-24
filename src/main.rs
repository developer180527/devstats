//! devstat — codebase statistics that tell project code apart from vendored,
//! generated and build output.

mod classify;
mod cli;
mod count;
mod git;
mod lang;
mod report;
mod scan;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use colored::*;
use globset::{Glob, GlobSetBuilder};

use classify::Classifier;
use cli::{Args, Format};
use report::Aggregates;
use scan::ScanOptions;

fn main() -> ExitCode {
    let args = Args::parse();

    if args.no_color || std::env::var_os("NO_COLOR").is_some() || args.format != Format::Table {
        colored::control::set_override(false);
    }

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{} {}", "error:".red().bold(), msg);
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    if args.list_languages {
        list_languages();
        return Ok(());
    }

    let root = args
        .path
        .canonicalize()
        .map_err(|e| format!("cannot read {}: {e}", args.path.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let classifier = Classifier::new(&root)
        .vendor_dirs(&args.vendor_dir)
        .build_dirs(&args.build_dir)
        .generated_dirs(&args.generated_dir)
        .source_dirs(&args.source_dir)
        .exclude(build_globs(&args.exclude)?)
        .include(build_globs(&args.include)?)
        .detect_markers(!args.no_marker_detection)
        .merge_tests_into_source(args.tests_as_source);

    if let Some(target) = &args.explain {
        return explain(&root, target, &classifier);
    }

    if args.threads > 0 {
        // A failure here just means a pool already exists; the defaults are fine.
        let _ = rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global();
    }

    let opts = ScanOptions {
        hidden: args.hidden,
        respect_ignore: !args.no_ignore,
        follow_links: args.follow_links,
        fast: args.fast,
        max_file_bytes: args.max_file_size.saturating_mul(1024 * 1024),
        detect_markers: !args.no_marker_detection,
    };

    let scanned = scan::scan(&root, &classifier, &opts);
    if scanned.files.is_empty() {
        return Err(format!(
            "no files matched under {} (check --include / --exclude, or pass --no-ignore)",
            root.display()
        ));
    }

    let agg = Aggregates::build(&scanned, args);
    let git_stats = if args.git { git::GitStats::collect(&root) } else { None };

    report::render(&scanned, &agg, args, git_stats.as_ref());
    Ok(())
}

fn build_globs(patterns: &[String]) -> Result<Option<globset::GlobSet>, String> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).map_err(|e| format!("bad glob {p:?}: {e}"))?;
        b.add(glob);
        // `src/**` is the obvious way to say "everything under src", so accept
        // the bare directory form too rather than silently matching nothing.
        if !p.contains('*') && !p.ends_with('/') {
            if let Ok(g) = Glob::new(&format!("{}/**", p.trim_end_matches('/'))) {
                b.add(g);
            }
        }
    }
    b.build().map(Some).map_err(|e| format!("cannot build glob set: {e}"))
}

fn explain(root: &Path, target: &Path, classifier: &Classifier) -> Result<(), String> {
    // Accept either an absolute path or one relative to the scan root.
    let rel: PathBuf = if target.is_absolute() {
        target
            .strip_prefix(root)
            .map_err(|_| format!("{} is not under {}", target.display(), root.display()))?
            .to_path_buf()
    } else {
        target.to_path_buf()
    };

    let full = root.join(&rel);
    let head = std::fs::read(&full).ok().and_then(|b| {
        if count::is_binary(&b) {
            None
        } else {
            Some(String::from_utf8_lossy(&b[..b.len().min(512)]).into_owned())
        }
    });

    let language = lang::detect(&rel);
    let result = classifier.classify(&rel, head.as_deref());

    println!("\n{} {}", "path:".bold(), rel.display().to_string().cyan());
    println!("{}     {}", "exists:".bold(), if full.exists() { "yes" } else { "no (classified from the path alone)" });
    println!(
        "{}   {}",
        "language:".bold(),
        language.map(|l| l.name).unwrap_or("unrecognised")
    );
    println!(
        "{}       {}",
        "role:".bold(),
        if result.role.is_project_code() {
            result.role.label().green().bold()
        } else {
            result.role.label().yellow().bold()
        }
    );
    println!("{}     {}", "reason:".bold(), result.reason);
    println!(
        "{}    {}\n",
        "counted:".bold(),
        if result.role.is_project_code() {
            "yes — included in the project-code total"
        } else {
            "no — reported separately from project code"
        }
    );
    Ok(())
}

fn list_languages() {
    let mut names: Vec<(&str, Vec<&str>)> = lang::LANGUAGES
        .iter()
        .map(|l| {
            let mut e: Vec<&str> = l.exts.to_vec();
            e.extend(l.filenames.iter().copied());
            (l.name, e)
        })
        .collect();
    names.sort_by_key(|(n, _)| *n);
    println!("{} languages recognised by devstat {}\n", names.len(), env!("CARGO_PKG_VERSION"));
    for (name, exts) in names {
        println!("  {:<20} {}", name, exts.join(", "));
    }
}
