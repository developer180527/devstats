# devstat

Codebase statistics that tell **your code** apart from vendored, generated and
build output.

Most line counters answer "how many lines are in this tree?". In a real project
that number is dominated by things nobody on the team wrote — a bundled physics
engine, protobuf output, three build directories. devstat answers the question
you actually asked: *how much of this did we write?*

```
  Project code       148,392  lines across 1,204 files (Source + Tests)
  Comments            21,908  lines (12.9% of all project lines)
  Vendored         2,624,015  lines — reported separately, not yours
  Generated           91,244  lines — reported separately, not yours
  Build output        6.14 GB  across 41,882 files — reclaimable, no authorship
  Total scanned       61,330  files, 8.02 GB on disk
```

## Install

```bash
cargo install --path .
```

## The classification model

Every file gets exactly one **role**, and the role is decided by **where the
file sits in the tree**, not by how big it is or what extension it has. A
12,000-line header is project code in `src/` and vendor code in `third_party/`.

| Role | What it means | In the headline figure? |
|---|---|---|
| `Source` | Code the team wrote and maintains | yes |
| `Tests` | The project's own tests and benchmarks | yes |
| `Vendor` | Third-party code living in the tree | no |
| `Generated` | Machine-emitted source a tool will rewrite | no |
| `Build output` | Compiler/packager output — size only, never read | no |
| `Docs` | Prose, and anything under `docs/` | no |
| `Config` | Declarative project configuration | no |
| `Assets` | Images, fonts, models, binaries | no |
| `Other` | Recognised file that fits nowhere above | no |

Rules are applied in that order, so build output *inside* a vendored library is
still build output.

**Vendor** is recognised from directory names (`third_party`, `external`,
`vendor`, `deps`, `node_modules`, `Pods`, `vcpkg`, …), from `.gitmodules`
entries, and from any nested directory carrying its own `.git`.

**Generated** is recognised from directory names (`generated/`, `gen/`,
`codegen/`), from filename patterns (`*.pb.cc`, `*_pb2.py`, `moc_*`, `ui_*`,
`*.g.dart`, `*.min.js`), and from a `@generated` / `DO NOT EDIT` banner in the
first 512 bytes.

**Build output** is recognised from directory names and prefixes, so `build`,
`build-asan`, `build-ship`, `cmake-build-debug`, `target`, `out`, `dist`,
`__pycache__` and friends are all caught. These files are never opened — only
their size is measured, which is what keeps a multi-gigabyte build tree cheap to
report on.

Nothing is guessed from file size or line count.

### When it gets it wrong

Every rule is overridable, and every decision is traceable:

```bash
devstat --explain third_party/bgfx/src/renderer_vk.cpp
```

```
path: third_party/bgfx/src/renderer_vk.cpp
exists:     yes
language:   C++
role:       Vendor
reason:     inside a known third-party directory
counted:    no — reported separately from project code
```

Then fix it with `--source-dir`, `--vendor-dir`, `--build-dir` or
`--generated-dir`. `--source-dir` beats every other rule, which is the escape
hatch for a Go project whose own code genuinely lives in `vendor/`.

## Usage

```bash
devstat                                   # report on the current directory
devstat ~/code/engine --top 20            # widen the tables
devstat --by dirs --depth 2               # group two directories deep
devstat --format json > stats.json        # machine-readable
devstat --git --largest-files 10          # add repo history and disk hogs
devstat --vendor-dir libs --vendor-dir sdk
devstat --explain src/proto/api.pb.cc
```

### Output

| Flag | Effect |
|---|---|
| `-f, --format <table\|json\|csv\|markdown>` | Output format (default `table`) |
| `--by <languages\|roles\|dirs\|files>` | Pick sections; repeatable (default: languages, roles, dirs) |
| `--top <N>` | Max rows per table (default 15) |
| `--sort <code\|files\|size\|name>` | Row ordering |
| `--depth <N>` | Directory grouping depth (default 1) |
| `--largest-files <N>` | Add a largest-files-on-disk table |
| `--min-code <N>` | Hide rows below N lines of code |
| `-q, --quiet` | Summary line only |
| `-v, --verbose` | List files that could not be read or decoded |
| `--no-color` | Disable ANSI colour (`NO_COLOR` is honoured too) |

### What counts

| Flag | Effect |
|---|---|
| `--tests-as-source` | Fold tests into the source figure |
| `--include-vendor` | Count vendored code in the project total |
| `--include-generated` | Count generated code in the project total |
| `--no-marker-detection` | Don't read file heads looking for `@generated` |
| `--fast` | Measure vendor and generated trees by size only |
| `--max-file-size <MB>` | Skip counting files above this size (default 16) |

### Classification overrides

| Flag | Effect |
|---|---|
| `--source-dir <NAME>` | Force a directory name to count as project source |
| `--vendor-dir <NAME>` | Treat a directory name as third-party |
| `--build-dir <NAME>` | Treat a directory name as build output |
| `--generated-dir <NAME>` | Treat a directory name as generated |

All are repeatable and match on any path component.

### Traversal

| Flag | Effect |
|---|---|
| `--include <GLOB>` / `--exclude <GLOB>` | Filter paths; repeatable |
| `--hidden` | Include hidden files and directories |
| `--no-ignore` | Ignore `.gitignore` / `.ignore` rules |
| `--follow-links` | Follow symlinks |
| `-j, --threads <N>` | Worker threads (0 = one per core) |

### One-shot modes

| Flag | Effect |
|---|---|
| `--explain <PATH>` | Show how one path is classified, and why |
| `--list-languages` | Print every recognised language |
| `--git` | Append commit count, contributors, history span |
| `-V, --version` | Print the version |

## Line counting

Lines are split into **code**, **comments** and **blank**. The counter tracks
block comments and string literals, so `"http://example.com"` is not mistaken
for a comment. A line holding any code counts as code even if it also carries a
trailing comment — the same convention `cloc` and `tokei` use.

It is a scanner, not a parser. Raw strings, heredocs and nested block comments
are approximated. For an exactly-correct count of a single language, use that
language's own tooling; devstat is built to make a whole tree's proportions
trustworthy.

## Machine-readable output

`--format json` emits the summary, per-role, per-language and per-directory
breakdowns; adding `--by files` includes one record per file with its role and
the reason it was assigned:

```json
{"path":"src/proto/api.pb.cc","role":"generated",
 "reason":"filename matches a code-generator output pattern",
 "language":"C++","code":4,"comments":1,"blank":0,"bytes":48}
```

`--format csv` emits the same sections as flat rows for spreadsheets.

## Exit codes

`0` on success, `1` on an unreadable path, a bad glob, or when no files matched.
