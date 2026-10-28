# R, Python and command-line API

yamaa has the same small API in R and Python. Use it to create, verify and save
SDTM and ADaM datasets and define.xml files. Both packages run the same Rust
engine, so the same files give the same results, printed in the same words,
provided that host-specific project functions are equivalent. Each language
calls its own implementation, and the function file's tests check both on the
same cases
([Project configuration](#project-configuration)). The
[command line](#command-line) runs the same engine from a shell, except for
project functions.

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
| Read an output as a stream | `spec$run(stream = TRUE)$output` | `spec.run(stream=True).output` |
| Follow a run, and see where it spent its time | `spec$run(progress = print)$timings` | `spec.run(progress=print).timings` |
| define.xml and transport files | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| See the resolved specification | `spec$resolve()` | `spec.resolve()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
| Lint, and fix the layout | `yamaa_lint(paths, fix = TRUE)` | `yamaa.lint(paths, fix=True)` |
| JSON Schema of a file type | `yamaa_schema("spec")` | `yamaa.schema("spec")` |
| Conditions and their history | `yamaa_conditions()` | `yamaa.conditions()` |
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
version the file needs. The [schema fields](../reference/schema-fields.md)
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

Every table that yamaa returns is a data frame in R, except the output of a
run read as a stream ([Read an output as a stream](#read-an-output-as-a-stream)).
In Python it is a `yamaa.Table`, which needs no other package: printing it
shows its column names and types and its first and last rows, `len()` and
`num_rows` count its rows, `column_names` names its columns, and `to_pylist()`
gives one dict per row. `to_polars()`, `to_pandas()` and `to_arrow()` convert
it to the table library you use, which must be installed: Polars, pandas with
PyArrow, or PyArrow. A library that reads the Arrow PyCapsule interface, such
as `polars.DataFrame(table)` or `pyarrow.table(table)`, reads it directly.

| Column type | R | Python value | Arrow |
|---|---|---|---|
| `str` | character | `str` | Utf8 |
| `int` | double; an error beyond 2^53 | `int` | Int64 |
| `float` | double | `float` | Float64 |
| `date` | `Date` | `datetime.date` | Date32 |
| `datetime` | `POSIXct` in UTC | `datetime.datetime`, without a time zone | Timestamp, in seconds |
| list, in `issues` and `conditions()` | list column | `list` | List |

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

## Read an output as a stream

`run(stream = TRUE)` in R and `run(stream=True)` in Python build a dataset as
its output is read. The output's whole table, which a run without `stream`
keeps, is never built; the run still holds the accepted rows that each batch
is built from. The call reads no study data and runs no project code. The
output is an Arrow stream: in Python a `yamaa.Table`, which a library that
reads the Arrow PyCapsule stream interface pulls batch by batch, such as
`pyarrow.RecordBatchReader.from_stream(run.output)` or
`polars.DataFrame(run.output)`; in R an Arrow C stream, an external pointer of
class `nanoarrow_array_stream`, which nanoarrow and arrow read, such as
`arrow::RecordBatchReader$import_from_c(run$output)`.

The first batch builds the run through every check and the declared order, so
it exists only once the whole output is accepted. Each later batch, of at most
65,536 rows, is made as it is read and not kept. The batches hold the values,
order and column types of the table a run without `stream` returns, and the
run's `save()` writes the same files.

Once the stream has ended, `status`, `issues`, `report`, `save()` and printing
give the run's result. A run that fails ends the stream with an error that
names its first issue, and it then holds its issues and saves only its report
file. Before anything reads the output, asking for the result runs the
dataset whole, as a run without `stream` does, and the output is then the
table it keeps, which can be read as often as you like. In Python, `len()`,
`num_rows`, printing, `to_pylist()` and the `to_*()` conversions of the
output read it whole in the same way.

A stream is read once. Reading it again, or asking for the result while a
reader has not reached its end, raises `RuntimeError` in Python and signals
`yamaa_error` in R. Stopping before the last batch, by dropping the reader or
with Ctrl+C, cancels the run: nothing more is built, and the run has no
result, so it saves nothing, and asking for its result reports `cannot read
<dataset>: operation cancelled`. Through another library's reader, Ctrl+C, or
a project function's `KeyboardInterrupt` in Python and interrupt in R, ends
the stream with that library's error. A run whose project functions run in
the host is built only on the thread that called `run()`. Read on another
thread, its stream refuses the read with that library's error, whose message
is `cannot read <dataset>: a run that can call project functions is built only
on the thread that started it [host_thread]`. The refusal holds no issue and
fails nothing: the thread that called `run()` can still read the stream.

=== "R"

    ```r
    run <- yamaa_spec("adam/adlb.yaml")$run(stream = TRUE)
    reader <- arrow::RecordBatchReader$import_from_c(run$output)
    while (!is.null(batch <- reader$read_next_batch())) {
      # write or summarize the batch
    }
    run$status
    run$save()
    ```

=== "Python"

    ```python
    import pyarrow as pa

    run = yamaa.spec("adam/adlb.yaml").run(stream=True)
    for batch in pa.RecordBatchReader.from_stream(run.output):
        ...                      # write or summarize the batch
    run.status
    run.save()
    ```

The command line saves whole runs; it has no streaming flag.

## Progress and timings

`check()`, `run()` and `submission()` take a `progress` function, called
with one event as each phase starts and as it ends. An event is a named list
in R and a dict in Python, with `name`, `phase`, `kind`, `outcome`, `number`
and `rows`:

| Field | Value |
|---|---|
| `name` | The specification's path as given; a project run member's name; a submission's section |
| `phase` | `preparation`, `activation`, `input`, `admission`, `derivation`, `output_keys`, `verification`, `output` or `composition` |
| `kind` | `start` or `end` of the phase; `partition` as a partitioned run enters a partition of its input, in `derivation`; `batch` as a streamed run hands out a batch of its output, after `output` |
| `outcome` | For an `end`, `succeeded` or `failed`; otherwise missing |
| `number`, `rows` | For a `partition`, its number from 1 and its records; for a `batch`, its number and its rows; otherwise missing |

A run passes the phases from `preparation` to `output` in that order, with
`activation` only when its specification calls a project function; a run of
a plan an earlier `run()` of the object kept starts at `input`. A check is
one `preparation` of each specification it checks, and a submission one
`composition` of each section. The function runs on the thread that called
the method, so it can show progress, and it may call yamaa itself, on the
same object too. In Python, a call releases the global interpreter lock while
the engine works and takes it again to call the function or a project
function, so other threads run meanwhile: two threads can run two
specifications at once.

A `progress` function that signals an error in R or raises in Python is
called no more: the method finishes its work, and its result then fails with
one issue, `engine_rejected` with the code `host_progress_failure`. A failed
run keeps no output and saves nothing; a project run builds no later
specification, and a submission composes no later section, and either
refuses its save. An interrupt in the function cancels the call, as Ctrl+C
does ([Interrupts](#interrupts)).

Every check, run, project run, member of a project run and submission has
`timings`: a table of each phase it ran, in the order the phases started,
with the columns `name`, `phase`, `started_ms` (from the start of the call),
`elapsed_ms` and `outcome`. Timings are facts of one call, so no saved file
holds them: the saved output, run report, Dataset-JSON and define.xml are the
same with or without them. The command line prints both under `--verbose`
([Output](#output)).

=== "R"

    ```r
    run <- yamaa_spec("adam/adlb.yaml")$run(progress = function(event) {
      if (identical(event$kind, "start")) message(event$phase, " ...")
    })
    run$timings
    ```

=== "Python"

    ```python
    def report(event):
        if event["kind"] == "start":
            print(event["phase"], "...")

    run = yamaa.spec("adam/adlb.yaml").run(progress=report)
    run.timings.to_polars()
    ```

## Check a specification without data

`check()` reports problems in a specification without reading study data or
running project code. It does not open the input datasets, so some problems,
such as a column name that no input has, appear only when you call `run()`.
The pattern of a `str_extract` or `str_replace` is one of them when only the
data gives its source's type, such as a field of a Parquet, SAS or ODM input
that `types` does not declare: `run()` then reports either the source's type
or the pattern.

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

A fragment, a specification that a layer lists under `include`, names no
template of its own, so a check of the fragment alone fails at the first
value its template would give. `check(parent = )` checks it as if the
template you name were its parent, before any study file lists it: the
fragment composes as it would in a file that names that template as its
`parent`, lists the fragment under `include` and writes nothing else, and the
check reports what that file's check would report. Each issue is located in
the fragment or in a layer of the template. `parent` is a path you give, like
the specification's own: one that cannot be read is an issue that names it
as you gave it, at no field. A specification that declares `parent` or
`include` is no fragment, and a `parent` given for it raises `yamaa.Error`,
as `value_not_permitted` at `parent`
([LAYER-4](../../rules/inheritance.md#layer-4)). `run()`, `resolve()` and a
project's `check()` take no `parent`.

=== "R"

    ```r
    yamaa_spec("fragments/ae-duration.yaml")$check(parent = "templates/adae.yaml")$issues
    ```

=== "Python"

    ```python
    yamaa.spec("fragments/ae-duration.yaml").check(parent="templates/adae.yaml").issues
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
`"status" "output" "issues" "report" "timings" "save"`, and `length()` counts
them. A project run's `names()` are its specifications, as `$names` gives
them. In Python, `repr()` and `str()` return the printed text.

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
  transport: dataset_json
```

* `functions` lists the function files that declare the project functions
  the specifications call.
* `environments` names the packaging lock of each host language: a uv lock
  for Python and an renv lock for R. By design, one language runs the whole
  study, and each package reads only its own entry. Before a build that calls
  a project function, the package checks the installed versions of yamaa and
  of each called package against its lock, and the build fails with
  `runtime_artifact_mismatch` when one differs. Without a lock, the installed
  callables run unchecked. The lock pins yamaa too, since the engine is part
  of what the functions were tested with, so after you upgrade yamaa every
  build that calls a project function fails this way until its lock is made
  again: regenerate each `environments.<host>.lock`, for example with
  `uv lock --upgrade-package yamaa` or `renv::snapshot()`, and re-run each
  function package's own tests.
* `codelists`, not used here, lists the codelists files the specifications
  bind ([Codelists and terminology](terminology.md)).
* `study`, and each section's `standards` and `define`, are the metadata
  define.xml needs, and `transport` writes one Dataset-JSON (`dataset_json`,
  or `dataset_ndjson` for its newline-delimited form) or SAS Transport
  (`xpt`) file for each dataset. Under `context: Submission`, the default, a
  section that composes define.xml must name its `transport`
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

Set `define_version` on a submission section to select `2.0.0` or any
published 2.1 release from `2.1.0` through `2.1.11`. The generated document
retains that exact version; omission selects `2.1.11`.

`submission()` composes one define.xml for the selected section, or one for
every section that names `define`. Each specification it covers declares the
dataset's `submission` metadata, and each of its output columns a `label` and
`submission` metadata ([the full mapping table](excel.md#2-the-full-mapping-table)
lists them), as the
[sdtm-dm-metadata](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-metadata.html)
benchmark does; the short `sdtm/dm.yaml` above declares too little, and its
submission fails with `submission_metadata_missing`. Each section also names
its `transport`, `dataset_json`, `dataset_ndjson` or `xpt`, and the submission
writes each saved dataset as one Dataset-JSON file, in its single-object or
its newline-delimited form, or one SAS Transport version 5 file next to
define.xml. A section without `transport` fails with `missing_required_field`
at it, unless it sets `context: Other` for a document that is not a
submission, which then writes define.xml alone.
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
interrupted R code does. An interrupt never stops a save partway: one
stopped this way writes nothing and leaves the result unsaved, so you can
call `save()` again. A project function that raises `KeyboardInterrupt` or
`SystemExit` in Python ends the whole call the same way. A run read as a
stream also stops before each batch
([Read an output as a stream](#read-an-output-as-a-stream)).

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

## Conditions and their history

`conditions()` returns the condition catalogue: every condition the engine
may report in an issue, and every condition a release retired, one row per
condition in name order. `condition` is the name that an issue's `condition`
column gives, `severity` the severity of its issues, `phases` the phases that
may report it, `rules` the rule cards that its issues cite in their `rule`
column, in card order, and `message` the template that its issues' messages
are written from. `renamed_from` gives the name a condition had before a
release renamed it, and `retired_in` the release that retired a condition,
which keeps its row with no phase, rule or message. So a program that
matches conditions by name, such as one that explains them, finds every name
it knows: a condition of the catalogue, the `renamed_from` of one, or a
retired one. `phases` and `rules` are list columns, and a `message`,
`renamed_from` or `retired_in` that a condition lacks is `NA` in R and `None`
in Python. The catalogue is part of the installed engine, so every
installation of one release gives the same table, and `yaml/conditions.yaml`
at the release's tag holds it too.

=== "R"

    ```r
    catalogue <- yamaa_conditions()
    catalogue[!is.na(catalogue$renamed_from), c("condition", "renamed_from")]
    ```

=== "Python"

    ```python
    rows = yamaa.conditions().to_pylist()
    renamed = {row["renamed_from"]: row["condition"] for row in rows if row["renamed_from"]}
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
engine 0.6.2, language 1.0, python 3.12.4, linux-x86_64
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
files (`.xpt`), Dataset-JSON 1.1 files (`.json`, or `.ndjson` for the
newline-delimited form) or ODM XML files (`.xml`). A SAS input declares its
`encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int}}`. A
Dataset-JSON input takes each field's type from its column's `dataType`, so
`types` only narrows one, as in `{path: input/vs.ndjson, types: {VISITNUM: int}}`. Outputs are
CSV or Parquet; Dataset-JSON and SAS Transport version 5 files come from
`submission()`. Every run divides its input into partitions of whole subjects,
so a CSV or Parquet input that is a specification's only input may hold more
than 10,000,000 records when the windows and aggregates group by the first two
keys, such as `STUDYID` and `USUBJID`
([Resource limits](../release.md#resource-limits)).

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
parent, and an included fragment's beside the fragment, even when it sits on
another drive than the specification you pass. A
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
yamaa check PATH [--spec NAME | --parent PATH] [--verbose]
yamaa run PATH [--spec NAME | --section SECTION] [--verbose]
yamaa submission PATH [--section SECTION] [--created TIME] [--verbose]
yamaa resolve PATH [--spec NAME]
yamaa read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] [--max-expanded-bytes N] [--max-xml-depth N]
yamaa lint [--fix] PATH...
yamaa schema TYPE
yamaa conditions
yamaa selftest
```

| Command | What it does | In R and Python |
|---|---|---|
| `check` | Checks a specification or a project without reading data, or with `--parent` a fragment on a template | `check()`, `check(parent = )` |
| `run` | Builds a specification or a project, then saves what it built and prints each path written | `run()`, then `save()` |
| `submission` | Composes a project's define.xml and transport files, then saves them and prints each path written | `submission()`, then `save()` |
| `resolve` | Prints the resolved specification as canonical JSON | `resolve()` |
| `read-odm` | Reads SOURCE, an ODM XML file or a TAR archive that holds one, and saves its items to OUT | `read_odm()` |
| `lint` | Lints each PATH, a yamaa file or a directory of them; with `--fix`, first applies the proved fixes in place | `lint()` |
| `schema` | Prints the JSON Schema of TYPE: `spec`, `project`, `codelists` or `function` | `schema()` |
| `conditions` | Prints the condition catalogue | `conditions()` |
| `selftest` | Runs the cases built into the engine and compares each with its expected files | `selftest()` |

`check`, `run` and `resolve` take a specification or a project, whichever
PATH declares in its `type:`, and `submission` takes a project. For a
project, `--spec NAME` selects one of its specifications, as `spec(name)`
does, and `--section SECTION` one of its sections, `sdtm`, `adam` or `send`;
without them, `check` and `run` cover the whole project, and `submission`
every section that names `define`. `run` takes `--spec` or `--section`, not
both, and `resolve` needs `--spec` for a project, which has no resolved form
of its own. For a specification, `check --parent` checks it as a fragment
whose parent is the template the flag names, as `check(parent = )` does, so
`check` takes `--spec` or `--parent`, not both. `--created TIME` gives the ISO 8601 creation time that a
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
`unsupported_operation` at the first derivation that calls one, in the
order a formatted file writes them (the intermediates', then the columns',
then the row templates'): run that specification from R or Python. Every
other build runs as it does in R and Python, and `check` checks every
specification, since it runs no project code.

```sh
yamaa check project.yaml
yamaa check fragments/ae-duration.yaml --parent templates/adae.yaml
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
`--fix` adds `; fixed 1 file`. `conditions` prints the catalogue as a table: a
line of its column names, then one line per condition, each cell separated by
a tab, a list's items joined with `, ` and a missing cell empty. `selftest`
prints the report that R and Python print. A call that cannot complete prints
the message of the error that R and Python raise ([Errors](#errors)), which
carries the first issue, then each other issue's line and the paths written
before it.

With `--format json`, a command prints one canonical JSON document instead:
one line, with its members in key order ([JSON documents](#json-documents)).

Standard error carries only the refusals of exit status 2, each as
`yamaa: <message>` and then `run 'yamaa --help' for usage`, which a failed
write to standard output leaves out. There are two exceptions. A `selftest`
case that panics fails, and the panic message, which shows where the engine
panicked, goes to standard error. And `check`, `run` and `submission` with
`--verbose` write each progress event there as it happens, as
`<name>: <phase> <kind>` with a partition's number and records or an end's
outcome, such as `sdtm/dm.yaml: derivation end: succeeded`. Then, for each
run, they write how it divided its driver input, as
`<name>: partitions <strategy> <count>`, where the strategy is
`whole_table`, `key_bounded` or `consecutive`, such as
`sdtm/dm.yaml: partitions key_bounded 1`. Last comes the timings table of
the result, its column names and one line per phase, each cell separated by
a tab ([Progress and timings](#progress-and-timings)).
Standard output is the same with or without `--verbose`.

### Exit status

| Status | Meaning |
|---|---|
| 0 | The command completed and succeeded: its status is `succeeded` and any save wrote everything, or `resolve`, `schema`, `--version` or `--help` printed its output. |
| 1 | The command completed with findings: a status of `failed` or `skipped`, a refused save, remaining lint findings, a failed selftest case, or a refusal that carries issues, such as an unknown section, an unknown TYPE or a PATH that cannot be read. |
| 2 | The command could not run as asked: a usage error, an argument the engine cannot take, such as text that is not UTF-8, a PATH whose declared type the command does not take, or a failed write to standard output. |

A file problem the engine reports as an issue, such as a missing input,
exits 1, since R and Python return the same issue.

Press Ctrl+C, and `check`, `run` and `submission` stop at the engine's next
checkpoint, as a call in R or Python does ([Interrupts](#interrupts)): the
command prints nothing, and an interrupt never stops a save partway. The
other commands stop at once, so an interrupted `lint --fix`, which replaces
each file whole, can leave some files fixed and others not. Either way, the
process then ends as an interrupted program does, with none of the statuses
above: on Linux and macOS by the interrupt signal, which a shell reports as
status 130, and on Windows with the exit code `STATUS_CONTROL_C_EXIT`.

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
| `conditions` | `command`, `status`, `conditions` |
| `selftest` | `command`, `status`, `cases`, `info` |
| `--version` | `version`, `language_versions`, `features`, `host`, `host_version`, `platform` |

* `command` is the command's name. `path`, `spec`, `section` and `type` are
  its arguments as given, null for a flag not given; `read-odm`'s `path` is
  SOURCE, and a check with `--parent` prints the same keys, its `path` the
  fragment.
* `status` is the result's status, which a refused save keeps, and `failed`
  for a call refused before it has a result. `lint`'s is `failed` when a
  finding remains, and `selftest`'s when a case failed; `conditions`' is
  always `succeeded`.
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
* `conditions` has one object for each condition of the catalogue, in name
  order, with its `condition`, `severity`, `phases`, `rules`, `message`,
  `renamed_from` and `retired_in`; `phases` and `rules` are lists, and a
  missing cell is null.

From 1.0.0, every 1.x release keeps the commands, flags, exit statuses and
JSON keys on this page, and may add commands, flags and keys, as the
[stability contract](../release.md#stability-contract) says. The messages
are not frozen: a program reads `status`, and an issue's `condition` and
`rule`, never its `message`.
