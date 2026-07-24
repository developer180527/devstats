# devstat User Guide

A practical manual for running devstat on your project and trusting what comes
back.

**Contents**

1. [What devstat answers](#1-what-devstat-answers)
2. [Install](#2-install)
3. [Your first run](#3-your-first-run)
4. [Reading the output](#4-reading-the-output)
5. [The nine roles](#5-the-nine-roles)
6. [When a number looks wrong](#6-when-a-number-looks-wrong)
7. [Recipes](#7-recipes)
8. [Flag reference](#8-flag-reference)
9. [Output formats](#9-output-formats)
10. [Limits and honest caveats](#10-limits-and-honest-caveats)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. What devstat answers

Most line counters answer *"how many lines are in this tree?"* On a real project
that number is dominated by code nobody on the team wrote: a bundled physics
engine, protobuf output, three build directories.

devstat answers **"how much of this did we write?"**

It does that by giving every file a **role** based on **where it sits in the
tree** — never on how big the file is or what extension it has. A 12,000-line
header is your code in `src/` and somebody else's in `third_party/`.

---

## 2. Install

```bash
cargo install --path .
```

Or run it out of the build directory:

```bash
cargo build --release
./target/release/devstat --version
```

No configuration file, no setup step, no cache directory. devstat reads only the
tree you point it at.

---

## 3. Your first run

```bash
devstat
```

That scans the current directory. To scan somewhere else:

```bash
devstat ~/code/my-engine
```

If you only want the bottom line:

```bash
devstat -q
```

```
  Project code       148,392  lines across 1,204 files (Source + Tests)
  Comments            21,908  lines (12.9% of all project lines)
  Vendored         2,624,015  lines — reported separately, not yours
  Generated           91,244  lines — reported separately, not yours
  Build output        6.14 GB  across 41,882 files — reclaimable, no authorship
  Total scanned       61,330  files, 8.02 GB on disk
```

---

## 4. Reading the output

A full run prints four blocks.

### Summary

The six lines above. **Project code** is the headline, and it always names the
roles it summed — `(Source + Tests)` by default — so the figure can never quietly
mean something other than what it says.

Read it together with the lines beneath it. "148k ours, 2.6M vendored" is a very
different project from "148k ours, 0 vendored", and both are useful facts.

### Project code by language

Files, code, comments, blank lines and share of the total, per language. This
table covers **only** the roles in the headline figure — vendored C++ never
appears here. A `TOTAL` row closes it.

```
┌────────────┬───────┬───────┬──────────┬───────┬───────┐
│ Language   │ Files │  Code │ Comments │ Blank │ Share │
╞════════════╪═══════╪═══════╪══════════╪═══════╪═══════╡
│ C++        │   412 │ 96,204 │   14,882 │ 12,004 │ 64.8% │
│ C++ Header │   388 │ 41,110 │    5,904 │  4,881 │ 27.7% │
│ Rust       │    24 │ 11,078 │    1,122 │  1,388 │  7.5% │
│ TOTAL      │   824 │148,392 │   21,908 │ 18,273 │  100% │
└────────────┴───────┴───────┴──────────┴───────┴───────┘
```

### Classification

Where every file went, and — the important column — whether it counted.

```
┌──────────────┬────────┬───────────┬──────────┬──────────┬────────────────────┐
│ Role         │  Files │      Code │ Comments │  On disk │ Counted as project │
╞══════════════╪════════╪═══════════╪══════════╪══════════╪════════════════════╡
│ Source       │    736 │   127,484 │   19,332 │  14.1 MB │ yes                │
│ Tests        │     88 │    20,908 │    2,576 │   2.2 MB │ yes                │
│ Vendor       │ 12,904 │ 2,624,015 │  402,118 │ 812.4 MB │ no                 │
│ Generated    │    204 │    91,244 │    1,002 │   9.8 MB │ no                 │
│ Build output │ 41,882 │         — │        — │   6.14 GB │ no                 │
└──────────────┴────────┴───────────┴──────────┴──────────┴────────────────────┘
```

The `—` on **Build output** is not a zero. Those files are never opened at all;
only their size is measured. That is deliberate — it is what keeps a
multi-gigabyte build tree cheap to scan, and a line count of compiler output
would be meaningless anyway.

### Project code by directory

Which parts of *your* code are big. Group deeper with `--depth`:

```bash
devstat --depth 2 --top 25
```

Files sitting directly in the scan root are grouped under `.`.

---

## 5. The nine roles

Rules are applied **in this order**, and the first match wins.

| # | Role | What it means | Counted? |
|---|---|---|---|
| 1 | `Build output` | Compiler/packager output. Size only, never read. | no |
| 2 | `Vendor` | Third-party code living in your tree. | no |
| 3 | `Generated` | Machine-emitted source a tool will rewrite. | no |
| 4 | `Tests` | Your own tests, benchmarks, fixtures. | **yes** |
| 5 | `Source` | Code your team wrote and maintains. | **yes** |
| — | `Docs` | Prose, and anything under `docs/`. | no |
| — | `Config` | Declarative project configuration. | no |
| — | `Assets` | Images, fonts, models, binaries. | no |
| — | `Other` | Recognised file that fits nowhere above. | no |

Order matters more than it looks. Because **Build output is checked first**, a
vendored library's own `build/` directory is build output rather than vendor —
it is not code anyone wrote, no matter whose subtree it appears in.

### How each is recognised

**Build output** — directory names and prefixes: `build`, `build-asan`,
`build-ship`, `cmake-build-debug`, `target`, `out`, `dist`, `obj`, `_build`,
`__pycache__`, `.gradle`, `.next`, `.tox`, `coverage`, and more. Any directory
starting with `build` is caught, so your naming variants work without
configuration.

**Vendor** — three independent signals:
- directory names: `third_party`, `third-party`, `3rdparty`, `vendor`,
  `external`, `extern`, `deps`, `dependencies`, `node_modules`, `Pods`,
  `vcpkg`, `conan`, `submodules`, `contrib`, `sdk`, and others;
- paths listed in the repository's root `.gitmodules`;
- any nested directory carrying its own `.git` — a vendored checkout is
  recognised even if its directory has an ordinary name.

**Generated** — three signals:
- directory names: `generated`, `__generated__`, `gen`, `codegen`, `autogen`,
  `moc`, `uic`, `rcc`;
- filename patterns: `*.pb.cc`, `*.pb.go`, `*_pb2.py`, `moc_*`, `ui_*`,
  `qrc_*`, `*.g.dart`, `*.freezed.dart`, `*.designer.cs`, `*.min.js`,
  `*-lock.json`, and more;
- a `@generated`, `DO NOT EDIT`, or `Code generated by` banner in the first
  512 characters of the file.

**Tests** — directory names (`test`, `tests`, `spec`, `__tests__`, `benchmarks`,
`testdata`, …) or filename conventions (`test_*`, `*_test.*`, `*.spec.*`,
`FooTest.java`, `FooTests.cs`).

Fold tests into the source figure with `--tests-as-source` if you'd rather see
one number.

---

## 6. When a number looks wrong

Every decision is traceable. Ask devstat why:

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

The path can be relative to the scan root or absolute. It does not even have to
exist — devstat will classify a hypothetical path from its shape alone, which is
handy for checking a rule before you restructure a directory.

Once you know the rule, change it:

| Flag | Effect |
|---|---|
| `--source-dir <NAME>` | Force this directory name to count as project source |
| `--vendor-dir <NAME>` | Treat this directory name as third-party |
| `--build-dir <NAME>` | Treat this directory name as build output |
| `--generated-dir <NAME>` | Treat this directory name as generated |

All four are repeatable and match on **any path component**, not just the top
level. `--vendor-dir libs` catches `apps/game/libs/zlib/deflate.c`.

`--source-dir` beats every other rule. That is the escape hatch for a Go project
whose own code genuinely lives in `vendor/`:

```bash
devstat --source-dir vendor
```

### Cases worth checking on your project

Three defaults are judgement calls rather than facts, and they are the ones most
likely to disagree with your layout:

- **`packages`, `contrib` and `sdk` are treated as vendor.** Some projects use
  those for their own code. Fix with `--source-dir packages`.
- **Code under `docs/` is attributed to Docs, not Source.** Doc-site themes and
  generated HTML there are a common source of inflation. If you keep real
  shipping code under `docs/`, use `--source-dir docs`.
- **`gen` is treated as generated.** If it's short for something else in your
  tree, use `--source-dir gen`.

---

## 7. Recipes

**A quick honest number for a status update**

```bash
devstat -q
```

**Find what is eating your disk**

```bash
devstat --largest-files 20
```

**See which subsystem is biggest**

```bash
devstat --by dirs --depth 2 --top 30
```

**Only look at one subtree**

```bash
devstat --include 'src/engine/**'
```

**Exclude a directory without reclassifying it**

```bash
devstat --exclude 'src/legacy/**' --exclude '**/*.generated.cpp'
```

`--include`/`--exclude` remove files from the report entirely. Use them to
narrow a question; use `--vendor-dir` and friends to fix a wrong role.

**Scan a huge tree quickly**

```bash
devstat --fast
```

Measures vendor and generated trees by size only. You lose their line counts and
keep everything else.

**Count everything, including dependencies**

```bash
devstat --include-vendor --include-generated
```

The summary then says `(Source + Tests + Vendor + Generated)` so the number stays
self-describing.

**Track the figure over time in CI**

```bash
devstat --format json --no-color > stats.json
jq .summary.project_code stats.json
```

**Add repository history**

```bash
devstat --git
```

Appends branch, commit count, contributor count, first and last commit dates,
days with commits, uncommitted changes, and the top five contributors. Requires
`git` on `PATH` and a work tree; silently omitted otherwise.

**Paste a report into a PR or wiki**

```bash
devstat --format markdown
```

**Find your biggest files by line count**

```bash
devstat --by files --top 20
```

**See what devstat could not read**

```bash
devstat -v
```

Lists files skipped as binary, unreadable, or over `--max-file-size`.

---

## 8. Flag reference

### Output

| Flag | Default | Effect |
|---|---|---|
| `-f, --format <table\|json\|csv\|markdown>` | `table` | Output format |
| `--by <languages\|roles\|dirs\|files>` | languages, roles, dirs | Pick sections; repeatable |
| `--top <N>` | `15` | Max rows per table |
| `--sort <code\|files\|size\|name>` | `code` | Row ordering |
| `--depth <N>` | `1` | Directory grouping depth |
| `--largest-files <N>` | `0` | Add a largest-files-on-disk table |
| `--min-code <N>` | `0` | Hide rows below N lines of code |
| `-q, --quiet` | — | Summary only |
| `-v, --verbose` | — | List files that could not be counted |
| `--no-color` | — | Disable ANSI colour (`NO_COLOR` honoured too) |

`-q` and `-v` are mutually exclusive. Colour is switched off automatically for
every format except `table`, so redirected output is always clean.

### What counts

| Flag | Default | Effect |
|---|---|---|
| `--tests-as-source` | off | Fold tests into the source figure |
| `--include-vendor` | off | Count vendored code in the project total |
| `--include-generated` | off | Count generated code in the project total |
| `--no-marker-detection` | off | Don't read file heads looking for `@generated` |
| `--fast` | off | Measure vendor and generated by size only |
| `--max-file-size <MB>` | `16` | Skip counting files above this size |

### Classification overrides

| Flag | Effect |
|---|---|
| `--source-dir <NAME>` | Force a directory name to count as project source |
| `--vendor-dir <NAME>` | Treat a directory name as third-party |
| `--build-dir <NAME>` | Treat a directory name as build output |
| `--generated-dir <NAME>` | Treat a directory name as generated |

Repeatable; matched case-insensitively against any path component.

### Traversal

| Flag | Default | Effect |
|---|---|---|
| `--include <GLOB>` | — | Only analyse matching paths; repeatable |
| `--exclude <GLOB>` | — | Skip matching paths; repeatable |
| `--hidden` | off | Include hidden files and directories |
| `--no-ignore` | off | Ignore `.gitignore` / `.ignore` rules |
| `--follow-links` | off | Follow symlinks |
| `-j, --threads <N>` | `0` | Worker threads (0 = one per core) |

Globs match paths relative to the scan root. A pattern with no wildcard, like
`--exclude src/legacy`, is also matched as `src/legacy/**`, so the obvious
shorthand does what you expect.

### One-shot modes

| Flag | Effect |
|---|---|
| `--explain <PATH>` | Show how one path is classified, and why |
| `--list-languages` | Print all 86 recognised languages and their extensions |
| `--git` | Append repository history statistics |
| `-V, --version` | Print the version |
| `-h, --help` | Full help with examples |

---

## 9. Output formats

### `--format json`

One object with `tool`, `version`, `root`, `summary`, `roles`, `languages` and
`directories`. Adding `--by files` includes one record per file:

```json
{"path":"src/proto/api.pb.cc","role":"generated",
 "reason":"filename matches a code-generator output pattern",
 "language":"C++","code":4,"comments":1,"blank":0,"bytes":48}
```

`--git` adds a `git` object. Every file record carries its `reason`, so a
pipeline can audit the classification, not just consume the totals.

### `--format csv`

Flat rows for a spreadsheet, with a `section` column distinguishing them:

```
section,name,role,files,code,comments,blank,bytes
role,Source,source,736,127484,19332,14204,14812416
language,C++,,412,96204,14882,12004,9832100
directory,src/engine,,188,52104,7220,6110,5120044
```

### `--format markdown`

The same tables as `table`, in GitHub-flavoured markdown, with the summary as a
bullet list. Good for pasting into a PR description.

---

## 10. Limits and honest caveats

**The line counter is a scanner, not a parser.** It tracks block comments and
string literals — so `"http://example.com"` is not mistaken for a comment — but
raw strings, heredocs and nested block comments are approximated. A line holding
any code counts as code even if it also carries a trailing comment, the same
convention `cloc` and `tokei` use. For an exactly-correct count of one language,
use that language's own tooling; devstat exists to make a whole tree's
*proportions* trustworthy.

**Classification is rule-based, not magic.** It reads directory names,
`.gitmodules`, nested `.git` markers and generator banners. A dependency copied
into `src/mylib_copy/` with no other signal will be counted as your code — and
should be, as far as devstat can tell. Check surprising directories with
`--explain` and correct them with `--vendor-dir`.

**Assets and unrecognised file types are never line-counted**, only sized. Their
`Code` column reads `0`.

**Files above `--max-file-size` (16 MB) are sized but not counted**, and appear
under `-v`.

**`--git` shells out to `git`.** If it is missing, or the directory is not a work
tree, the section is omitted rather than failing the run.

---

## 11. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `no files matched under …` | Everything was filtered out, or the whole tree is gitignored | Check `--include`/`--exclude`, or add `--no-ignore` |
| Project code looks far too high | A dependency is in a directory devstat doesn't know | `devstat --explain <a file in it>`, then `--vendor-dir <name>` |
| Project code looks far too low | Your source lives somewhere on the vendor/build lists | `--source-dir <name>` — it beats every other rule |
| A directory you expect is missing | It is gitignored | `--no-ignore` |
| Dotfiles missing | Hidden by default | `--hidden` |
| Build output not recognised | Unusual directory name | `--build-dir <name>` |
| A hand-written file is marked Generated | It contains a `DO NOT EDIT` banner | `--no-marker-detection`, or `--source-dir` on its directory |
| Scan is slow on a huge tree | Vendor trees are being line-counted | `--fast` |
| Colour codes in a redirected file | Forcing table format | `--no-color`, or set `NO_COLOR` |

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Unreadable path, path is not a directory, bad glob, or no files matched |

---

For how any of this is implemented, see [INTERNALS.md](../INTERNALS.md).
