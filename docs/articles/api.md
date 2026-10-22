# R and Python API

yamaa has the same small API in R and Python. Use it to create, verify and save
SDTM and ADaM datasets and define.xml files. Both packages run the same Rust
engine, so the same files give the same results, printed in the same words.
Only a project function runs code of your own: each language calls its own
implementation, and the function file's tests check both on the same cases
([Project configuration](#project-configuration)).

## Quick reference

| Task | R | Python |
|---|---|---|
| Open a specification or a project | `yamaa_spec(path)`, `yamaa_project(path)` | `yamaa.spec(path)`, `yamaa.project(path)` |
| One specification of a project | `project$spec("DM")` | `project.spec("DM")` |
| A project's specifications | `project$names` | `project.names` |
| Check without data | `spec$check()$issues` | `spec.check().issues` |
| Build one dataset in memory | `run <- spec$run()` | `run = spec.run()` |
| Did it succeed? | `run$status` | `run.status` |
| Save it, and get the paths written | `paths <- run$save()` | `paths = run.save()` |
| Build every SDTM dataset | `project$run("sdtm")$save()` | `project.run("sdtm").save()` |
| One dataset of a project run | `runs[["ADSL"]]$output` | `runs["ADSL"].output` |
| Run report | `run$report` | `run.report` |
| A table in Polars or pandas | `run$output` is a data frame | `run.output.to_polars()`, `run.output.to_pandas()` |
| define.xml and transport files | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| See the resolved specification | `spec$resolve()` | `spec.resolve()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
| Lint, and fix the layout | `yamaa_lint(paths, fix = TRUE)` | `yamaa.lint(paths, fix=True)` |
| JSON Schema of a file type | `yamaa_schema("spec")` | `yamaa.schema("spec")` |
| Engine version and platform | `yamaa_info()` | `yamaa.info()` |
| Check an installation | `yamaa_selftest()` | `yamaa.selftest()` |

Examples in R and Python are shown in tabs, R first; pick a language once and
every tab on the page follows. From 1.0.0, every 1.x release keeps the
functions, arguments, members and printed forms on this page, as the
[stability contract](../release.md#stability-contract) says.

## Input files

Every yamaa file is YAML whose first two fields are `yamaa`, the oldest
language version the file needs, always quoted, as in `yamaa: "1.0"`, and
`type`, which names one of four file types.

* A **specification** (`type: spec`) describes one dataset: its inputs, keys,
  columns, checks and `output`.
* A **project** (`type: project`) describes a study. It lists the
  specifications of each section, `sdtm`, `adam` or `send`, the function and
  codelists files they share, the lock of each host language, and the
  metadata define.xml needs.
* A **function file** (`type: function`) declares one project function: its
  parameters, return type and tests, and the callable that implements it in
  each host language.
* A **codelists file** (`type: codelists`) holds the codelists of one
  terminology source ([Codelists and terminology](terminology.md)).

An engine reads a file that declares its own language version or an older 1.x
one, and refuses a newer file as `unsupported_language_version`, naming the
version the file needs. The [schema fields](../reference/schema-fields-v1.md)
list every field of each file type.

> We recommend using AI to help you create these files. Human review and
> sign-off are necessary steps.

## Dataset specification

For example, `sdtm/dm.yaml` builds SDTM DM, one row per subject, from the
study's ODM XML export ([Read ODM XML](#read-odm-xml)).

```yaml
# sdtm/dm.yaml
yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [STUDYID, USUBJID]
input:
  ODM: ../edc/odm.xml

output:
  path: dm.parquet
  columns: [STUDYID, DOMAIN, USUBJID, SUBJID, AGE, AGEU, SEX]
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

  # define.xml lists each origin of a column.
  - name: SEX
    type: str
    label: Sex
    derivation: {odm: ODM.IT.DM.SEX}
    submission:
      origins:
        - type: Collected
          source: Investigator
```

`model` names the CDISC model, `SDTM`, `ADaM` or `SEND`; a specification
that declares `submission` metadata needs it. `keys` identify the rows, and
each column's `derivation` says how its value is made. A column's
`submission.origins` is a list, since define.xml can give a variable several
origins.

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
    run$status            # "succeeded" or "failed"
    run$output            # generated dataset
    run$issues            # identified errors
    run$report            # run report
    paths <- run$save()   # save run report and dataset
    ```

=== "Python"

    ```python
    spec = yamaa.spec("sdtm/dm.yaml")
    run = spec.run()
    run.status            # "succeeded" or "failed"
    run.output            # generated dataset
    run.issues            # identified errors
    run.report            # run report
    paths = run.save()    # save run report and dataset
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
running project code. It does not open the input datasets, so some problems,
such as a column name that no input has, appear only when you call `run()`.

Each problem is one row of `issues`: its `severity`, `phase` and
`condition`, the `rule` it breaks, the `spec_paths` it concerns, a `message`
to read, and its facts in `context` as JSON text. Its `file`, `line` and
`column` say where to edit: the YAML file that wrote the first spec path,
which is the parent layer when the specification inherits that value, and
the line and column of that entry. A problem with no place in a YAML file,
such as a project section that does not exist, leaves them missing.

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
    runs <- yamaa_project("project.yaml")$run()
    runs$status        # "succeeded"
    runs               # the summary
    runs[["ADSL"]]     # one specification's run
    ```

=== "Python"

    ```python
    runs = yamaa.project("project.yaml").run()
    runs.status        # "succeeded"
    print(runs)        # the summary
    print(runs["ADSL"])
    ```

In a study whose ADAE fails, so that ADTTE, which reads ADAE, is skipped, the
run of its `adam` section and ADTTE's run print:

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
sdtm/dm.yaml`. A check prints `yamaa check of <path>` with its status and
issue count, and a submission counts its define.xml and transport files.

In R, every result also has S3 methods: `print()` writes the lines and
returns the result invisibly, `format()` returns them, `names()` lists the
result's fields and methods, starting with `status`, as `names(run)` gives
`"status" "output" "issues" "report" "save"`, and `length()` counts them. A
project run's `names()` are its specifications, as `$names` gives them. In
Python, `repr()` and `str()` return the printed text.

The labels of each printed line, their order and separators, the layout of
the `first issue:` line and of a project run's member lines, and a table
preview's `yamaa.Table:` header are frozen from 1.0.0. The messages are not: a
release may improve them, so a program reads `status`, and an issue's
`condition` and `rule`, never the printed text.

## Errors

`check()` and `run()` return problems as values. A call that cannot complete,
such as a `save()` of a failed run, a `resolve()` of a specification that
cannot be composed, or a name the project does not have, raises `yamaa.Error`
in Python or signals a `yamaa_error` condition in R. Its `issues` hold every
issue, in the columns of the issues table, and its `paths` list the files a
refused save wrote first. Its message says what the call could not do, then
gives the first issue with its location and counts the others:

```text
cannot save ADAE: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5] (and 2 more issues)
```

The message starts with `cannot save`, `cannot resolve`, `cannot select`,
`cannot read`, `cannot lint` or `cannot give the schema of`, and what the
call named.

=== "R"

    ```r
    result <- tryCatch(
      yamaa_project("project.yaml")$run("send"),
      yamaa_error = function(error) error
    )
    conditionMessage(result)
    result$issues
    ```

=== "Python"

    ```python
    try:
        yamaa.project("project.yaml").run("send")
    except yamaa.Error as error:
        print(error)
        error.issues
    ```

## See the resolved specification

`resolve()` returns the specification after its layers are composed: the
complete normalized model, where each field came from, and where each resource
is. A producing specification linked through `schema` is a resource of its
consumer, not a separate entry. R returns a named list and Python a dict. It
reads no study data and runs no project code. Each entry records its
`language_version`, the newest version any of its layers declares in
`yamaa`.

=== "R"

    ```r
    resolved <- yamaa_spec("sdtm/dm.yaml")$resolve()
    names(resolved)
    ```

=== "Python"

    ```python
    resolved = yamaa.spec("sdtm/dm.yaml").resolve()
    resolved.keys()
    ```

## Project configuration

A project file lists each section's specifications in `specs`, with what they
share:

```yaml
# project.yaml
yamaa: "1.0"
type: project
functions: [functions/bmi.yaml]
environments:
  python: {lock: uv.lock}
  r: {lock: renv.lock}
study:
  id: STUDY01
  name: STUDY01
  description: An example study
  protocol_name: STUDY01

sdtm:
  specs: [sdtm/dm.yaml]
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}
  define: sdtm/define.xml
  transport: xpt

adam:
  specs: [adam/adsl.yaml]
  standards:
    - {name: ADaMIG, version: "1.3", status: Final}
  define: adam/define.xml
```

* `functions` lists the function files that declare the project functions
  the specifications call.
* `environments` names the packaging lock of each host language: a uv lock
  for Python and an renv lock for R. By design, one language runs the whole
  study, and each package reads only its own entry. Before a build that calls
  a project function, the package checks the installed versions of yamaa and
  of each called package against its lock, and the build fails with
  `runtime_artifact_mismatch` when one differs. Without a lock, the installed
  callables run unchecked.
* `codelists`, not used here, lists the codelists files the specifications
  bind ([Codelists and terminology](terminology.md)).
* `study`, and each section's `standards` and `define`, are the metadata
  define.xml needs; `transport` adds one Dataset-JSON (`dataset_json`) or
  SAS Transport (`xpt`) file for each dataset
  ([Create define.xml](#create-definexml-and-submission-datasets)).

A function file declares one project function: its parameters, what it
returns, the callable that implements it in each host language, and its
tests. Every build that calls the function runs its tests first, in whichever
language runs the study, before it reads any study data. The tests cover the
cases the contract implies: an ordinary value, a boundary, a missing value of
each parameter, and a decimal comparison for a `float` result.

```yaml
# functions/bmi.yaml
yamaa: "1.0"
type: function
name: bmi
description: Body mass index from a weight in kg and a height in cm.
params:
  - {name: weight_kg, type: float}
  - {name: height_cm, type: float}
returns: float
implementations: {python: projectbmi.bmi, r: projectbmi::bmi}

tests:
  - id: normal
    covers: [normal, numeric-comparison]
    args: {weight_kg: 70.0, height_cm: 175.0}
    result: 22.857142857142858

  - id: zero-weight
    covers: [boundary, numeric-comparison]
    args: {weight_kg: 0.0, height_cm: 175.0}
    result: 0.0

  - id: missing-weight
    covers: ["short-circuit-missing:weight_kg"]
    args: {weight_kg: null, height_cm: 175.0}
    result: null

  - id: missing-height
    covers: ["short-circuit-missing:height_cm"]
    args: {weight_kg: 70.0, height_cm: null}
    result: null
```

## Read another dataset

`adam/adsl.yaml` builds ADSL from the DM dataset that `sdtm/dm.yaml` writes,
and calls `bmi`:

```yaml
# adam/adsl.yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: {path: ../sdtm/dm.parquet, schema: ../sdtm/dm.yaml}
  VS:
    path: ../edc/vitals.csv
    types: {WEIGHT: float, HEIGHT: float}
    key: [USUBJID]
base: DM

output:
  path: adsl.parquet
  columns: [STUDYID, USUBJID, AGE, SEX, BMIBL]
  report: adsl-report.parquet

columns:
  # A copied column takes its type and label from sdtm/dm.yaml.
  - copy:
      dataset: DM
      columns: [STUDYID, USUBJID, AGE, SEX]

  # VS.WEIGHT and VS.HEIGHT join on VS's key, USUBJID.
  - name: BMIBL
    type: float
    label: Baseline Body Mass Index (kg/m2)
    derivation:
      function:
        name: bmi
        args: {weight_kg: VS.WEIGHT, height_cm: VS.HEIGHT}
```

* `DM` names the dataset file and, through `schema`, the specification that
  writes it. The copy entry takes four of its columns, each with its type
  and label, and with a Predecessor origin that names `DM`'s column
  ([SDTM to ADaM](sdtm-to-adam.md)). The link reads only what `dm.yaml`
  declares; it never runs DM.
* `DM` is the `base` that builds the rows, so a read of `DM` needs no join. A
  read of another input, such as `VS.WEIGHT`, is a join on that input's
  declared `key`, each column matched against the current row's column of the
  same name; yamaa never infers a join from the columns two datasets happen to
  share ([Declared join keys](derive.md#reading-another-dataset-declared-join-keys)).
* `function` calls a project function with its arguments by name. A
  specification that calls one, or binds a codelist, runs with its project:
  open it with the project's `spec(name)`.

## Work with a project

`yamaa_project()` and `yamaa.project()` read nothing until you call a method.
`spec(name)` returns one of the project's specifications by its `domain` name,
with the project's codelists and functions. Open a specification this way
when it binds a codelist or an inventory vocabulary, or calls a project
function: a specification that does so, checked or run without its project,
fails before any data is read with `project_binding_missing` at each such
binding.

=== "R"

    ```r
    project <- yamaa_project("project.yaml")
    project$names             # every specification, in run order
    adsl <- project$spec("ADSL")
    adsl$run()$save()
    project$check()$issues    # every specification, no data read
    ```

=== "Python"

    ```python
    project = yamaa.project("project.yaml")
    project.names             # every specification, in run order
    adsl = project.spec("ADSL")
    adsl.run().save()
    project.check().issues    # every specification, no data read
    ```

Only specifications listed in the project have names, and each name is unique
across the project. An unknown name raises an error whose issue lists the
names the project has. `names` lists them in the order a run of every section
builds them, each specification after the ones it reads from, and reads no
study data; it raises when the project file cannot be read or lists a name
twice.

The project's `check()` checks every listed specification as `spec(name)`
would. It also reports two listings that share a name, specifications that
would write the same file, and an input that reads another listed
specification's dataset without linking it through `schema`.

## Project execution

`run()` builds every specification in the section you name, `"sdtm"`, `"adam"`
or `"send"`, in dependency order. Without a name, it builds every section.
Naming a section the project does not declare, in `run()` or `submission()`,
raises an error whose issue lists the sections it declares. As with `spec()`,
a later `run()` reuses what earlier runs of the same project object compiled,
and compiles again any specification whose files changed.
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
    len(runs)                     # how many; "ADSL" in runs, list(runs)
    runs["ADSL"].output           # one specification's dataset
    runs.report                   # all run reports, in run order
    runs.save()                   # save every dataset and run report
    ```

## Create define.xml and submission datasets

`submission()` composes one define.xml for the selected section, or one for
every section that names `define`. Each specification it covers declares the
dataset's `submission` metadata, and each of its output columns a `label` and
`submission` metadata ([the full mapping table](excel.md#2-the-full-mapping-table)
lists them), as the
[sdtm-dm-metadata](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-metadata.html)
benchmark does; the short `sdtm/dm.yaml` above declares too little, and its
submission fails with `submission_metadata_missing`. With
`transport: dataset_json` or `transport: xpt` in a section, it also writes
each saved dataset as one Dataset-JSON or SAS Transport version 5 file next to
define.xml.
Its `save()` writes each section's transport files, then its define.xml, and
returns the paths it wrote. Its `created` is the creation time that the
documents carry: the `created` you gave, or else the time the submission read
once and gave every section.

=== "R"

    ```r
    project$run("sdtm")$save()
    submission <- project$submission("sdtm", created = "2026-10-21T12:00:00")
    submission$status
    submission$issues
    submission$save()
    project$submission()$save()
    ```

=== "Python"

    ```python
    project.run("sdtm").save()
    submission = project.submission("sdtm", created="2026-10-21T12:00:00")
    submission.status
    submission.issues
    submission.save()
    project.submission().save()
    ```

## Interrupts

Press Ctrl+C while a call runs, and it stops at the next point where the
engine looks for an interrupt: as it starts, before each specification of a
project and between the steps that build a dataset, before each batch of
project function calls, before each section of a submission, and once in a
save, after it has prepared every file and before it writes the first. The
call returns no result and keeps nothing it built. Python raises the
`KeyboardInterrupt` itself, and R signals its interrupt condition, as any
interrupted R code does. A save writes every file it planned or none: one
stopped this way writes nothing and leaves the result unsaved, so you can
call `save()` again. A project function that raises `KeyboardInterrupt` or
`SystemExit` in Python ends the whole call the same way.

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
    findings <- yamaa_lint(c("project.yaml", "sdtm/", "adam/"))
    remaining <- yamaa_lint("sdtm/dm.yaml", fix = TRUE)
    ```

=== "Python"

    ```python
    findings = yamaa.lint(["project.yaml", "sdtm/", "adam/"])
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

`selftest()` runs the benchmark cases built into the engine, in memory, and
compares each result with the case's independently authored expected files;
it reads and writes no file and uses no network, so every host runs the same
cases on the same bytes. `status` is `failed` when a case failed, and
`succeeded` otherwise. `cases` has one row per case, with its `case`,
`status` and `message`; a case that needs a feature this build does not
compile is `skipped`. `info` names the engine, its language versions and
features, the host and the platform. Printing the result gives its report,
the installation and operational qualification evidence of the installation:
the summary line, the first failure when a case failed, a `not covered:` line
naming each feature, as `info()` names it, that no case exercised, and the
information line.

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
files (`.xpt`) or ODM XML files (`.xml`). A SAS input declares its
`encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int}}`. Outputs are
CSV or Parquet; Dataset-JSON and SAS Transport version 5 files come from
`submission()`.

A relative path resolves from the YAML file that writes it, so
`../edc/odm.xml` in `sdtm/dm.yaml` is the study's `edc/odm.xml`. A path you
pass to a function is a character string in R, and a string or any
`os.PathLike`, such as a `pathlib.Path`, in Python.

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
