use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};

const LONG_ABOUT: &str = "\
devstat walks a project and reports how much of it the team actually wrote.

Every file is classified by where it sits in the tree, not by how big it is or
what extension it has. Vendored dependencies (third_party/, external/, git
submodules, nested repositories), generated sources (protobuf output, moc_*,
*.g.dart) and build output (build*/, target/, out/) are counted separately from
project source, so the headline number means something.

EXAMPLES:
    devstat                                 report on the current directory
    devstat ~/code/engine --top 20          widen the per-directory table
    devstat --format json > stats.json      machine-readable output
    devstat --explain third_party/bgfx/x.c  show why a file was classified
    devstat --vendor-dir libs --vendor-dir sdk
    devstat --git --largest-files 10";

#[derive(Parser, Debug)]
#[command(
    name = "devstat",
    version,
    about = "Codebase statistics that tell project code apart from vendored, generated and build output",
    long_about = LONG_ABOUT,
    max_term_width = 100
)]
pub struct Args {
    /// Directory to analyse
    #[arg(default_value = ".", value_name = "PATH")]
    pub path: PathBuf,

    // ---- output ---------------------------------------------------------
    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Table, value_name = "FORMAT")]
    pub format: Format,

    /// Sections to print; repeat to combine [default: languages, roles, dirs]
    #[arg(long = "by", value_enum, value_name = "VIEW", action = ArgAction::Append)]
    pub by: Vec<View>,

    /// Maximum rows per table
    #[arg(long, default_value_t = 15, value_name = "N")]
    pub top: usize,

    /// Row ordering
    #[arg(long, value_enum, default_value_t = Sort::Code, value_name = "KEY")]
    pub sort: Sort,

    /// Directory nesting depth used to group the per-directory table
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub depth: usize,

    /// Also list the N largest files on disk
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub largest_files: usize,

    /// Hide rows with fewer than N lines of code
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub min_code: u64,

    /// Print only the summary line
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Report files that could not be read or decoded
    #[arg(short, long)]
    pub verbose: bool,

    /// Disable ANSI colour (also honours NO_COLOR)
    #[arg(long)]
    pub no_color: bool,

    // ---- what counts ----------------------------------------------------
    /// Fold tests into the source figure instead of reporting them separately
    #[arg(long)]
    pub tests_as_source: bool,

    /// Include vendored code in the project-code total
    #[arg(long)]
    pub include_vendor: bool,

    /// Include generated code in the project-code total
    #[arg(long)]
    pub include_generated: bool,

    /// Do not read file headers looking for "@generated" banners
    #[arg(long)]
    pub no_marker_detection: bool,

    /// Measure vendor and generated trees by size only; do not count their lines
    #[arg(long)]
    pub fast: bool,

    /// Skip files larger than this many megabytes when counting lines
    #[arg(long, default_value_t = 16, value_name = "MB")]
    pub max_file_size: u64,

    // ---- classification overrides ---------------------------------------
    /// Treat this directory name as third-party (repeatable)
    #[arg(long = "vendor-dir", value_name = "NAME", action = ArgAction::Append)]
    pub vendor_dir: Vec<String>,

    /// Treat this directory name as build output (repeatable)
    #[arg(long = "build-dir", value_name = "NAME", action = ArgAction::Append)]
    pub build_dir: Vec<String>,

    /// Treat this directory name as generated (repeatable)
    #[arg(long = "generated-dir", value_name = "NAME", action = ArgAction::Append)]
    pub generated_dir: Vec<String>,

    /// Force this directory name to count as project source, overriding every
    /// other rule (repeatable)
    #[arg(long = "source-dir", value_name = "NAME", action = ArgAction::Append)]
    pub source_dir: Vec<String>,

    // ---- traversal ------------------------------------------------------
    /// Only analyse paths matching this glob (repeatable)
    #[arg(long, value_name = "GLOB", action = ArgAction::Append)]
    pub include: Vec<String>,

    /// Skip paths matching this glob (repeatable)
    #[arg(long, value_name = "GLOB", action = ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Include hidden files and directories
    #[arg(long)]
    pub hidden: bool,

    /// Do not honour .gitignore / .ignore files
    #[arg(long)]
    pub no_ignore: bool,

    /// Follow symbolic links
    #[arg(long)]
    pub follow_links: bool,

    /// Worker threads (0 = one per core)
    #[arg(short = 'j', long, default_value_t = 0, value_name = "N")]
    pub threads: usize,

    // ---- one-shot modes -------------------------------------------------
    /// Explain how a single path is classified, then exit
    #[arg(long, value_name = "PATH")]
    pub explain: Option<PathBuf>,

    /// List every recognised language and exit
    #[arg(long)]
    pub list_languages: bool,

    /// Append repository statistics from git history
    #[arg(long)]
    pub git: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Boxed tables for a terminal
    Table,
    /// Machine-readable JSON
    Json,
    /// Comma-separated values
    Csv,
    /// GitHub-flavoured markdown tables
    Markdown,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum View {
    /// Breakdown by programming language
    Languages,
    /// Breakdown by classification role
    Roles,
    /// Breakdown by top-level directory
    Dirs,
    /// One row per file
    Files,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Sort {
    Code,
    Files,
    Size,
    Name,
}

impl Args {
    /// Sections to print, applying the default set when `--by` was not given.
    pub fn views(&self) -> Vec<View> {
        if self.by.is_empty() {
            vec![View::Languages, View::Roles, View::Dirs]
        } else {
            let mut v = self.by.clone();
            v.dedup();
            v
        }
    }
}
