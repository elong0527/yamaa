# R and Python API

yamaa has the same small API in R and Python. Use it to create, verify and save
SDTM and ADaM datasets and define.xml files.

## Quick reference

| Task | R | Python |
|---|---|---|
| Build one dataset | `yamaa_spec(path)$run()$save()` | `yamaa.spec(path).run().save()` |
| Build one project dataset | `project$spec("DM")$run()$save()` | `project.spec("DM").run().save()` |
| A project's specifications | `project$names` | `project.names` |
| Build every SDTM dataset | `project$run("sdtm")$save()` | `project.run("sdtm").save()` |
| Build every ADaM dataset | `project$run("adam")$save()` | `project.run("adam").save()` |
| One dataset of a project run | `runs[["ADSL"]]$output` | `runs["ADSL"].output` |
| Check without data | `spec$check()$issues` | `spec.check().issues` |
| Did it succeed? | `run$status` | `run.status` |
| See the resolved specification | `spec$resolve()` | `spec.resolve()` |
| Run report | `run$report` | `run.report` |
| A table in Polars or pandas | `run$output` is a data frame | `run.output.to_polars()`, `run.output.to_pandas()` |
| define.xml and transport files | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
| Lint specifications | `yamaa_lint(paths)` | `yamaa.lint(paths)` |
| JSON Schema of a file type | `yamaa_schema("spec")` | `yamaa.schema("spec")` |
| Check an installation | `yamaa_selftest()` | `yamaa.selftest()` |
| Engine version and platform | `yamaa_info()` | `yamaa.info()` |

Examples in R and Python are shown in tabs, R first; pick a language once and
every tab on the page follows. Both packages run the same Rust engine, so the
same files give the same results.

## Input files

A **specification** (`spec.yaml`) describes one dataset: its inputs, columns,
checks and `output`.

A **project** (`project.yaml`) describes a study. It lists the specifications
of each standard, the codelists they share, the submission metadata, and the
environment file for R or Python.

> We recommend using AI to help you create these files. Human review and
> sign-off are necessary steps.

## Dataset specification

For example, `sdtm/dm.yaml` builds SDTM DM, one row per subject, from the
study's ODM XML export ([Read ODM XML](#read-odm-xml)).

```yaml
# sdtm/dm.yaml
yamaa: "1.0"
type: spec
domain: DM
keys: [STUDYID, USUBJID]
input:
  ODM: ../edc/odm.xml

output:
  path: dm.parquet
  columns: [STUDYID, DOMAIN, USUBJID, SUBJID, AGE, AGEU]
  report: dm-report.parquet

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: ODM.StudyOID

  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: "'DM'"

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: {str_template: "{ODM.StudyOID}-{ODM.SubjectKey}"}

  - name: SUBJID
    type: str
    label: Subject Identifier for the Study
    derivation: ODM.SubjectKey

  # An age outside 18 to 85 is listed in the report and does not stop the run.
  - name: AGE
    type: int
    label: Age
    derivation: {odm: ODM.IT.DM.AGE}
    verifications:
      - range: {min: 18, max: 85, severity: warning}

  - name: AGEU
    type: str
    label: Age Units
    derivation:
      case:
        - when: "AGE IS NOT NULL"
          then: "'YEARS'"
        - otherwise: null
```

* `yamaa_spec()` and `yamaa.spec()` point to a specification file and read
  nothing yet.
* `run()` builds the dataset in memory. The first `run()` compiles the
  specification and keeps it. A later `run()` of the same object reuses it if
  its paths still name the same files, even after a change of working
  directory, and every file it was compiled from is unchanged. Otherwise it
  compiles again. Every `run()` reads the input data afresh. A copied,
  pickled or restored object starts with nothing kept.
* `save()` writes the run report to `output.report`, then the dataset to
  `output.path`, and returns the absolute paths it wrote, in that order (R
  returns them invisibly). A failed run writes only its run report, then
  raises `yamaa.Error` (in R, signals `yamaa_error`) with the run's issues;
  the error's `paths` lists what the save wrote first.

=== "R"

    ```r
    spec <- yamaa_spec("sdtm/dm.yaml")
    run <- spec$run()
    run$status    # "succeeded" or "failed"
    run$output    # generated dataset
    run$issues    # identified errors
    run$report    # run report
    run$save()    # save run report and dataset; returns their paths
    ```

=== "Python"

    ```python
    spec = yamaa.spec("sdtm/dm.yaml")
    run = spec.run()
    run.status    # "succeeded" or "failed"
    run.output    # generated dataset
    run.issues    # identified errors
    run.report    # run report
    run.save()    # save run report and dataset; returns their paths
    ```

## Tables

Every table that yamaa returns is a data frame in R. In Python it is a
`yamaa.Table`, which needs no other package: printing it shows its column
names and types and its first and last rows, `len()` and `num_rows` count its
rows, `column_names` names its columns, and `to_pylist()` gives one dict per
row. `to_polars()`, `to_pandas()` and `to_arrow()` convert it to the table
library you use, which must be installed: Polars, pandas with PyArrow, or
PyArrow. A library that reads the Arrow PyCapsule interface, such as
`polars.DataFrame(table)` or `pyarrow.table(table)`, reads it directly.

| Column type | R | Python value | Arrow |
|---|---|---|---|
| `str` | character | `str` | Utf8 |
| `int` | double; an error beyond 2^53 | `int` | Int64 |
| `float` | double | `float` | Float64 |
| `date` | `Date` | `datetime.date` | Date32 |
| `datetime` | `POSIXct` in UTC | `datetime.datetime`, without a time zone | Timestamp, in seconds |
| list, in `issues` | list column | `list` | List |

A missing value is `NA` in R, `NULL` for a missing list, and `None` in
Python. `to_pandas()` keeps each column's Arrow type as a `pandas.ArrowDtype`,
so an integer column with a missing value keeps its exact integers.

=== "R"

    ```r
    dm <- run$output
    nrow(dm)
    ```

=== "Python"

    ```python
    dm = run.output              # prints a preview
    dm.num_rows
    rows = dm.to_pylist()        # [{"STUDYID": "STUDY01", ...}, ...]
    frame = dm.to_polars()       # or dm.to_pandas(), dm.to_arrow()
    ```

## Check a specification without data

`check()` reports problems in a specification without reading study data or
running project code. It does not open the input datasets, so some problems, such
as a column name that no input has, appear only when you call `run()`.

Each problem is one row of `issues`: its `severity`, `phase` and
`condition`, the `rule` it breaks, the `spec_paths` it concerns, a `message`
to read, and its facts in `context` as JSON text. Its `file`, `line` and
`column` say where to edit: the YAML file that wrote the first spec path,
which is the parent layer when the specification inherits that value, and
the line and column of that entry. A problem with no place in a YAML file,
such as a project section that does not exist, leaves them missing. A call
that cannot complete, such as a `save()` of a failed run, raises
`yamaa.Error` in Python or signals a `yamaa_error` condition in R. Its
message names the call and the first issue with its location, such as
`cannot save ADAE: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is
declared after it [forward_reference, COLUMN-5] (and 2 more issues)`, and its
`issues` hold every row.

=== "R"

    ```r
    yamaa_spec("sdtm/dm.yaml")$check()$issues
    ```

=== "Python"

    ```python
    yamaa.spec("sdtm/dm.yaml").check().issues
    ```

## Status and printed results

Every result has a `status`. A check succeeds when it finds no issue, and a
run when it builds its dataset with no issue. A project run succeeds when
every specification in it succeeds, and a submission when every section
composes with no issue; anything else is `failed`. A specification of a
project run that did not run, because a specification it reads from did not
complete, is `skipped`.

Printing a result shows its status, its size, its issue count and its first
issue with its location. The engine writes the text, so R, Python and the
command line print the same words. A project run lists each specification on
a line of its own, and one of its runs prints like any run:

=== "R"

    ```r
    runs <- yamaa_project("project.yaml")$run("adam")
    runs$status        # "failed"
    runs               # the summary below
    runs[["ADTTE"]]
    ```

=== "Python"

    ```python
    runs = yamaa.project("project.yaml").run("adam")
    runs.status        # "failed"
    print(runs)        # the summary below
    print(runs["ADTTE"])
    ```

```text
yamaa project run of project.yaml, section adam: failed, 3 specifications, 2 issues
  ADSL   succeeded  254 rows x 48 columns, 0 issues
  ADAE   failed     no output, 1 issue
  ADTTE  skipped    no output, 1 issue
first issue: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]
yamaa run of ADTTE: skipped, no output, 1 issue
first issue: adam/adtte.yaml:9:5: this specification did not run because the producer of its input ADAE did not complete [producer_not_completed, PROJECT-11]
```

`yamaa_spec()`, `yamaa.spec()` and the project functions read nothing, so
their objects print only the path you gave, such as `yamaa spec
sdtm/dm.yaml`. In R, `format()` returns the printed lines, and `names()` of a
result lists its fields and methods, starting with `status`, as
`names(run)` gives `"status" "output" "issues" "report" "save"`; `length()`
counts them.

## See the resolved specification

`resolve()` returns the specification after its layers are composed: the
complete normalized model, where each field came from, and where each resource
is. A producing specification linked through `schema` is a resource of its
consumer, not a separate entry. R returns a named list and Python a dict. It
reads no study data and runs no project code.

Each entry names the language its specification is written in: a 1.0
specification's entry records `language_version`, the newest version any of
its layers declares in `yamaa`, and a 0.5 specification's entry records its
`schema_version`.

=== "R"

    ```r
    resolved <- yamaa_spec("adam/adsl.yaml")$resolve()
    names(resolved)
    ```

=== "Python"

    ```python
    resolved = yamaa.spec("adam/adsl.yaml").resolve()
    resolved.keys()
    ```

## Project configuration

The R and Python project files differ only in `environments`.

=== "R"

    ```yaml
    # project.yaml
    schema_version: "1.0"
    environments: {r: r/environment.yaml}
    study:
      id: STUDY01
      name: STUDY01
      description: "An example study"
      protocol_name: STUDY01
    codelists: [terminology/ct.yaml]
    sdtm:
      specs: [sdtm/dm.yaml, sdtm/ae.yaml]
      standard: {name: SDTMIG, version: "3.4", status: Final}
      define: sdtm/define.xml
    adam:
      specs: [adam/adsl.yaml, adam/adae.yaml]
      standard: {name: ADaMIG, version: "1.3", status: Final}
      define: adam/define.xml
    ```

=== "Python"

    ```yaml
    # project.yaml
    schema_version: "1.0"
    environments: {python: python/environment.yaml}
    study:
      id: STUDY01
      name: STUDY01
      description: "An example study"
      protocol_name: STUDY01
    codelists: [terminology/ct.yaml]
    sdtm:
      specs: [sdtm/dm.yaml, sdtm/ae.yaml]
      standard: {name: SDTMIG, version: "3.4", status: Final}
      define: sdtm/define.xml
    adam:
      specs: [adam/adsl.yaml, adam/adae.yaml]
      standard: {name: ADaMIG, version: "1.3", status: Final}
      define: adam/define.xml
    ```

An **environment** (`environment.yaml`) names the package lock file and the
project-specific functions the specifications call. By design, one language
runs the whole study: each package reads only the environment of its own
language.

=== "R"

    ```yaml
    # r/environment.yaml
    schema_version: "1.0"
    lock: ../renv.lock  # environment lock file
    functions:
      bmi: functions/bmi.yaml  # project-specific function definition
    ```

=== "Python"

    ```yaml
    # python/environment.yaml
    schema_version: "1.0"
    lock: ../uv.lock  # environment lock file
    functions:
      bmi: functions/bmi.yaml  # project-specific function definition
    ```

## Work with a project

`yamaa_project()` and `yamaa.project()` read nothing until you call a method.
`spec(name)` returns one of the project's specifications by its `domain` name,
with the project's codelists and the functions of your language's environment.
Open a specification this way when it binds a codelist or an inventory
vocabulary, or calls a project function: a 1.0 specification that does so,
checked or run without its project, fails before any data is read with
`project_binding_missing` at each such binding.

=== "R"

    ```r
    project <- yamaa_project("project.yaml")
    project$names             # every specification, in run order
    adae <- project$spec("ADAE")
    adae$run()$save()
    project$check()$issues    # every specification, no data read
    ```

=== "Python"

    ```python
    project = yamaa.project("project.yaml")
    project.names             # every specification, in run order
    adae = project.spec("ADAE")
    adae.run().save()
    project.check().issues    # every specification, no data read
    ```

Only specifications listed in the project have names, and each name is unique
across the project. An unknown name raises an error that lists the names the
project has. `names` lists them in the order a run of every section builds
them, each specification after the ones it reads from, and reads no study
data; it raises when the project file cannot be read or lists a name twice.

The project's `check()` checks every listed specification as `spec(name)`
would. It also reports two listings that share a name, specifications that
would write the same file, and an input that reads another listed
specification's dataset without linking it through `schema`.

## Project execution

`run()` builds every specification in the section you name, `"sdtm"`, `"adam"`
or `"send"`, in dependency order. Without a name, it builds every section.
Naming a section the project does not declare, in `run()` or `submission()`,
raises an error that lists the sections it declares. As with `spec()`, a later
`run()` reuses what earlier runs of the same project object compiled, and
compiles again any specification whose files changed.
Its `save()` writes each specification's run report and dataset in run order,
or only the run reports when any specification failed, and returns the paths it
wrote. It checks every target before it writes anything.

=== "R"

    ```r
    project$run("sdtm")$save()    # all SDTM datasets
    project$run("adam")$save()    # all ADaM datasets
    runs <- project$run()         # all SDTM and ADaM datasets
    runs$names                    # the specifications, in run order
    length(runs)                  # how many; names(runs) lists them too
    runs[["ADSL"]]$output         # one specification's dataset
    runs$report                   # all run reports, in run order
    runs$save()                   # save every dataset and run report
    ```

=== "Python"

    ```python
    project.run("sdtm").save()    # all SDTM datasets
    project.run("adam").save()    # all ADaM datasets
    runs = project.run()          # all SDTM and ADaM datasets
    runs.names                    # the specifications, in run order
    len(runs)                     # how many
    runs["ADSL"].output           # one specification's dataset
    runs.report                   # all run reports, in run order
    runs.save()                   # save every dataset and run report
    ```

## Create define.xml and submission datasets

`submission()` composes one define.xml for the selected section, or one for
every section that names `define`. Each specification it covers needs
`submission` metadata, as the
[sdtm-dm-metadata](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-metadata.html)
benchmark shows. With `datasets: json` or `datasets: xpt` in a section, it also
writes each saved dataset as one Dataset-JSON or SAS Transport version 5 file
next to define.xml.
Its `save()` writes each section's transport files, then its define.xml, and
returns the paths it wrote. Its `created` is the creation time that the
documents carry: the `created` you gave, or else the time the submission read
once and gave every section.

=== "R"

    ```r
    project$run("sdtm")$save()
    submission <- project$submission("sdtm")
    submission$issues
    submission$save()
    project$submission()$save()
    ```

=== "Python"

    ```python
    project.run("sdtm").save()
    submission = project.submission("sdtm")
    submission.issues
    submission.save()
    project.submission().save()
    ```

## Read ODM XML

A 1.0 specification reads an ODM 1.3 or 2.0 XML file directly: an input whose
path ends in `.xml`, such as `ODM: ../edc/odm.xml` in `sdtm/dm.yaml`, reads one
row per clinical item in file order. Only a snapshot reads: a transactional
file, and one without clinical data such as a define.xml, are refused with
their issue, and audit records, signatures and annotations are skipped
([ODM-1](../../rules/reference/odm.md#odm-1)).

`read_odm()` reads the same table in R or Python. Use it for an export inside
a TAR archive, which an input does not open, or to look at the items
yourself.

=== "R"

    ```r
    odm <- yamaa_read_odm("edc/export.tar.gz", archive_member = "odm.xml")
    ```

=== "Python"

    ```python
    odm = yamaa.read_odm("edc/export.tar.gz", archive_member="odm.xml")
    ```

## Lint specifications

`lint()` checks yamaa files against the [style guide](style.md) and returns
one row per finding: the file, the line and column where the departure
starts, the finding, its spec path and a message. A directory names the
yamaa files beneath it, of every file type. With `fix`, it first applies
the proved fixes, replacing a file only when it reads as the same values
with the same comments, or as those values with exactly the canonical
rewrite a finding names, and returns what remains.

=== "R"

    ```r
    findings <- yamaa_lint(c("sdtm/dm.yaml", "adam/"))
    remaining <- yamaa_lint("sdtm/dm.yaml", fix = TRUE)
    ```

=== "Python"

    ```python
    findings = yamaa.lint(["sdtm/dm.yaml", "adam/"])
    remaining = yamaa.lint("sdtm/dm.yaml", fix=True)
    ```

## JSON Schema of a file type

`schema()` returns the JSON Schema of a file type, `spec`, `project`,
`codelists` or `function`, as canonical JSON text: one line, with its members
in key order. Save it, and an editor that validates YAML against a JSON Schema
checks and completes yamaa files as you type. The schema is part of the
installed engine, so every installation of one release gives the same text. A
specification file may be one layer of an inheritance chain, so the `spec`
schema checks what one layer may write. Any other file type raises
`yamaa.Error` in Python or signals a `yamaa_error` condition in R, whose issue
names the condition `unexpected_file_type`.

=== "R"

    ```r
    writeLines(yamaa_schema("spec"), "spec.schema.json")
    ```

=== "Python"

    ```python
    from pathlib import Path

    Path("spec.schema.json").write_text(yamaa.schema("spec") + "\n")
    ```

## Check an installation

`selftest()` runs the cases that the package carries through the installed
engine and compares each result with its independently authored expected
files; it writes no file and uses no network. `status` is `failed` when a case
failed, `cases` has one row per case, and `info` names the engine, its
language versions and features, the host and the platform. Printing the
result gives its report, the installation and operational qualification
evidence of the installation:

=== "R"

    ```r
    yamaa_selftest()
    ```

=== "Python"

    ```python
    print(yamaa.selftest())
    ```

```text
yamaa selftest: succeeded, 5 of 5 cases passed
engine 1.0.0, language 1.0, python 3.12.4, linux-x86_64
```

## Engine information

`info()` returns what the installed engine is and where it runs: its
`version`, the `language_versions` it reads, the optional `features` it was
built with, the `host`, the host's own `host_version`, and the `platform`,
such as `linux-x86_64`. R returns a named list and Python a dict, in that
order. A selftest's `info` holds the same facts, and the command line prints
them for `yamaa --version`. The engine version is the same in every host;
`packageVersion("yamaa")` and `yamaa.__version__` stay the package versions.

=== "R"

    ```r
    yamaa_info()$version
    ```

=== "Python"

    ```python
    yamaa.info()["version"]
    ```

## Inputs, outputs and paths

Inputs can be CSV, Parquet, SAS datasets (`.sas7bdat`), SAS transport v5/v8
files (`.xpt`) or, in a 1.0 specification, ODM XML files (`.xml`). A SAS
input declares its `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int}}`. Outputs are
CSV or Parquet; Dataset-JSON and SAS Transport version 5 files come from
`submission()`.

A relative path resolves from the YAML file that writes it, so
`../edc/odm.xml` in `sdtm/dm.yaml` is the study's `edc/odm.xml`.

On Windows, a specification, project or ODM file you pass may sit on a network
share (`\\server\share\study\project.yaml`) or a mapped drive, and the paths
its YAML writes resolve on that share. Paths inside YAML keep forward slashes.
Below the directory you pass, symbolic links, junctions and other reparse
points are refused on every drive. On a share, a link is refused when the file
server reports it as one, as Windows servers do. A server that follows links
itself, as Samba does with its default `follow symlinks = yes`, shows the
link's target instead, and that target is read like any other file.

A parent specification's relative input and output paths resolve beside the
parent, even when it sits on another drive than the specification you pass. A
parent on a network share that the specification you pass is not on cannot
declare relative input or output paths, because a path in YAML cannot name a
share: such a parent is refused with `parent_on_another_share`, naming the
parent, the path and both roots. Keep it on the same share, or write those
paths with `relative_to: entry`.

While a save replaces a file, it locks the file's folder, and another save on
the same computer that reaches that folder meanwhile fails rather than waits.
The lock does not reach other computers: saves from two computers into one
network folder are not serialized, so save into a shared folder from one
computer at a time.

## Command line

The `yamaa` command line runs the same engine as the R and Python packages,
as one standalone binary that needs no Python, R or Rust. Each command makes
the calls described above and prints the engine's own text, so a result, an
issue or an error reads as it does in R and Python. Only its arguments change
what it does: no configuration file, environment variable or colour does.

```text
yamaa [--format text|json] COMMAND [ARGUMENTS]
yamaa --version [--format text|json]
yamaa --help
```

`--format` selects what a command prints: `text`, the default, or `json`, one
JSON document. It may come before or after the command. `--version` prints
the [engine information](#engine-information) for the host `cli`, one field
per line, or with `--format json` one object with the six keys of `info()`,
whose `host_version` is null. `--help`, given anywhere before `--`, prints the
usage whatever else is given. Both exit 0.

### Commands

```text
yamaa check PATH [--spec NAME]
yamaa run PATH [--spec NAME | --section SECTION]
yamaa submission PATH [--section SECTION] [--created TIME]
yamaa resolve PATH [--spec NAME]
yamaa read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] [--max-expanded-bytes N] [--max-xml-depth N]
yamaa lint [--fix] PATH...
yamaa schema TYPE
yamaa selftest
```

| Command | What it does | In R and Python |
|---|---|---|
| `check` | Checks a specification or a project without reading data | `check()` |
| `run` | Builds a specification or a project, then saves what it built and prints each path written | `run()`, then `save()` |
| `submission` | Composes a project's define.xml and transport files, then saves them and prints each path written | `submission()`, then `save()` |
| `resolve` | Prints the resolved specification as canonical JSON | `resolve()` |
| `read-odm` | Reads SOURCE, an ODM XML file or a TAR archive that holds one, and saves its items to OUT | `read_odm()` |
| `lint` | Lints each PATH, a yamaa file or a directory of them; with `--fix`, first applies the proved fixes in place | `lint()` |
| `schema` | Prints the JSON Schema of TYPE: `spec`, `project`, `codelists` or `function` | `schema()` |
| `selftest` | Runs the cases built into the engine and compares each with its expected files | `selftest()` |

`check`, `run` and `resolve` take a specification or a project, whichever
PATH declares in its `type:`, and `submission` takes a project. For a
project, `--spec NAME` selects one of its specifications, as `spec(name)`
does, and `--section SECTION` one of its sections, `sdtm`, `adam` or `send`;
without them, `check` and `run` cover the whole project, and `submission`
every section that names `define`. `run` takes `--spec` or `--section`, not
both, and `resolve` needs `--spec` for a project, which has no resolved form
of its own. `--created TIME` gives the ISO 8601 creation time that a
submission's documents carry; without it, the submission reads the clock once
for every section and prints the time it read. A command's flags follow it,
before or after its arguments, each at most once, and `--` ends the flags, so
a PATH that starts with `-` can follow it. A PATH whose type the command does
not take, such as a specification with `--spec`, is a usage error.

`read-odm` reads SOURCE as `read_odm()` does, then saves the items to OUT,
whose extension, `.csv` or `.parquet`, selects the format; any other
extension is refused before anything is written. Its flags are
`read_odm()`'s arguments, with the same defaults: `--archive-member NAME`
names the archive member to read, and `--max-archive-members N` (default
10000), `--max-expanded-bytes N` (default 268435456) and `--max-xml-depth N`
(default 64) bound the members an archive holds, the bytes read or expanded,
and the depth of element nesting
([ODM-1](../../rules/reference/odm.md#odm-1)). Each N is a whole number.

The command line runs no project function. It reads no project's
`environments` entry, so it checks no lock, and a run of a specification
that calls a project function fails before it reads any input, with
`unsupported_operation`: run that specification from R or Python. Every
other build runs as it does in R and Python, and `check` checks every
specification, since it runs no project code.

```sh
yamaa check project.yaml
yamaa run sdtm/dm.yaml
yamaa run project.yaml --section sdtm
yamaa --format json run project.yaml --spec DM
yamaa submission project.yaml --section sdtm --created 2026-10-21T12:00:00
yamaa read-odm edc/export.tar.gz edc/odm.parquet --archive-member odm.xml
yamaa lint --fix project.yaml sdtm/ adam/
yamaa schema spec > spec.schema.json
```

### Output

Standard output carries what a command prints, each line ending in a line
feed. As text, `check`, `run` and `submission` print the summary line and
member lines that R and Python print
([Status and printed results](#status-and-printed-results)), then each issue
on a line of its own, as the `first issue:` line shows the first. `run` and
`submission` then print the absolute path of each file the save wrote, in the
order written, and a submission made without `--created` prints
`created: <time>` after its summary line. `resolve` and `schema` print
canonical JSON in either format, and `read-odm` prints the path it wrote.
`lint` prints each finding as `<file>:<line>:<column>: <finding> <message>`,
then a summary line such as `3 lint findings in 2 of 5 files`, to which
`--fix` adds `; fixed 1 file`. `selftest` prints the report that R and Python
print. A call that cannot complete prints the message of the error that R and
Python raise, which carries the first issue, then each other issue's line and
the paths written before it.

With `--format json`, a command prints one canonical JSON document instead:
one line, with its members in key order ([JSON documents](#json-documents)).

Standard error carries only the refusals of exit status 2, each as
`yamaa: <message>` and then `run 'yamaa --help' for usage`, which a failed
write to standard output leaves out. The one exception is a `selftest` case
that panics: the case fails, and the panic message, which shows where the
engine panicked, goes to standard error.

### Exit status

| Status | Meaning |
|---|---|
| 0 | The command completed and succeeded: its status is `succeeded` and any save wrote everything, or `resolve`, `schema`, `--version` or `--help` printed its output. |
| 1 | The command completed with findings: a status of `failed` or `skipped`, a refused save, remaining lint findings, a failed selftest case, or a refusal that carries issues, such as an unknown section, an unknown TYPE or a PATH that cannot be read. |
| 2 | The command could not run as asked: a usage error, an argument the engine cannot take, such as text that is not UTF-8, a PATH whose declared type the command does not take, or a failed write to standard output. |

A file problem the engine reports as an issue, such as a missing input,
exits 1, since R and Python return the same issue.

Press Ctrl+C, and `check`, `run` and `submission` stop at the engine's next
checkpoint, as a call in R or Python does: the command prints nothing, and a
save writes every file it planned or none. The other commands stop at once.
Either way, an interrupted command ends with none of the statuses above.

### JSON documents

With `--format json`, each command prints one object with the keys below, and
`resolve` and `schema` print the JSON they print as text. A refusal prints the
same keys, except where the table names a refusal's.

| Command | Keys |
|---|---|
| `check` | `command`, `path`, `spec`, `status`, `issues` |
| `run` | `command`, `path`, `spec`, `section`, `status`, `members`, `issues`, `paths` |
| `submission` | `command`, `path`, `section`, `created`, `status`, `issues`, `paths` |
| `resolve` | the resolved specification; a refusal: `command`, `path`, `spec`, `status`, `issues` |
| `read-odm` | `command`, `path`, `rows`, `status`, `issues`, `paths` |
| `lint` | `command`, `status`, `findings`, `paths`; a refusal: `command`, `status`, `findings`, `issues`, `paths` |
| `schema` | the JSON Schema; a refusal: `command`, `type`, `status`, `issues` |
| `selftest` | `command`, `status`, `cases`, `info` |
| `--version` | `version`, `language_versions`, `features`, `host`, `host_version`, `platform` |

* `command` is the command's name. `path`, `spec`, `section` and `type` are
  its arguments as given, null for a flag not given; `read-odm`'s `path` is
  SOURCE.
* `status` is the result's status, which a refused save keeps, and `failed`
  for a call refused before it has a result. `lint`'s is `failed` when a
  finding remains, and `selftest`'s when a case failed.
* `issues` holds every issue as an object with the twelve columns of
  `issues`: `severity`, `phase`, `condition`, `rule`, `spec_paths`, `file`,
  `line`, `column`, `message`, `context`, `source_span` and `operand_route`,
  whose `context` is an object rather than JSON text.
* `paths` lists the files written, as the text prints them.
* `members` has one object for each specification of a project run, in run
  order, with its `name`, `status`, `rows`, `columns` and `issue_count`;
  `rows` and `columns` are null without output, and a listing that could not
  be read is named by its path. A specification's own run has none.
* `created` is the creation time that a submission's documents carry; a
  refused submission has none, so it is `--created` as given, or null.
* `rows` is the number of items `read-odm` read, null only when the read is
  refused. Its `status` follows the read, as a run's follows the run: a read
  that completed is `succeeded` even when the save of OUT is then refused,
  which its `issues` and exit status 1 report.
* `findings` has one object for each lint finding, with its `file`, `line`,
  `column`, `finding`, `spec_path` and `message`.
* `cases` has one object for each selftest case, with its `case`, `status`
  and `message`, and `info` holds the engine information with the keys that
  `--version` prints.

From 1.0.0, every 1.x release keeps the commands, flags, exit statuses and
JSON keys on this page, and may add commands, flags and keys, as the
[stability contract](../release.md#stability-contract) says. The messages
are not frozen: a program reads `status`, and an issue's `condition` and
`rule`, never its `message`.
