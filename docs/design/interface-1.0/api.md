# API, command line and diagnostics

Part of the [1.0 interface RFC](../interface-1.0.md): #3030 sections 2.7
and 2.8, decisions D11, D12 and D28 to D46, and Phase 0 item 3. The bindings
draft is [`rust/interface/bindings-1.0.yaml`](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/rust/interface/bindings-1.0.yaml)
at the yamaa-v0.5.0 tag; `rust/interface/bindings.yaml` replaced it in 1.0
(#3434).
The printed text, error text and JSON below are designs, not captured output;
their messages illustrate the templates of `yaml/conditions.yaml` (P-MSG-1).

## Today's surface, measured

| Host | Public names | Result and lazy classes | Printing |
| --- | --- | --- | --- |
| Python | `__all__`: `Error`, `__version__`, `project`, `read_odm`, `spec` | `Check`, `Run`, `MemberRun`, `ProjectRun`, `Submission`, `Spec`, `Project`; 25 public members | none: `repr()` gives `<yamaa.Run object at 0x...>` |
| R | `yamaa_spec`, `yamaa_project`, `yamaa_read_odm`; the S3 method `[[.yamaa_project_run` | seven locked environments | none: `print()` shows the bare environment |
| Native (`rust/interface/bindings.yaml`, format 1) | Python: 17 exports, 15 class methods; R: 26 functions | | 24 paired operations |

- No result carries a status; callers count issue rows. The run report's
  `run` row already spells its outcome `succeeded` or `failed` (CHECK-24).
- `save()` returns nothing. A refusal raises `yamaa.Error` or signals
  `yamaa_error`, carrying `issues`.
- The issues table has 7 columns; the run report 14 (CHECK-24).
- Four facade refusals carry text but no issue (`rust/crates/yamaa/src/api.rs`):
  invalid resolved facts, an unknown member name, a project run that built
  nothing, a submission that composed nothing.
- Engine information is Python's `__version__` and R's
  `packageVersion("yamaa")`; the facade's optional Cargo features are `yaml`,
  `parquet`, `readstat` and `odm`.
- Lint is `python -m yamaa.style`, private and Python-only: 10 findings, 5
  fixable; each finding has a name, path, line, column, spec path and message.

## `status` (D36, P-STATUS-1)

| Option | Values | Assessment |
| --- | --- | --- |
| **A** | **`succeeded`, `failed`, `skipped`** | **D36's names; the first two are the run report's `OUTCOME` words** |
| B | A, plus `cancelled` | No interrupted result exists to label: an interrupt raises the host's own interrupt (D46) |
| C | A, plus `partial` or `warned` | Programs would test two values for "something went wrong"; warnings are report rows, not issues |

| Result | `succeeded` | `failed` | `skipped` |
| --- | --- | --- | --- |
| Check, of a spec or a project | no error-severity issue | at least one | never |
| Run, from `spec.run()` | an output was built and there is no error-severity issue | otherwise | never |
| MemberRun, one spec of a project run | as Run | as Run, including a member stopped by its own findings | it did not run because a run-scope producer failed or was skipped (`producer_not_completed`, PROJECT-11) |
| ProjectRun | every member succeeded and there is no project-level issue | a member failed or was skipped, or a project finding refused the run | never |
| Submission | every selected section composed with no issue | otherwise | never |
| SelfTest | every case passed | a case failed | never |

Rust computes `status` from the issues and the build; it is never a second
stored fact. `skipped` propagates to consumers. A future warning-severity
issue never changes the status. `status` describes the operation, not a later
save, whose refusal raises. A project run whose scope is empty succeeds, and
its `save()` refuses with `nothing_to_save`.

## Printed text (D37, D39, P-PRINT-1)

| Option | Form |
| --- | --- |
| **A** | **One summary line, then the first issue's line, rendered in Rust, the same bytes in every host** |
| B | A block of labelled fields, one per line |
| C | Host-native printing, which D37 rules out |

Rules:

1. Rust renders every line. Python's `__repr__` and `__str__` return it; R's
   `format()` returns its lines and `print()` writes them with
   `cat(sep = "\n")` and returns the object invisibly; the command line prints
   the same summary.
2. The first line of a run, project run or submission is `yamaa <kind> of
   <subject>: <status>, <size>, <n> issue(s)`, with the kind `run`,
   `project run` or `submission`. The subject of a run is its dataset name
   once read, otherwise the path as given; a project operation names the
   path, followed by `, section <s>` when a section was named. The size is
   `<r> rows x <c> columns`, `no output`, `<k> specifications` or `<d>
   define.xml and <t> transport files`. Counts are plain integers, singular
   for 1. Three forms differ, because they have no size:
   - a check reads no data, so it prints `yamaa check of <subject>: <status>,
     <n> issue(s)`;
   - a selftest prints `yamaa selftest: <status>, <p> of <c> cases passed`,
     then `first failure: <case>: <message>` when a case failed, then one
     line naming the engine, language, host and platform;
   - a lazy spec or project, which has read nothing, prints `yamaa spec
     <path>` or `yamaa project <path>`.
3. With issues, a second line gives the first: `first issue: <location>:
   <message> [<condition>, <rule>]`. The location is `<file>:<line>:<column>`,
   the file relative to the current directory when beneath it and absolute
   otherwise, with forward slashes; missing parts are dropped.
4. A project run lists its members between the two lines, in run order: two
   spaces, the name padded to the longest name, the status padded to 9
   characters, then the size and issue count. A member whose listing could
   not be read has no name; its line names it by the path the project file
   lists, as written, and so does the command line's `members` (D130).
5. ASCII and LF only; no colour; nothing depends on the terminal width.

```text
>>> yamaa.spec("sdtm/dm.yaml")
yamaa spec sdtm/dm.yaml
```

```text
yamaa check of sdtm/dm.yaml: failed, 2 issues
first issue: sdtm/dm.yaml:41:5: AGEU reads AGEUNIT, which no input or earlier column declares [unknown_reference, COLUMN-5]
```

```text
yamaa run of ADAE: succeeded, 1234 rows x 7 columns, 0 issues
```

```text
yamaa project run of project.yaml, section adam: failed, 3 specifications, 2 issues
  ADSL   succeeded  254 rows x 48 columns, 0 issues
  ADAE   failed     no output, 1 issue
  ADTTE  skipped    no output, 1 issue
first issue: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]
```

```text
>>> r["ADTTE"]
yamaa run of ADTTE: skipped, no output, 1 issue
first issue: adam/adtte.yaml:9:5: ADTTE did not run because its producer ADAE did not complete [producer_not_completed, PROJECT-11]
```

```text
yamaa submission of project.yaml, section sdtm: succeeded, 1 define.xml and 12 transport files, 0 issues
```

```text
yamaa selftest: failed, 23 of 24 cases passed
first failure: parquet-artifact: the output bytes differ from the expected bytes
engine 1.0.0, language 1.0, python 3.12.4, linux-x86_64
```

The selftest case count is invented; #3062 fixes the cases. A `yamaa.Table`
prints a header line, a name row, a type row and at most 10 value rows (the
first 5, `...`, the last 5) of at most 8 columns; a cell is cut at 20
characters, missing prints `null`, and dates print in VALUE-17's text:

```text
yamaa.Table: 3 rows x 6 columns
STUDYID  DOMAIN  USUBJID     SUBJID  AGE   AGEU
str      str     str         str     int   str
STUDY01  DM      STUDY01-01  01      34    YEARS
STUDY01  DM      STUDY01-02  02      51    YEARS
STUDY01  DM      STUDY01-03  03      null  null
```

**Error text (D39).** A short prefix from a closed set, the first issue's
line, and the count of the others:

```text
yamaa.Error: cannot save ADAE: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5] (and 2 more issues)
```

The prefixes are `cannot save <subject>`, `cannot resolve <path>`, `cannot
select <name>`, `cannot read <path>`, `cannot lint <path>` and `cannot give
the schema of <type>`; the prefix says which call refused, which the first
message alone does not when a failed run's `save()` refuses with the run's own
issues.

**What freezes at 1.0.0:** the labels, order and separators of every form
in rule 2 (the run, project run and submission line, the check line, the
selftest line with its `first failure:` and information lines, and the lazy
spec and project line), the `first issue:` layout, the member-line layout and
the `yamaa.Table:` header. Message text and the table preview's cells may
improve in any release.

## `lint()` (D40, P-LINT-2)

| Option | Columns |
| --- | --- |
| A | D40's five: `file`, `line`, `column`, `finding`, `message` |
| **B** | **A, plus `spec_path`** |
| C | B, plus `fixable` or `severity` |

| Column | Type | Meaning |
| --- | --- | --- |
| `file` | `str` | absolute path, forward slashes |
| `line` | `int` | one-based |
| `column` | `int` | one-based, in Unicode scalar values (sources are ASCII) |
| `finding` | `str` | one name from the closed list below |
| `spec_path` | `str` | the spec path of the departure, `$` for the root |
| `message` | `str` | rendered in Rust; not frozen |

- Rows come in argument order of the files (a directory's files in sorted
  path order), then by line, column and finding.
- `paths` is a path or a list of paths, a character vector in R, one or more
  PATH arguments on the command line. A directory expands to the `*.yaml`
  files whose first keys are the 1.0 header, so every file type is linted.
- `fix = TRUE` applies the proved layout fixes: same parsed value, same
  comments, unchanged by a second pass; each changed file is replaced
  atomically, and the table lists what remains. These user-named files are the
  one write outside declared outputs.
- A file that cannot be read or parsed refuses the call with the issue that
  says why, so the command line exits 1.
- No `select` or `ignore` arguments in 1.0; the suppression comment covers
  single sites, spelled `# yamaa-lint: allow <finding> -- <reason>` (P-LINT-1).
- Findings: `field_order`, `root_spacing`, `entry_spacing`, `blank_lines`,
  `line_width`, `invalid_suppression`, `repeated_row_derivation`,
  `redundant_intermediate`, `missing_label`, and the proposed
  `redundant_long_form` (P-LONG-1), `restated_default` (P-HND-3),
  `scalar_as_text` (a whole-field literal written as grammar text),
  `unused_input_key` (P-JOIN-1) and `equality_filter` (P-ORD-3).
  `literal_form` retires with `{literal: X}`.
- Command-line text keeps the style tool's line, `file:line:column: finding
  message`, followed by `N lint findings in M of K files`.

## `info()` (D41, P-INFO-1)

| Field | Type | Example | Meaning |
| --- | --- | --- | --- |
| `version` | `str` | `1.0.0` | the engine version, identical in every host; a candidate reads `1.0.0-rc.1` everywhere |
| `language_versions` | `list<str>` | `["1.0"]` | every `yamaa:` version the engine reads, oldest first |
| `features` | `list<str>` | `["odm", "parquet", "readstat", "yaml"]` | compiled optional features, sorted |
| `host` | `str` | `python` | `python`, `r` or `cli` |
| `host_version` | `optional<str>` | `3.12.4` | the Python or R version; missing for `cli` |
| `platform` | `str` | `linux-x86_64` | `<os>-<arch>` of the compiled engine |

Under D53 the R package stays `1.0.0` during candidates while Python and
Cargo carry the candidate, so the host package version would print three
versions for one engine (against I7); `version` is the engine's. Features are
the Cargo names `docs/release.md` and the bindings handshake already use. The
platforms of D49 read `linux-x86_64`, `linux-aarch64`, `windows-x86_64` and
`macos-aarch64`; a target triple would separate musl from glibc builds, which
users do not need. `info()` returns plain data: a Python dict, an R named
list, a text block or JSON object on the command line. No build time or
commit (the same source gives the same bytes), no toolchain, and no OS release,
which belongs in the selftest report. `__version__` and
`packageVersion("yamaa")` stay the host package versions.

```text
$ yamaa --version
yamaa 1.0.0
language versions: 1.0
features: odm, parquet, readstat, yaml
host: cli
platform: linux-x86_64
```

## Other surface details

| Item | Proposal | Reason |
| --- | --- | --- |
| `p.names` (D45) | the listed spec names in run order, read without data; refuses when the project cannot be read or lists a name twice | mirrors `ProjectRun.names` |
| `save()` paths (D38) | absolute, forward slashes, in publication order: a run's report before its primary file (OUTPUT-10), a section's transport files before its define.xml (JSON-14) | one spelling in every host (I8) |
| `Error.paths` (new, Q11) | `yamaa.Error.paths` and the R condition's `paths`: the files written before the refusal, possibly none | a failed run's `save()` writes the report, then refuses (OUTPUT-10, PROJECT-12) |
| `Submission.created` (new, Q11) | the ISO 8601 creation time the documents carry: the argument as given, or the UTC time read once for every section when it was omitted (D129) | section 2.8 says the result records the time it used |
| `schema(file_type)` | the JSON Schema as canonical JSON text | the same bytes as `yamaa schema TYPE`; R needs no JSON library |
| `selftest()` | a SelfTest result with `status`, `cases` (`case`, `status`, `message`) and `info` | #3062 owns the case set |
| Check | a native handle with `status`, `issues` and its summary | status and printing come from Rust |
| R `names()`, `length()` (D44) | a ProjectRun's names are its spec names; every other result lists its public members | `names()` of an environment would list its hidden handle |
| `yamaa.Table` (D11, D43) | `num_rows`, `column_names`, `len()`, `__arrow_c_stream__`, `to_polars()`, `to_pandas()`, `to_arrow()` importing their library only when called, `to_pylist()` importing nothing | replaces the private `_ArrowTable` |
| Member lookup | `r["X"]` for a name the run lacks refuses with `unknown_specification_name` (PROJECT-6) | D39's error text needs an issue |
| Python exports | `Error`, `Table`, `__version__`, `info`, `lint`, `project`, `read_odm`, `schema`, `selftest`, `spec` | result classes stay importable outside `__all__`, as today |
| R exports | `yamaa_info`, `yamaa_lint`, `yamaa_project`, `yamaa_read_odm`, `yamaa_schema`, `yamaa_selftest`, `yamaa_spec` | the D10 names plus the new functions |

## The command line (D12, P-CLI-1)

| Command | Synopsis | Host equivalent |
| --- | --- | --- |
| `check` | `yamaa check PATH [--spec NAME]` | `s.check()`, `p.check()`, `p.spec(name).check()` |
| `run` | `yamaa run PATH [--spec NAME \| --section SECTION]` | `run()`, then `save()`; prints the written paths |
| `submission` | `yamaa submission PATH [--section SECTION] [--created TIME]` | `p.submission(section, created=...)`, then `save()` |
| `resolve` | `yamaa resolve PATH [--spec NAME]` | `s.resolve()`; prints canonical JSON |
| `read-odm` | `yamaa read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] [--max-expanded-bytes N] [--max-xml-depth N]` | `read_odm()`, then writing OUT, whose extension selects CSV or Parquet |
| `lint` | `yamaa lint [--fix] PATH...` | `lint(paths, fix)` |
| `schema` | `yamaa schema TYPE` | `schema(file_type)` |
| `selftest` | `yamaa selftest` | `selftest()` |
| flag | `yamaa --version` | `info()` |

Each command dispatches on the file's `type:` header. Two refinements of
#3030's table need approval (Q10): `--spec NAME`, without which the command
line cannot check, run or resolve one spec of a project (and a spec checked
alone fails for its project bindings under D32); and `submission` taking its
section as `--section`, since the host argument is optional.

Global flags: `--format text|json` (default `text`, before or after the
command), `--help` and `--version`. No configuration file, environment
variable or colour. Standard output carries the command's output, or with
`--format json` exactly one canonical JSON document (RFC 8785, one line) and
a LF; standard error carries only usage errors. One exception (D142): a
panic inside a `selftest` case fails that case, so the command exits 1, and
Rust's default panic hook writes the panic message, which shows where the
engine panicked, to standard error.

| Exit status | Meaning |
| --- | --- |
| 0 | the command completed and its status is `succeeded`; for `resolve`, `schema` and `--version`, the output was printed |
| 1 | the command completed with findings: a `failed` or `skipped` status, remaining lint findings, a failed selftest case, or a refusal that carries issues; a PATH that is missing, not a regular file or a link is an issue, `resource_path_missing`, `resource_path_not_regular_file` or `resource_path_symlink` (FILE-7, FILE-8; D125, D188), as the hosts return it |
| 2 | the command could not run as asked: invalid usage or arguments, a PATH whose type the command does not take, `--spec` or `--section` with a spec PATH, or a failed write to standard output |

D12 says 2 is for "usage or IO errors". This design reads "IO errors" as the
command line's own input and output. A file problem the engine reports as an
issue, such as a missing input or a failed publication, exits 1, because the
hosts report the same issue and I7 asks for one behavior; the other reading,
2 for every file-system problem, makes the status depend on which conditions
count as IO. Ctrl+C publishes nothing (D46) and ends the process by the
signal, which POSIX shells report as 130.

```text
$ yamaa run project.yaml --section adam
yamaa project run of project.yaml, section adam: failed, 3 specifications, 2 issues
  ADSL   succeeded  254 rows x 48 columns, 0 issues
  ADAE   failed     no output, 1 issue
  ADTTE  skipped    no output, 1 issue
adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]
adam/adtte.yaml:9:5: ADTTE did not run because its producer ADAE did not complete [producer_not_completed, PROJECT-11]
/study/adam/adsl-report.csv
/study/adam/adae-report.csv
$ echo $?
1
```

The command line prints every issue, since it leaves no table to inspect.
Under PROJECT-12 a failed project run saves each run spec's report and no
primary file. The same run with `--format json`, pretty-printed here and
shortened to one issue:

```json
{
  "command": "run",
  "issues": [
    {"column": 9, "condition": "forward_reference",
     "context": {"column": "ASTDY", "dependency": "TRTSDT"},
     "file": "/study/adam/adae.yaml", "line": 57,
     "message": "column ASTDY reads TRTSDT, which is declared after it",
     "operand_route": null, "phase": "validation", "rule": "COLUMN-5",
     "severity": "error", "source_span": null,
     "spec_paths": ["columns.ASTDY.derivation"]}
  ],
  "members": [
    {"columns": 48, "issue_count": 0, "name": "ADSL", "rows": 254, "status": "succeeded"},
    {"columns": null, "issue_count": 1, "name": "ADAE", "rows": null, "status": "failed"},
    {"columns": null, "issue_count": 1, "name": "ADTTE", "rows": null, "status": "skipped"}
  ],
  "path": "project.yaml",
  "paths": ["/study/adam/adsl-report.csv", "/study/adam/adae-report.csv"],
  "section": "adam",
  "spec": null,
  "status": "failed"
}
```

In JSON an issue's `context` is an object, not text. Each command's keys are
listed in `rust/interface/bindings.yaml`'s `cli.commands[].json_keys`, with
`refusal_keys` where a refusal's keys differ; they freeze like table columns.

## The issues table (D28)

| # | Column | Type | Missing when | Python | R |
| --- | --- | --- | --- | --- | --- |
| 1 | `severity` | `str` | never | Utf8 | character |
| 2 | `phase` | `str` | never | Utf8 | character |
| 3 | `condition` | `str` | never | Utf8 | character |
| 4 | `rule` | `str` | `engine_rejected`, `unsupported_operation` | Utf8 | character, `NA` |
| 5 | `spec_paths` | `list<str>` | never; may be empty | List<Utf8> | list of character |
| 6 | `file` | `str` | no YAML file located | Utf8 | character, `NA` |
| 7 | `line` | `int` | as `file`, or no node located | Int64 | double, `NA` |
| 8 | `column` | `int` | as `line` | Int64 | double, `NA` |
| 9 | `message` | `str` | never | Utf8 | character |
| 10 | `context` | `str` | never | Utf8, canonical JSON | character, canonical JSON |
| 11 | `source_span` | `list<int>` | no span recorded | List<Int64> | `NULL` or two doubles |
| 12 | `operand_route` | `list<str>` | no route recorded | List<Utf8> | `NULL` or character |

`severity` comes from the registry, and every 1.0 condition is `error`.
`rule` is missing only for the two reviewed exceptions in the normative null
block of `rules/columns.md`. `context` stays canonical JSON text, so hosts
need no JSON library. Paths use forward slashes.

**`file`, `line` and `column` (P-ISSUE-1).**

| Option | Location | Assessment |
| --- | --- | --- |
| **A** | **The layer that wrote the deepest written node of the first spec path, the writer whose value took effect** | **Points where the user must edit** |
| B | Always the entry file, with a position only when the node is in it | Hides every error made in a parent layer |
| C | The root ancestor that first declared the member | Can point at a value a child overrode |

1. Take the first entry of `spec_paths`. With none, the three columns are
   missing, except for `invalid_yaml` and `non_ascii_source`, which give the
   file read and the parser's position, as their context does today.
2. Walk the path through the resolved specification's provenance to the
   deepest node a layer wrote; a path into an expanded long form stops at the
   written node.
3. Among the layers that wrote it, take the one whose value the resolved
   specification holds, the latest in contribution order (LAYER-3).
4. A path starting in the project file locates there, or in the listed file it
   continues into (`sdtm.specs[0].columns.X.label`).
5. `file` is that layer's absolute path with forward slashes; `line` and
   `column` are one-based, at the key of a mapping member or the first
   character of a sequence item.
6. A position never points into a data file; record positions stay in
   `context`. `source_span` stays byte offsets into the expression text.

The YAML decoder already keeps a source position for every node
(`DecodedYaml.locations` in `rust/crates/yamaa-adapters/src/yaml_decode.rs`).

**Messages (P-MSG-1).**

| Option | Source | Assessment |
| --- | --- | --- |
| **A** | **One template per condition in `yaml/conditions.yaml`** | **The language-neutral registry D28 names** |
| B | One template per cause in the core registry | 374 texts in Rust |
| C | The card title with the context appended | A title states a rule, not what went wrong |
| D | A, with per-cause overrides in Rust | Possible later |

```yaml
conditions:
  forward_reference:
    phases: [validation]
    message: "column {column} reads {dependency}, which is declared after it"
```

A placeholder names a context key, `{spec_path}` is the first spec path, and
`{{` and `}}` are literal braces; text renders as written, numbers in
canonical text, Booleans as `true` and `false`, missing as `null`, lists
joined with `, `, objects as canonical JSON, and there are no conditionals.
Every cause of a condition must supply each placeholder; the Rust tests that
reach every registered cause render every message. A message is one clause,
starts in lower case, has no final period, is ASCII, and names no location.
202 conditions have 374 causes; 48 conditions are shared by 221 causes, so a
shared condition uses only common keys. The templates are written in Phase 3
(#2845), and `check_diagnostics.py` drops its one-line registration pattern
in the same change.

## The bindings draft (Phase 0 item 3)

`tools/packaging/generate_bindings.py` is the only reader of
`rust/interface/bindings.yaml`. Its grammar is closed: it refuses unknown
top-level keys, requires `format_version` 1, and reconciles every declared
class, method, function and exception with the handwritten `#[pyclass]`,
`#[pymethods]`, `#[pyfunction]`, `create_exception!` and `#[extendr]` items.
`make check-repository`, CI's repository job and `tools/tests/test_bindings.py`
run it.

| Option | Where | Assessment |
| --- | --- | --- |
| A | In `bindings.yaml` now | Fails: the new top-level keys are refused, and the 1.0 exports have no handwritten registration |
| **B** | **A separate file, `rust/interface/bindings-1.0.yaml`** | **Only the generic YAML, ASCII and citation checks read it; no package stages `rust/interface/`** |
| C | Prose in the RFC only | Loses the machine-checkable pairing |

A YAML file under `docs/` would also be published as a site asset. Phase 3
merges the draft into `bindings.yaml` as format 2 in the pull requests that
implement each host (#2845 for Python and R, #3060 for the command line),
extends the generator, and deletes the draft. From then on the stability check
reads `bindings.yaml`.

| Section | Format 1 | Format 2 draft | Frozen by 2.11 |
| --- | --- | --- | --- |
| `values` | none | status, severity, phase, file type, section, host, feature, lint finding, report kind and outcome, output format | yes |
| `tables` | none | issues (12), report (14), lint (6), odm_items (30), selftest_cases (3) | yes |
| `records` | none | info (6 fields) | yes |
| `public` | none | type projections, functions, classes and members, errors, package metadata and exports | yes |
| `cli` | none | synopsis, global flags, streams, exit statuses, commands, JSON keys | yes |
| `python`, `r` | native registrations | the same shape, plus getters | no, private |
| `operations` | Python and R | adds the command a pairing reaches | no |
| `host_only` | Python and R | adds `cli`; `yamaa.Table`'s members are Python-only | no |

The draft declares 7 public functions (4 new: `lint`, `schema`, `info`,
`selftest`), 9 classes with 63 members (37 new), 8 commands, and natively 55
Python exports and methods, 48 R functions and 46 paired operations. A
consistency check (described on the [method page](method.md)) confirms that
every native export is paired or host-only, every operation names existing
exports, the issues and report columns equal 2.7 and CHECK-24, and every value
vocabulary a field names exists.
