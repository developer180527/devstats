//! Role classification.
//!
//! This is the part of devstat that decides whether a line of code belongs to
//! the project or to somebody else. The rule is: **position in the tree wins
//! over anything about the file itself.** A 12,000-line header is project code
//! if it sits in `src/` and vendor code if it sits in `third_party/`. Earlier
//! versions guessed from file size, which is why bundled engines like bgfx or
//! Jolt were reported as work the developer had done.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use globset::GlobSet;

use crate::lang::{self, FileKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// Code the project team wrote and maintains.
    Source,
    /// Project-owned tests, benchmarks and fixtures.
    Test,
    /// Third-party code that lives in the tree but is maintained elsewhere.
    Vendor,
    /// Machine-emitted source that a tool will rewrite on the next build.
    Generated,
    /// Compiler and packager output; carries disk cost but no authorship.
    Build,
    /// Prose.
    Docs,
    /// Declarative project configuration.
    Config,
    /// Binaries, images, models, fonts.
    Asset,
    /// Recognised file that fits nowhere above.
    Other,
}

impl Role {
    /// Roles whose lines are attributed to the project team. This is the set
    /// the headline "project code" figure is built from.
    pub fn is_project_code(self) -> bool {
        matches!(self, Role::Source | Role::Test)
    }

    /// Roles worth counting lines for at all. Build output is measured by size
    /// only — reading it would be slow and the number would be meaningless.
    pub fn worth_counting(self) -> bool {
        !matches!(self, Role::Build | Role::Asset)
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Source => "Source",
            Role::Test => "Tests",
            Role::Vendor => "Vendor",
            Role::Generated => "Generated",
            Role::Build => "Build output",
            Role::Docs => "Docs",
            Role::Config => "Config",
            Role::Asset => "Assets",
            Role::Other => "Other",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Role::Source => "source",
            Role::Test => "test",
            Role::Vendor => "vendor",
            Role::Generated => "generated",
            Role::Build => "build",
            Role::Docs => "docs",
            Role::Config => "config",
            Role::Asset => "asset",
            Role::Other => "other",
        }
    }

}

/// Directories holding compiler, packager or cache output.
const BUILD_DIRS: &[&str] = &[
    "out", "dist", "target", "_build", ".build", "obj", "objs", "bin-int", "intermediate",
    "derivedddata", "deriveddata", "cmakefiles", ".cmake", "testing", "coverage", "htmlcov",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".tox", ".nox", ".gradle",
    ".next", ".nuxt", ".svelte-kit", ".turbo", ".parcel-cache", ".angular", ".dart_tool",
    ".ccls-cache", ".clangd", ".cache", "cmake-build", "xcuserdata", "xcshareddata",
];

/// Directory-name prefixes that mark build output, so `build`, `build-asan`,
/// `build-ship`, `cmake-build-debug` and `build_x64` are all caught.
const BUILD_PREFIXES: &[&str] = &["build", "cmake-build", "out-", "dist-"];

/// Directories holding code maintained by somebody else.
const VENDOR_DIRS: &[&str] = &[
    "third_party", "third-party", "thirdparty", "3rdparty", "3rd_party", "3rd-party",
    "vendor", "vendors", "vendored", "external", "externals", "extern", "deps",
    "dependencies", "node_modules", "bower_components", "jspm_packages", "pods",
    "carthage", "packages", "site-packages", "vcpkg", "vcpkg_installed", "conan",
    "submodules", "subprojects", "contrib", "imported", "prebuilt", "sdk",
];

/// Directories holding machine-emitted source.
const GENERATED_DIRS: &[&str] = &[
    "generated", "__generated__", ".generated", "autogen", "auto_generated", "gen",
    "gen-cpp", "gen-py", "codegen", "protogen", "moc", "uic", "rcc",
];

/// Directories holding the project's own tests.
const TEST_DIRS: &[&str] = &[
    "test", "tests", "testing", "unittest", "unittests", "spec", "specs", "__tests__",
    "benchmark", "benchmarks", "bench", "fixtures", "testdata", "e2e", "integration_tests",
];

/// Directories holding prose.
const DOC_DIRS: &[&str] = &["doc", "docs", "documentation", "manual", "man", "wiki"];

/// Directories holding non-code payload.
const ASSET_DIRS: &[&str] = &["assets", "asset", "resources", "res", "media", "textures", "models", "fonts", "audio", "sounds", "shaders_compiled"];

/// Filename suffixes that only a code generator produces.
const GENERATED_SUFFIXES: &[&str] = &[
    ".pb.go", ".pb.cc", ".pb.h", ".pb.rs", "_pb2.py", "_pb2_grpc.py", "_pb.js",
    ".g.dart", ".freezed.dart", ".g.cs", ".designer.cs", ".generated.h", ".generated.cpp",
    ".gen.go", ".gen.rs", "_generated.h", "_generated.cpp", "_generated.go", "_generated.rs",
    ".min.js", ".min.css", ".bundle.js", ".d.ts.map", ".js.map", ".css.map",
    "-lock.json", ".lock", "_wrap.cxx", "_wrap.c",
];

/// Filename prefixes emitted by Qt's meta-object toolchain and similar.
const GENERATED_PREFIXES: &[&str] = &["moc_", "ui_", "qrc_", "sip", "swig_"];

/// Markers a generator writes into the head of a file it owns.
const GENERATED_MARKERS: &[&str] = &[
    "@generated",
    "do not edit",
    "code generated by",
    "automatically generated",
    "auto-generated",
    "autogenerated",
    "generated by the protocol buffer compiler",
    "this file is generated",
];

/// Filename patterns that mark a project's own tests. Operates on the name
/// with its final extension removed, so `app.spec.ts` and `parser_test.go`
/// are both recognised.
fn looks_like_test_file(file_name: &str) -> bool {
    let base = file_name.rsplit_once('.').map_or(file_name, |(b, _)| b);
    let lower = base.to_ascii_lowercase();
    lower.starts_with("test_")
        || lower.starts_with("test-")
        || lower.ends_with("_test")
        || lower.ends_with("-test")
        || lower.ends_with("_tests")
        || lower.ends_with("-tests")
        || lower.ends_with(".test")
        || lower.ends_with(".spec")
        || lower.ends_with("_spec")
        // CamelCase conventions: FooTest.java, FooTests.cs, FooSpec.scala.
        || base.ends_with("Test")
        || base.ends_with("Tests")
        || base.ends_with("Spec")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub role: Role,
    /// Why this role was chosen — surfaced by `--explain` so a surprising
    /// number can always be traced back to a rule.
    pub reason: &'static str,
}

/// User-supplied overrides and the derived state needed to classify a path.
pub struct Classifier {
    root: PathBuf,
    vendor_dirs: Vec<String>,
    build_dirs: Vec<String>,
    source_dirs: Vec<String>,
    generated_dirs: Vec<String>,
    exclude: Option<GlobSet>,
    include: Option<GlobSet>,
    /// Paths declared as git submodules in the root `.gitmodules`.
    submodules: Vec<PathBuf>,
    /// Memoised "is this directory the root of a nested git repository?".
    nested_repos: Mutex<HashMap<PathBuf, bool>>,
    /// Whether to sniff file heads for generator markers.
    detect_markers: bool,
    treat_tests_as_source: bool,
}

impl Classifier {
    pub fn new(root: &Path) -> Self {
        Classifier {
            root: root.to_path_buf(),
            vendor_dirs: Vec::new(),
            build_dirs: Vec::new(),
            source_dirs: Vec::new(),
            generated_dirs: Vec::new(),
            exclude: None,
            include: None,
            submodules: read_submodules(root),
            nested_repos: Mutex::new(HashMap::new()),
            detect_markers: true,
            treat_tests_as_source: false,
        }
    }

    pub fn vendor_dirs(mut self, v: &[String]) -> Self {
        self.vendor_dirs = lowercased(v);
        self
    }
    pub fn build_dirs(mut self, v: &[String]) -> Self {
        self.build_dirs = lowercased(v);
        self
    }
    pub fn source_dirs(mut self, v: &[String]) -> Self {
        self.source_dirs = lowercased(v);
        self
    }
    pub fn generated_dirs(mut self, v: &[String]) -> Self {
        self.generated_dirs = lowercased(v);
        self
    }
    pub fn exclude(mut self, g: Option<GlobSet>) -> Self {
        self.exclude = g;
        self
    }
    pub fn include(mut self, g: Option<GlobSet>) -> Self {
        self.include = g;
        self
    }
    pub fn detect_markers(mut self, on: bool) -> Self {
        self.detect_markers = on;
        self
    }
    pub fn merge_tests_into_source(mut self, on: bool) -> Self {
        self.treat_tests_as_source = on;
        self
    }

    pub fn submodule_count(&self) -> usize {
        self.submodules.len()
    }

    /// True when the path should not appear in the report at all.
    pub fn is_filtered_out(&self, rel: &Path) -> bool {
        if let Some(ex) = &self.exclude {
            if ex.is_match(rel) {
                return true;
            }
        }
        if let Some(inc) = &self.include {
            if !inc.is_match(rel) {
                return true;
            }
        }
        false
    }

    /// Classify `rel` (a path relative to the scan root).
    ///
    /// `head` is the first few hundred bytes of the file when available; it is
    /// only consulted to spot generator banners, and only after every
    /// directory-level rule has failed to place the file.
    pub fn classify(&self, rel: &Path, head: Option<&str>) -> Classification {
        let dirs: Vec<String> = rel
            .parent()
            .map(|p| {
                p.components()
                    .filter_map(|c| match c {
                        Component::Normal(s) => Some(s.to_string_lossy().to_ascii_lowercase()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let file_name = rel.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let lower_name = file_name.to_ascii_lowercase();

        // 0. Explicit user override beats every inferred rule.
        if dirs.iter().any(|d| self.source_dirs.contains(d)) {
            return self.source_or_test(&dirs, &file_name, "--source-dir override");
        }

        // 1. Build output. Checked first: a vendor library's build directory is
        //    still build output, and nothing under it is authored.
        if let Some(reason) = self.build_reason(&dirs) {
            return Classification { role: Role::Build, reason };
        }

        // 2. Vendored / third-party code.
        if let Some(reason) = self.vendor_reason(rel, &dirs) {
            return Classification { role: Role::Vendor, reason };
        }

        // 3. Generated source.
        if let Some(reason) = self.generated_reason(&dirs, &lower_name, head) {
            return Classification { role: Role::Generated, reason };
        }

        // 4. Everything left is the project's own. Split it by what the file is.
        let in_docs = dirs.iter().any(|d| DOC_DIRS.contains(&d.as_str()));
        let kind = lang::detect(rel).map(|l| l.kind).unwrap_or(FileKind::Other);
        match kind {
            // Code under docs/ is a doc-site build, a theme, or a snippet —
            // never the project's shipping source. Attributing it to Docs keeps
            // it visible without inflating the headline figure.
            FileKind::Code if in_docs => Classification { role: Role::Docs, reason: "code inside a documentation directory" },
            FileKind::Code => self.source_or_test(&dirs, &file_name, "project source tree"),
            FileKind::Docs => Classification { role: Role::Docs, reason: "documentation file type" },
            FileKind::Config if in_docs => Classification { role: Role::Docs, reason: "inside a documentation directory" },
            FileKind::Config => Classification { role: Role::Config, reason: "configuration file type" },
            FileKind::Asset => Classification { role: Role::Asset, reason: "binary or media file type" },
            FileKind::Other => {
                if dirs.iter().any(|d| ASSET_DIRS.contains(&d.as_str())) {
                    Classification { role: Role::Asset, reason: "inside an asset directory" }
                } else if in_docs {
                    Classification { role: Role::Docs, reason: "inside a documentation directory" }
                } else {
                    Classification { role: Role::Other, reason: "unrecognised file type" }
                }
            }
        }
    }

    fn source_or_test(&self, dirs: &[String], file_name: &str, base: &'static str) -> Classification {
        if self.treat_tests_as_source {
            return Classification { role: Role::Source, reason: base };
        }
        if dirs.iter().any(|d| TEST_DIRS.contains(&d.as_str())) {
            return Classification { role: Role::Test, reason: "inside a test directory" };
        }
        if looks_like_test_file(file_name) {
            return Classification { role: Role::Test, reason: "test naming convention" };
        }
        Classification { role: Role::Source, reason: base }
    }

    fn build_reason(&self, dirs: &[String]) -> Option<&'static str> {
        for d in dirs {
            if self.build_dirs.contains(d) {
                return Some("--build-dir override");
            }
            if BUILD_DIRS.contains(&d.as_str()) {
                return Some("inside a known build/cache directory");
            }
            if BUILD_PREFIXES.iter().any(|p| d.starts_with(p)) {
                return Some("directory name starts with a build prefix");
            }
        }
        None
    }

    fn vendor_reason(&self, rel: &Path, dirs: &[String]) -> Option<&'static str> {
        for d in dirs {
            if self.vendor_dirs.contains(d) {
                return Some("--vendor-dir override");
            }
            if VENDOR_DIRS.contains(&d.as_str()) {
                return Some("inside a known third-party directory");
            }
        }
        if self.submodules.iter().any(|s| rel.starts_with(s)) {
            return Some("declared as a git submodule in .gitmodules");
        }
        if self.in_nested_repo(rel) {
            return Some("inside a nested git repository");
        }
        None
    }

    fn generated_reason(&self, dirs: &[String], lower_name: &str, head: Option<&str>) -> Option<&'static str> {
        for d in dirs {
            if self.generated_dirs.contains(d) {
                return Some("--generated-dir override");
            }
            if GENERATED_DIRS.contains(&d.as_str()) {
                return Some("inside a generated-code directory");
            }
        }
        if GENERATED_SUFFIXES.iter().any(|s| lower_name.ends_with(s)) {
            return Some("filename matches a code-generator output pattern");
        }
        if GENERATED_PREFIXES.iter().any(|p| lower_name.starts_with(p)) {
            return Some("filename matches a code-generator output pattern");
        }
        if self.detect_markers {
            if let Some(head) = head {
                let probe = head.to_ascii_lowercase();
                if GENERATED_MARKERS.iter().any(|m| probe.contains(m)) {
                    return Some("file header carries a \"generated\" marker");
                }
            }
        }
        None
    }

    /// Walks up from the file towards the scan root looking for a directory
    /// that carries its own `.git`. Results are memoised per directory, so a
    /// vendored repo with 5,000 files costs a handful of `stat` calls.
    fn in_nested_repo(&self, rel: &Path) -> bool {
        let mut current = rel.parent();
        while let Some(dir) = current {
            if dir.as_os_str().is_empty() {
                break; // reached the scan root itself
            }
            let key = dir.to_path_buf();
            let cached = self.nested_repos.lock().ok().and_then(|m| m.get(&key).copied());
            let is_repo = match cached {
                Some(v) => v,
                None => {
                    let v = self.root.join(dir).join(".git").exists();
                    if let Ok(mut m) = self.nested_repos.lock() {
                        m.insert(key, v);
                    }
                    v
                }
            };
            if is_repo {
                return true;
            }
            current = dir.parent();
        }
        false
    }
}

fn lowercased(v: &[String]) -> Vec<String> {
    v.iter().map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()
}

/// Extract `path = ...` entries from a root `.gitmodules`, if present.
fn read_submodules(root: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root.join(".gitmodules")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("path")?.trim_start();
            let value = rest.strip_prefix('=')?.trim();
            (!value.is_empty()).then(|| PathBuf::from(value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier() -> Classifier {
        Classifier::new(Path::new("/nonexistent-root"))
    }

    fn role_of(p: &str) -> Role {
        classifier().classify(Path::new(p), None).role
    }

    #[test]
    fn vendored_engines_are_not_project_code() {
        // The exact case the old size heuristic got wrong.
        assert_eq!(role_of("third_party/bgfx/src/renderer_vk.cpp"), Role::Vendor);
        assert_eq!(role_of("third_party/Jolt/Physics/Body/Body.cpp"), Role::Vendor);
        assert_eq!(role_of("external/imgui/imgui.cpp"), Role::Vendor);
        assert_eq!(role_of("deps/glfw/src/window.c"), Role::Vendor);
        assert_eq!(role_of("node_modules/react/index.js"), Role::Vendor);
    }

    #[test]
    fn a_large_project_header_is_still_project_code() {
        // Size and extension are irrelevant; only position matters.
        assert_eq!(role_of("src/engine/renderer.hpp"), Role::Source);
    }

    #[test]
    fn build_variants_are_all_build_output() {
        for p in [
            "build/CMakeFiles/foo.cpp",
            "build-asan/gen/thing.c",
            "build-ship/x/y.cpp",
            "cmake-build-debug/a.c",
            "target/debug/build/x.rs",
            "out/bundle.js",
        ] {
            assert_eq!(role_of(p), Role::Build, "{p}");
        }
    }

    #[test]
    fn build_wins_over_vendor() {
        assert_eq!(role_of("third_party/bgfx/build/gen.cpp"), Role::Build);
    }

    #[test]
    fn generated_sources_are_separated() {
        assert_eq!(role_of("src/api/service.pb.go"), Role::Generated);
        assert_eq!(role_of("src/ui/moc_mainwindow.cpp"), Role::Generated);
        assert_eq!(role_of("src/model/user.g.dart"), Role::Generated);
        assert_eq!(role_of("src/generated/schema.rs"), Role::Generated);
    }

    #[test]
    fn generator_banner_is_detected() {
        let c = classifier();
        let r = c.classify(Path::new("src/parser.rs"), Some("// @generated by lalrpop"));
        assert_eq!(r.role, Role::Generated);
    }

    #[test]
    fn tests_are_split_from_source() {
        assert_eq!(role_of("tests/integration.rs"), Role::Test);
        assert_eq!(role_of("src/parser_test.go"), Role::Test);
        assert_eq!(role_of("src/utils/test_math.py"), Role::Test);
        assert_eq!(role_of("src/app.spec.ts"), Role::Test);
    }

    #[test]
    fn tests_can_be_folded_into_source() {
        let c = classifier().merge_tests_into_source(true);
        assert_eq!(c.classify(Path::new("tests/integration.rs"), None).role, Role::Source);
    }

    #[test]
    fn docs_config_and_assets_are_kept_apart() {
        assert_eq!(role_of("README.md"), Role::Docs);
        assert_eq!(role_of("Cargo.toml"), Role::Config);
        assert_eq!(role_of("assets/logo.png"), Role::Asset);
        assert_eq!(role_of("docs/notes.rst"), Role::Docs);
        // A generated doc site under docs/ must not land in the headline figure.
        assert_eq!(role_of("docs/api/index.html"), Role::Docs);
    }

    #[test]
    fn source_dir_override_rescues_a_vendor_named_directory() {
        let c = classifier().source_dirs(&["vendor".to_string()]);
        // A Go project whose own code genuinely lives in vendor/.
        assert_eq!(c.classify(Path::new("vendor/mypkg/main.go"), None).role, Role::Source);
    }

    #[test]
    fn vendor_dir_override_adds_a_directory() {
        let c = classifier().vendor_dirs(&["libs".to_string()]);
        assert_eq!(c.classify(Path::new("libs/zlib/deflate.c"), None).role, Role::Vendor);
    }

    #[test]
    fn only_source_and_tests_count_as_project_code() {
        assert!(Role::Source.is_project_code());
        assert!(Role::Test.is_project_code());
        for r in [Role::Vendor, Role::Generated, Role::Build, Role::Docs, Role::Config, Role::Asset, Role::Other] {
            assert!(!r.is_project_code(), "{r:?}");
        }
    }
}
