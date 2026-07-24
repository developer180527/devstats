//! Language table: maps file extensions and bare filenames onto a language
//! definition that knows how to recognise its own comments.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// What a file contributes to the project, independent of *where* it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// Something a compiler, interpreter or shader toolchain consumes.
    Code,
    /// Prose: markdown, rst, plain text.
    Docs,
    /// Declarative project/tool configuration.
    Config,
    /// Images, fonts, audio, models, binaries.
    Asset,
    /// Recognised extension that fits nowhere above, or no extension at all.
    Other,
}

#[derive(Debug)]
pub struct Language {
    pub name: &'static str,
    pub kind: FileKind,
    /// Tokens that comment out the remainder of a line.
    pub line: &'static [&'static str],
    /// (open, close) pairs. Nesting is not tracked.
    pub block: &'static [(&'static str, &'static str)],
    /// Quote bytes that open a string literal, so `"http://x"` is not read as a
    /// comment. Backslash escapes are honoured inside the literal.
    pub quotes: &'static [u8],
    pub exts: &'static [&'static str],
    /// Exact filenames (case sensitive) that identify the language on their own.
    pub filenames: &'static [&'static str],
}

const C_LINE: &[&str] = &["//"];
const C_BLOCK: &[(&str, &str)] = &[("/*", "*/")];
const HASH: &[&str] = &["#"];
const NO_LINE: &[&str] = &[];
const NO_BLOCK: &[(&str, &str)] = &[];
const DQ: &[u8] = b"\"";
const DQSQ: &[u8] = b"\"'";
const NO_Q: &[u8] = b"";

macro_rules! lang {
    ($name:expr, $kind:expr, $line:expr, $block:expr, $quotes:expr, $exts:expr) => {
        Language {
            name: $name,
            kind: $kind,
            line: $line,
            block: $block,
            quotes: $quotes,
            exts: $exts,
            filenames: &[],
        }
    };
    ($name:expr, $kind:expr, $line:expr, $block:expr, $quotes:expr, $exts:expr, $files:expr) => {
        Language {
            name: $name,
            kind: $kind,
            line: $line,
            block: $block,
            quotes: $quotes,
            exts: $exts,
            filenames: $files,
        }
    };
}

use FileKind::*;

pub static LANGUAGES: &[Language] = &[
    // ---- C family ---------------------------------------------------------
    lang!("C", Code, C_LINE, C_BLOCK, DQSQ, &["c"]),
    lang!("C Header", Code, C_LINE, C_BLOCK, DQSQ, &["h"]),
    lang!("C++", Code, C_LINE, C_BLOCK, DQSQ, &["cpp", "cc", "cxx", "c++", "ipp", "inl"]),
    lang!("C++ Header", Code, C_LINE, C_BLOCK, DQSQ, &["hpp", "hh", "hxx", "h++"]),
    lang!("Objective-C", Code, C_LINE, C_BLOCK, DQSQ, &["m"]),
    lang!("Objective-C++", Code, C_LINE, C_BLOCK, DQSQ, &["mm"]),
    lang!("C#", Code, C_LINE, C_BLOCK, DQSQ, &["cs", "csx"]),
    lang!("CUDA", Code, C_LINE, C_BLOCK, DQSQ, &["cu", "cuh"]),
    // ---- Systems ----------------------------------------------------------
    lang!("Rust", Code, C_LINE, C_BLOCK, DQ, &["rs"]),
    lang!("Go", Code, C_LINE, C_BLOCK, DQ, &["go"]),
    lang!("Zig", Code, C_LINE, NO_BLOCK, DQ, &["zig"]),
    lang!("Nim", Code, HASH, &[("#[", "]#")], DQ, &["nim"]),
    lang!("D", Code, C_LINE, C_BLOCK, DQ, &["d"]),
    lang!("Assembly", Code, &[";", "#", "//"], C_BLOCK, DQ, &["asm", "s", "S"]),
    // ---- JVM / .NET -------------------------------------------------------
    lang!("Java", Code, C_LINE, C_BLOCK, DQSQ, &["java"]),
    lang!("Kotlin", Code, C_LINE, C_BLOCK, DQ, &["kt", "kts"]),
    lang!("Scala", Code, C_LINE, C_BLOCK, DQ, &["scala", "sc"]),
    lang!("Groovy", Code, C_LINE, C_BLOCK, DQ, &["groovy"]),
    lang!("Clojure", Code, &[";"], NO_BLOCK, DQ, &["clj", "cljs", "cljc", "edn"]),
    lang!("F#", Code, &["//"], &[("(*", "*)")], DQ, &["fs", "fsi", "fsx"]),
    lang!("Visual Basic", Code, &["'"], NO_BLOCK, DQ, &["vb"]),
    // ---- Scripting --------------------------------------------------------
    lang!("Python", Code, HASH, NO_BLOCK, DQSQ, &["py", "pyi", "pyw", "pyx", "pxd"]),
    lang!("Ruby", Code, HASH, &[("=begin", "=end")], DQSQ, &["rb", "rake"], &["Rakefile", "Gemfile"]),
    lang!("Perl", Code, HASH, &[("=pod", "=cut")], DQSQ, &["pl", "pm", "t"]),
    lang!("PHP", Code, &["//", "#"], C_BLOCK, DQSQ, &["php", "phtml"]),
    lang!("Lua", Code, &["--"], &[("--[[", "]]")], DQSQ, &["lua"]),
    lang!("Shell", Code, HASH, NO_BLOCK, DQSQ, &["sh", "bash", "zsh", "ksh", "fish"]),
    lang!("PowerShell", Code, HASH, &[("<#", "#>")], DQSQ, &["ps1", "psm1", "psd1"]),
    lang!("Batch", Code, &["REM", "rem", "::"], NO_BLOCK, DQ, &["bat", "cmd"]),
    lang!("Tcl", Code, HASH, NO_BLOCK, DQ, &["tcl"]),
    lang!("R", Code, HASH, NO_BLOCK, DQ, &["r", "R"]),
    lang!("Julia", Code, HASH, &[("#=", "=#")], DQ, &["jl"]),
    lang!("MATLAB", Code, &["%"], &[("%{", "%}")], DQSQ, &["mat"]),
    lang!("Elixir", Code, HASH, NO_BLOCK, DQ, &["ex", "exs"]),
    lang!("Erlang", Code, &["%"], NO_BLOCK, DQ, &["erl", "hrl"]),
    lang!("Haskell", Code, &["--"], &[("{-", "-}")], DQ, &["hs", "lhs"]),
    lang!("OCaml", Code, NO_LINE, &[("(*", "*)")], DQ, &["ml", "mli"]),
    lang!("Lisp", Code, &[";"], &[("#|", "|#")], DQ, &["lisp", "el", "scm", "rkt"]),
    // ---- Web --------------------------------------------------------------
    lang!("JavaScript", Code, C_LINE, C_BLOCK, DQSQ, &["js", "mjs", "cjs", "jsx"]),
    lang!("TypeScript", Code, C_LINE, C_BLOCK, DQSQ, &["ts", "mts", "cts", "tsx"]),
    lang!("Vue", Code, C_LINE, &[("/*", "*/"), ("<!--", "-->")], DQSQ, &["vue"]),
    lang!("Svelte", Code, C_LINE, &[("/*", "*/"), ("<!--", "-->")], DQSQ, &["svelte"]),
    lang!("HTML", Code, NO_LINE, &[("<!--", "-->")], NO_Q, &["html", "htm", "xhtml"]),
    lang!("CSS", Code, NO_LINE, C_BLOCK, NO_Q, &["css"]),
    lang!("Sass", Code, C_LINE, C_BLOCK, NO_Q, &["scss", "sass", "less", "styl"]),
    lang!("Dart", Code, C_LINE, C_BLOCK, DQSQ, &["dart"]),
    lang!("Swift", Code, C_LINE, C_BLOCK, DQ, &["swift"]),
    lang!("Solidity", Code, C_LINE, C_BLOCK, DQ, &["sol"]),
    // ---- Graphics / engine ------------------------------------------------
    lang!("GLSL", Code, C_LINE, C_BLOCK, DQ, &["glsl", "vert", "frag", "geom", "comp", "tesc", "tese"]),
    lang!("HLSL", Code, C_LINE, C_BLOCK, DQ, &["hlsl", "fx", "cginc", "compute"]),
    lang!("Metal", Code, C_LINE, C_BLOCK, DQ, &["metal"]),
    lang!("WGSL", Code, C_LINE, C_BLOCK, DQ, &["wgsl"]),
    lang!("QML", Code, C_LINE, C_BLOCK, DQSQ, &["qml"]),
    lang!("ShaderLab", Code, C_LINE, C_BLOCK, DQ, &["shader"]),
    // ---- Interface / schema ----------------------------------------------
    lang!("Protobuf", Code, C_LINE, C_BLOCK, DQ, &["proto"]),
    lang!("Thrift", Code, &["//", "#"], C_BLOCK, DQ, &["thrift"]),
    lang!("GraphQL", Code, HASH, NO_BLOCK, DQ, &["graphql", "gql"]),
    lang!("SQL", Code, &["--"], C_BLOCK, DQSQ, &["sql"]),
    lang!("IDL", Code, C_LINE, C_BLOCK, DQ, &["idl"]),
    // ---- Build systems ----------------------------------------------------
    lang!("CMake", Code, HASH, NO_BLOCK, DQ, &["cmake"], &["CMakeLists.txt"]),
    lang!("Make", Code, HASH, NO_BLOCK, DQ, &["mk", "mak"], &["Makefile", "makefile", "GNUmakefile"]),
    lang!("Meson", Code, HASH, NO_BLOCK, DQSQ, &[], &["meson.build", "meson_options.txt"]),
    lang!("Bazel", Code, HASH, NO_BLOCK, DQ, &["bzl", "bazel"], &["BUILD", "BUILD.bazel", "WORKSPACE"]),
    lang!("Gradle", Code, C_LINE, C_BLOCK, DQ, &["gradle"]),
    lang!("Just", Code, HASH, NO_BLOCK, DQ, &[], &["Justfile", "justfile"]),
    lang!("Dockerfile", Code, HASH, NO_BLOCK, DQ, &["dockerfile"], &["Dockerfile", "Containerfile"]),
    lang!("Nix", Code, HASH, &[("/*", "*/")], DQ, &["nix"]),
    // ---- Config -----------------------------------------------------------
    lang!("JSON", Config, NO_LINE, NO_BLOCK, NO_Q, &["json", "jsonc", "json5"]),
    lang!("YAML", Config, HASH, NO_BLOCK, DQSQ, &["yaml", "yml"]),
    lang!("TOML", Config, HASH, NO_BLOCK, DQSQ, &["toml"]),
    lang!("XML", Config, NO_LINE, &[("<!--", "-->")], NO_Q, &["xml", "xsd", "xsl", "plist", "ui", "qrc"]),
    lang!("INI", Config, &[";", "#"], NO_BLOCK, NO_Q, &["ini", "cfg", "conf", "properties", "editorconfig"]),
    lang!("Env", Config, HASH, NO_BLOCK, DQSQ, &["env"]),
    // ---- Docs -------------------------------------------------------------
    lang!("Markdown", Docs, NO_LINE, NO_BLOCK, NO_Q, &["md", "markdown", "mdx"]),
    lang!("reStructuredText", Docs, NO_LINE, NO_BLOCK, NO_Q, &["rst"]),
    lang!("AsciiDoc", Docs, NO_LINE, NO_BLOCK, NO_Q, &["adoc", "asciidoc"]),
    lang!("Text", Docs, NO_LINE, NO_BLOCK, NO_Q, &["txt"], &["README", "LICENSE", "COPYING", "NOTICE", "AUTHORS", "CHANGELOG"]),
    lang!("TeX", Docs, &["%"], NO_BLOCK, NO_Q, &["tex", "sty", "bib"]),
    // ---- Assets -----------------------------------------------------------
    lang!(
        "Image",
        Asset,
        NO_LINE,
        NO_BLOCK,
        NO_Q,
        &["png", "jpg", "jpeg", "gif", "bmp", "tga", "webp", "ico", "svg", "psd", "exr", "hdr", "dds", "ktx"]
    ),
    lang!("Audio", Asset, NO_LINE, NO_BLOCK, NO_Q, &["wav", "mp3", "ogg", "flac", "aiff", "m4a"]),
    lang!("Video", Asset, NO_LINE, NO_BLOCK, NO_Q, &["mp4", "mov", "avi", "webm", "mkv"]),
    lang!("Font", Asset, NO_LINE, NO_BLOCK, NO_Q, &["ttf", "otf", "woff", "woff2", "eot"]),
    lang!("3D Model", Asset, NO_LINE, NO_BLOCK, NO_Q, &["fbx", "obj", "gltf", "glb", "dae", "blend", "usd", "usda", "usdc"]),
    lang!("Archive", Asset, NO_LINE, NO_BLOCK, NO_Q, &["zip", "tar", "gz", "bz2", "xz", "7z", "rar"]),
    lang!("Binary", Asset, NO_LINE, NO_BLOCK, NO_Q, &["so", "dylib", "dll", "a", "lib", "o", "obj", "exe", "pdb", "bin", "pak", "wasm", "class", "jar", "pyc"]),
    lang!("Data", Asset, NO_LINE, NO_BLOCK, NO_Q, &["csv", "tsv", "parquet", "db", "sqlite", "pb"]),
];

type Lookup = HashMap<&'static str, &'static Language>;

fn by_ext() -> &'static Lookup {
    static MAP: OnceLock<Lookup> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for l in LANGUAGES {
            for e in l.exts {
                m.entry(*e).or_insert(l);
            }
        }
        m
    })
}

fn by_filename() -> &'static Lookup {
    static MAP: OnceLock<Lookup> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for l in LANGUAGES {
            for f in l.filenames {
                m.entry(*f).or_insert(l);
            }
        }
        m
    })
}

/// Resolve a path to a language. Exact filename wins over extension so that
/// `CMakeLists.txt` is CMake rather than plain text.
pub fn detect(path: &Path) -> Option<&'static Language> {
    let name = path.file_name()?.to_str()?;

    if let Some(l) = by_filename().get(name) {
        return Some(l);
    }
    // `Dockerfile.dev`, `README.md` handled below; `.gitignore` has no stem.
    if let Some((stem, _)) = name.split_once('.') {
        if !stem.is_empty() {
            if let Some(l) = by_filename().get(stem) {
                return Some(l);
            }
        }
    }

    let ext = path.extension()?.to_str()?;
    if let Some(l) = by_ext().get(ext) {
        return Some(l);
    }
    // Extensions are matched case-sensitively first so `.S` (preprocessed
    // assembly) and `.s` can differ; fall back to lowercase for `.PNG` etc.
    let lower = ext.to_ascii_lowercase();
    by_ext().get(lower.as_str()).copied()
}
