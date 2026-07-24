//! Aggregation and rendering.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use colored::*;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table};

use crate::classify::Role;
use crate::cli::{Args, Format, Sort, View};
use crate::count::Counts;
use crate::scan::Scan;

/// One aggregated row: a language, a role, or a directory.
#[derive(Debug, Clone, Default)]
pub struct Bucket {
    pub name: String,
    pub files: u64,
    pub counts: Counts,
    pub size: u64,
}

pub struct Aggregates {
    pub by_language: Vec<Bucket>,
    pub by_role: Vec<(Role, Bucket)>,
    pub by_dir: Vec<Bucket>,
    pub largest: Vec<(PathBuf, u64, Role)>,
    /// Totals for the roles that count as project code.
    pub project: Bucket,
    /// Everything scanned, whatever its role.
    pub scanned: Bucket,
    pub vendor_code: u64,
    pub generated_code: u64,
    pub build_bytes: u64,
    pub build_files: u64,
    pub project_roles: Vec<Role>,
}

impl Aggregates {
    pub fn build(scan: &Scan, args: &Args) -> Aggregates {
        let mut project_roles = vec![Role::Source, Role::Test];
        if args.include_vendor {
            project_roles.push(Role::Vendor);
        }
        if args.include_generated {
            project_roles.push(Role::Generated);
        }
        let counts_for_project = |r: Role| project_roles.contains(&r);

        let mut langs: HashMap<&'static str, Bucket> = HashMap::new();
        let mut roles: HashMap<Role, Bucket> = HashMap::new();
        let mut dirs: HashMap<String, Bucket> = HashMap::new();
        let mut project = Bucket { name: "project".into(), ..Default::default() };
        let mut scanned = Bucket { name: "all".into(), ..Default::default() };
        let mut vendor_code = 0u64;
        let mut generated_code = 0u64;
        let mut build_bytes = 0u64;
        let mut build_files = 0u64;

        for f in &scan.files {
            scanned.files += 1;
            scanned.size += f.size;
            scanned.counts.add(&f.counts);

            let role_bucket = roles.entry(f.role).or_insert_with(|| Bucket { name: f.role.label().into(), ..Default::default() });
            role_bucket.files += 1;
            role_bucket.size += f.size;
            role_bucket.counts.add(&f.counts);

            match f.role {
                Role::Vendor => vendor_code += f.counts.code,
                Role::Generated => generated_code += f.counts.code,
                Role::Build => {
                    build_bytes += f.size;
                    build_files += 1;
                }
                _ => {}
            }

            if !counts_for_project(f.role) {
                continue;
            }

            project.files += 1;
            project.size += f.size;
            project.counts.add(&f.counts);

            if let Some(l) = f.lang {
                let b = langs.entry(l.name).or_insert_with(|| Bucket { name: l.name.into(), ..Default::default() });
                b.files += 1;
                b.size += f.size;
                b.counts.add(&f.counts);
            }

            let key = group_key(&f.path, args.depth);
            let b = dirs.entry(key.clone()).or_insert_with(|| Bucket { name: key, ..Default::default() });
            b.files += 1;
            b.size += f.size;
            b.counts.add(&f.counts);
        }

        let mut largest: Vec<(PathBuf, u64, Role)> = Vec::new();
        if args.largest_files > 0 {
            largest = scan.files.iter().map(|f| (f.path.clone(), f.size, f.role)).collect();
            largest.sort_by(|a, b| b.1.cmp(&a.1));
            largest.truncate(args.largest_files);
        }

        let mut by_role: Vec<(Role, Bucket)> = roles.into_iter().collect();
        by_role.sort_by_key(|(r, _)| *r);

        let mut agg = Aggregates {
            by_language: langs.into_values().collect(),
            by_role,
            by_dir: dirs.into_values().collect(),
            largest,
            project,
            scanned,
            vendor_code,
            generated_code,
            build_bytes,
            build_files,
            project_roles,
        };
        sort_buckets(&mut agg.by_language, args.sort);
        sort_buckets(&mut agg.by_dir, args.sort);
        agg
    }
}

/// Collapse a file path to its grouping directory at `depth` components.
/// Files sitting directly in the scan root group under `.`.
fn group_key(rel: &Path, depth: usize) -> String {
    let parts: Vec<String> = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    Component::Normal(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if parts.is_empty() {
        return ".".to_string();
    }
    let take = depth.max(1).min(parts.len());
    parts[..take].join("/")
}

fn sort_buckets(v: &mut [Bucket], sort: Sort) {
    match sort {
        Sort::Code => v.sort_by(|a, b| b.counts.code.cmp(&a.counts.code).then_with(|| a.name.cmp(&b.name))),
        Sort::Files => v.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.name.cmp(&b.name))),
        Sort::Size => v.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name))),
        Sort::Name => v.sort_by(|a, b| a.name.cmp(&b.name)),
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn pct(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "—".into();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

fn role_color(role: Role) -> Color {
    match role {
        Role::Source => Color::Green,
        Role::Test => Color::Cyan,
        Role::Vendor => Color::Yellow,
        Role::Generated => Color::Magenta,
        Role::Build => Color::Red,
        Role::Docs => Color::Blue,
        Role::Config => Color::DarkGrey,
        Role::Asset => Color::DarkGrey,
        Role::Other => Color::DarkGrey,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render(scan: &Scan, agg: &Aggregates, args: &Args, git: Option<&crate::git::GitStats>) {
    match args.format {
        Format::Json => render_json(scan, agg, args, git),
        Format::Csv => render_csv(scan, agg, args),
        Format::Table | Format::Markdown => render_tables(scan, agg, args, git),
    }
}

fn new_table(markdown: bool) -> Table {
    let mut t = Table::new();
    if markdown {
        t.load_preset(comfy_table::presets::ASCII_MARKDOWN);
    } else {
        t.load_preset(UTF8_FULL).set_content_arrangement(ContentArrangement::Dynamic);
    }
    t
}

fn right_align(table: &mut Table, cols: &[usize]) {
    for &c in cols {
        if let Some(col) = table.column_mut(c) {
            col.set_cell_alignment(CellAlignment::Right);
        }
    }
}

fn header(md: bool, title: &str) {
    if md {
        println!("\n## {title}\n");
    } else {
        println!("\n{}", title.bold());
    }
}

fn render_tables(scan: &Scan, agg: &Aggregates, args: &Args, git: Option<&crate::git::GitStats>) {
    let md = args.format == Format::Markdown;

    if !args.quiet {
        if md {
            println!("# devstat — {}", scan.root.display());
        } else {
            println!(
                "\n{} {}",
                "devstat".bold(),
                scan.root.display().to_string().cyan()
            );
        }
    }

    if args.quiet {
        print_summary(agg, md);
        return;
    }

    for view in args.views() {
        match view {
            View::Languages => {
                header(md, "Project code by language");
                let mut t = new_table(md);
                t.set_header(vec![
                    Cell::new("Language"),
                    Cell::new("Files"),
                    Cell::new("Code"),
                    Cell::new("Comments"),
                    Cell::new("Blank"),
                    Cell::new("Share"),
                ]);
                let rows = visible(&agg.by_language, args);
                for b in &rows {
                    t.add_row(vec![
                        Cell::new(&b.name),
                        Cell::new(group_digits(b.files)),
                        Cell::new(group_digits(b.counts.code)),
                        Cell::new(group_digits(b.counts.comments)),
                        Cell::new(group_digits(b.counts.blank)),
                        Cell::new(pct(b.counts.code, agg.project.counts.code)),
                    ]);
                }
                add_total_row(&mut t, "TOTAL", agg.project.files, &agg.project.counts, md);
                right_align(&mut t, &[1, 2, 3, 4, 5]);
                println!("{t}");
                note_truncation(agg.by_language.len(), rows.len(), md);
            }
            View::Roles => {
                header(md, "Classification");
                let mut t = new_table(md);
                t.set_header(vec![
                    Cell::new("Role"),
                    Cell::new("Files"),
                    Cell::new("Code"),
                    Cell::new("Comments"),
                    Cell::new("On disk"),
                    Cell::new("Counted as project"),
                ]);
                for (role, b) in &agg.by_role {
                    let counted = agg.project_roles.contains(role);
                    let code = if *role == Role::Build {
                        "—".to_string()
                    } else {
                        group_digits(b.counts.code)
                    };
                    let comments = if *role == Role::Build {
                        "—".to_string()
                    } else {
                        group_digits(b.counts.comments)
                    };
                    t.add_row(vec![
                        if md { Cell::new(role.label()) } else { Cell::new(role.label()).fg(role_color(*role)) },
                        Cell::new(group_digits(b.files)),
                        Cell::new(code),
                        Cell::new(comments),
                        Cell::new(format_bytes(b.size)),
                        Cell::new(if counted { "yes" } else { "no" }),
                    ]);
                }
                right_align(&mut t, &[1, 2, 3, 4]);
                println!("{t}");
            }
            View::Dirs => {
                header(md, &format!("Project code by directory (depth {})", args.depth.max(1)));
                let mut t = new_table(md);
                t.set_header(vec![
                    Cell::new("Directory"),
                    Cell::new("Files"),
                    Cell::new("Code"),
                    Cell::new("Comments"),
                    Cell::new("On disk"),
                    Cell::new("Share"),
                ]);
                let rows = visible(&agg.by_dir, args);
                for b in &rows {
                    t.add_row(vec![
                        Cell::new(&b.name),
                        Cell::new(group_digits(b.files)),
                        Cell::new(group_digits(b.counts.code)),
                        Cell::new(group_digits(b.counts.comments)),
                        Cell::new(format_bytes(b.size)),
                        Cell::new(pct(b.counts.code, agg.project.counts.code)),
                    ]);
                }
                right_align(&mut t, &[1, 2, 3, 4, 5]);
                println!("{t}");
                note_truncation(agg.by_dir.len(), rows.len(), md);
            }
            View::Files => {
                header(md, "Files");
                let mut t = new_table(md);
                t.set_header(vec![
                    Cell::new("Path"),
                    Cell::new("Role"),
                    Cell::new("Language"),
                    Cell::new("Code"),
                    Cell::new("Size"),
                ]);
                let mut files: Vec<_> = scan.files.iter().filter(|f| f.counts.code >= args.min_code).collect();
                match args.sort {
                    Sort::Size => files.sort_by(|a, b| b.size.cmp(&a.size)),
                    Sort::Name => files.sort_by(|a, b| a.path.cmp(&b.path)),
                    _ => files.sort_by(|a, b| b.counts.code.cmp(&a.counts.code)),
                }
                let shown = files.len().min(args.top);
                for f in files.iter().take(shown) {
                    t.add_row(vec![
                        Cell::new(f.path.display().to_string()),
                        Cell::new(f.role.label()),
                        Cell::new(f.lang.map(|l| l.name).unwrap_or("—")),
                        Cell::new(group_digits(f.counts.code)),
                        Cell::new(format_bytes(f.size)),
                    ]);
                }
                right_align(&mut t, &[3, 4]);
                println!("{t}");
                note_truncation(files.len(), shown, md);
            }
        }
    }

    if !agg.largest.is_empty() {
        header(md, "Largest files on disk");
        let mut t = new_table(md);
        t.set_header(vec![Cell::new("Path"), Cell::new("Size"), Cell::new("Role")]);
        for (p, size, role) in &agg.largest {
            t.add_row(vec![
                Cell::new(p.display().to_string()),
                Cell::new(format_bytes(*size)),
                Cell::new(role.label()),
            ]);
        }
        right_align(&mut t, &[1]);
        println!("{t}");
    }

    if let Some(g) = git {
        header(md, "Repository");
        let mut t = new_table(md);
        t.set_header(vec![Cell::new("Metric"), Cell::new("Value")]);
        for (k, v) in g.rows() {
            t.add_row(vec![Cell::new(k), Cell::new(v)]);
        }
        println!("{t}");
    }

    if args.verbose && !scan.skipped.is_empty() {
        header(md, &format!("Not counted ({})", scan.skipped.len()));
        for (p, why) in scan.skipped.iter().take(args.top) {
            println!("  {} — {}", p.display(), why);
        }
        if scan.skipped.len() > args.top {
            println!("  … {} more", scan.skipped.len() - args.top);
        }
    }

    print_summary(agg, md);
}

fn visible(buckets: &[Bucket], args: &Args) -> Vec<Bucket> {
    buckets
        .iter()
        .filter(|b| b.counts.code >= args.min_code)
        .take(args.top)
        .cloned()
        .collect()
}

fn note_truncation(total: usize, shown: usize, md: bool) {
    if total > shown {
        let msg = format!("… {} more rows (raise --top or lower --min-code)", total - shown);
        if md {
            println!("\n_{msg}_");
        } else {
            println!("{}", msg.dimmed());
        }
    }
}

fn add_total_row(t: &mut Table, label: &str, files: u64, counts: &Counts, md: bool) {
    let cell = |s: String| if md { Cell::new(s) } else { Cell::new(s).add_attribute(comfy_table::Attribute::Bold) };
    t.add_row(vec![
        cell(label.to_string()),
        cell(group_digits(files)),
        cell(group_digits(counts.code)),
        cell(group_digits(counts.comments)),
        cell(group_digits(counts.blank)),
        cell("100%".to_string()),
    ]);
}

fn print_summary(agg: &Aggregates, md: bool) {
    let p = &agg.project;
    let roles: Vec<&str> = agg.project_roles.iter().map(|r| r.label()).collect();

    if md {
        println!("\n## Summary\n");
        println!("- **Project code:** {} lines across {} files ({})", group_digits(p.counts.code), group_digits(p.files), roles.join(" + "));
        println!("- **Comments:** {} ({} of project lines)", group_digits(p.counts.comments), pct(p.counts.comments, p.counts.code + p.counts.comments));
        println!("- **Vendored code:** {} lines", group_digits(agg.vendor_code));
        println!("- **Generated code:** {} lines", group_digits(agg.generated_code));
        println!("- **Build output:** {} across {} files", format_bytes(agg.build_bytes), group_digits(agg.build_files));
        println!("- **Total scanned:** {} files, {}", group_digits(agg.scanned.files), format_bytes(agg.scanned.size));
        return;
    }

    // Columns are padded before colouring: ANSI escapes would otherwise be
    // counted towards the field width and the summary would come out ragged.
    let value_width = [
        group_digits(p.counts.code).len(),
        group_digits(agg.vendor_code).len(),
        format_bytes(agg.build_bytes).len(),
        group_digits(agg.scanned.files).len(),
    ]
    .into_iter()
    .max()
    .unwrap_or(8);

    let row = |label: &str, value: String, paint: fn(String) -> ColoredString, note: String| {
        println!(
            "  {}  {}  {}",
            format!("{label:<14}").bold(),
            paint(format!("{value:>value_width$}")),
            note.dimmed()
        );
    };

    println!();
    row(
        "Project code",
        group_digits(p.counts.code),
        |s| s.green().bold(),
        format!("lines across {} files ({})", group_digits(p.files), roles.join(" + ")),
    );
    row(
        "Comments",
        group_digits(p.counts.comments),
        |s| s.blue(),
        format!("lines ({} of all project lines)", pct(p.counts.comments, p.counts.code + p.counts.comments)),
    );
    let folded = |r: Role| {
        if agg.project_roles.contains(&r) {
            "lines — folded into the project total above"
        } else {
            "lines — reported separately, not yours"
        }
        .to_string()
    };
    if agg.vendor_code > 0 {
        row("Vendored", group_digits(agg.vendor_code), |s| s.yellow(), folded(Role::Vendor));
    }
    if agg.generated_code > 0 {
        row("Generated", group_digits(agg.generated_code), |s| s.magenta(), folded(Role::Generated));
    }
    if agg.build_bytes > 0 {
        row(
            "Build output",
            format_bytes(agg.build_bytes),
            |s| s.red(),
            format!("across {} files — reclaimable, no authorship", group_digits(agg.build_files)),
        );
    }
    row(
        "Total scanned",
        group_digits(agg.scanned.files),
        |s| s.normal(),
        format!("files, {} on disk", format_bytes(agg.scanned.size)),
    );
    println!();
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn bucket_json(b: &Bucket) -> String {
    format!(
        r#"{{"name":"{}","files":{},"code":{},"comments":{},"blank":{},"bytes":{}}}"#,
        esc(&b.name),
        b.files,
        b.counts.code,
        b.counts.comments,
        b.counts.blank,
        b.size
    )
}

fn render_json(scan: &Scan, agg: &Aggregates, args: &Args, git: Option<&crate::git::GitStats>) {
    let langs: Vec<String> = agg.by_language.iter().map(bucket_json).collect();
    let dirs: Vec<String> = agg.by_dir.iter().map(bucket_json).collect();
    let roles: Vec<String> = agg
        .by_role
        .iter()
        .map(|(r, b)| {
            format!(
                r#"{{"role":"{}","counted_as_project":{},"files":{},"code":{},"comments":{},"blank":{},"bytes":{}}}"#,
                r.key(),
                agg.project_roles.contains(r),
                b.files,
                b.counts.code,
                b.counts.comments,
                b.counts.blank,
                b.size
            )
        })
        .collect();

    println!("{{");
    println!(r#"  "tool": "devstat", "version": "{}","#, env!("CARGO_PKG_VERSION"));
    println!(r#"  "root": "{}","#, esc(&scan.root.display().to_string()));
    println!(
        r#"  "summary": {{"project_code":{},"project_comments":{},"project_blank":{},"project_files":{},"vendor_code":{},"generated_code":{},"build_bytes":{},"build_files":{},"scanned_files":{},"scanned_bytes":{},"submodules":{},"skipped":{},"walk_errors":{}}},"#,
        agg.project.counts.code,
        agg.project.counts.comments,
        agg.project.counts.blank,
        agg.project.files,
        agg.vendor_code,
        agg.generated_code,
        agg.build_bytes,
        agg.build_files,
        agg.scanned.files,
        agg.scanned.size,
        scan.submodules,
        scan.skipped.len(),
        scan.walk_errors
    );
    println!(r#"  "roles": [{}],"#, roles.join(","));
    println!(r#"  "languages": [{}],"#, langs.join(","));
    print!(r#"  "directories": [{}]"#, dirs.join(","));

    if args.views().contains(&View::Files) {
        let files: Vec<String> = scan
            .files
            .iter()
            .filter(|f| f.counts.code >= args.min_code)
            .map(|f| {
                format!(
                    r#"{{"path":"{}","role":"{}","reason":"{}","language":{},"code":{},"comments":{},"blank":{},"bytes":{}}}"#,
                    esc(&f.path.display().to_string()),
                    f.role.key(),
                    esc(f.reason),
                    f.lang.map(|l| format!("\"{}\"", esc(l.name))).unwrap_or_else(|| "null".into()),
                    f.counts.code,
                    f.counts.comments,
                    f.counts.blank,
                    f.size
                )
            })
            .collect();
        println!(",");
        print!(r#"  "files": [{}]"#, files.join(","));
    }

    if let Some(g) = git {
        println!(",");
        print!("  \"git\": {}", g.to_json());
    }
    println!("\n}}");
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn render_csv(scan: &Scan, agg: &Aggregates, args: &Args) {
    println!("section,name,role,files,code,comments,blank,bytes");
    for (role, b) in &agg.by_role {
        println!(
            "role,{},{},{},{},{},{},{}",
            csv_field(role.label()),
            role.key(),
            b.files,
            b.counts.code,
            b.counts.comments,
            b.counts.blank,
            b.size
        );
    }
    for b in agg.by_language.iter().filter(|b| b.counts.code >= args.min_code) {
        println!("language,{},,{},{},{},{},{}", csv_field(&b.name), b.files, b.counts.code, b.counts.comments, b.counts.blank, b.size);
    }
    for b in agg.by_dir.iter().filter(|b| b.counts.code >= args.min_code) {
        println!("directory,{},,{},{},{},{},{}", csv_field(&b.name), b.files, b.counts.code, b.counts.comments, b.counts.blank, b.size);
    }
    if args.views().contains(&View::Files) {
        for f in scan.files.iter().filter(|f| f.counts.code >= args.min_code) {
            println!(
                "file,{},{},1,{},{},{},{}",
                csv_field(&f.path.display().to_string()),
                f.role.key(),
                f.counts.code,
                f.counts.comments,
                f.counts.blank,
                f.size
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_digits() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1,000");
        assert_eq!(group_digits(2_772_431), "2,772,431");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(6 * 1024 * 1024 * 1024), "6.00 GB");
    }

    #[test]
    fn groups_paths_by_depth() {
        assert_eq!(group_key(Path::new("main.rs"), 1), ".");
        assert_eq!(group_key(Path::new("src/main.rs"), 1), "src");
        assert_eq!(group_key(Path::new("src/engine/gfx/vk.cpp"), 2), "src/engine");
        // A shallower path is not padded out to the requested depth.
        assert_eq!(group_key(Path::new("src/main.rs"), 3), "src");
    }

    #[test]
    fn escapes_json() {
        assert_eq!(esc(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(esc("line\n"), "line\\n");
    }

    #[test]
    fn escapes_csv() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
