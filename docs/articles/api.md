# R and Python API

yamaa has the same small API in R and Python. Two objects mirror the two kinds
of file you write: a **spec** is one specification, which builds one dataset,
and a **project** is a study, which lists its specifications and holds the
metadata they share. Methods on these objects check, resolve, run and package.
Only a result writes files, and always through `save()`.

Every example on this page has an R tab and a Python tab. R is shown first;
pick a language once and every example on the page follows. Both packages run
the same Rust engine, so the same files give the same results. Install them
from the [GitHub releases](https://github.com/elong0527/yamaa/releases); the
[R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) and
[Python](https://github.com/elong0527/yamaa/blob/main/python/README.md)
package READMEs give the commands.

=== "R"

    ```r
    library(yamaa)
    ```

=== "Python"

    ```python
    import yamaa
    ```

## The files you write

A **specification** (`spec.yaml`) describes one dataset: its inputs, columns,
checks and `output`. `output.path` names the dataset file, and the optional
`output.report` names the run report file.

A **project** (`project.yaml`) describes a study. It lists the specifications
of each standard, the codelists they share, the submission metadata, and the
environment file for each language:

```yaml
schema_version: "1.0"
environments: {python: python/environment.yaml, r: r/environment.yaml}
study: {id: STUDY01, name: STUDY01, description: Example, protocol_name: STUDY-01}
codelists: [terminology/ct.yaml]
sdtm:
  specs: [sdtm/dm.yaml, sdtm/ae.yaml]
  standard: {name: SDTMIG, version: "3.4", status: Final}
  define: sdtm/define.xml
  datasets: json
adam:
  specs: [adam/adsl.yaml, adam/adae.yaml]
  standard: {name: ADaMIG, version: "1.3", status: Final}
  define: adam/define.xml
```

A section needs only `specs` to run. The `study` block and a section's
`standard` and `define` are needed only to create submission files, and
`datasets: json` adds Dataset-JSON files to them, so a project can list its
specifications before its submission metadata is ready. Relative paths in the
project file resolve from its directory.

An **environment** (`environment.yaml`) holds the code one language installs:
an optional lock file (`uv.lock` for Python, `renv.lock` for R) and the project
functions your specifications call. Write one per language; the project's
`environments` map says which file belongs to which language, and each package
reads only its own. Relative paths in an environment file resolve from that
file.

```yaml
schema_version: "1.0"
lock: ../uv.lock
functions:
  bmi: functions/bmi.yaml
```

## Run one specification

`yamaa_spec()` and `yamaa.spec()` read nothing. `run()` builds the dataset in
memory and writes nothing; `save()` writes the run report to `output.report`,
when the specification declares one, and then the dataset to `output.path`.

=== "R"

    ```r
    spec <- yamaa_spec("sdtm/dm.yaml")
    run <- spec$run()
    run$output    # the dataset, or NULL when the run failed
    run$issues    # what is wrong; empty when nothing is
    run$report    # what the run read, counted and checked
    run$save()
    ```

=== "Python"

    ```python
    spec = yamaa.spec("sdtm/dm.yaml")
    run = spec.run()
    run.output    # the dataset, or None when the run failed
    run.issues    # what is wrong; empty when nothing is
    run.report    # what the run read, counted and checked
    run.save()
    ```

Every method rereads the files, so an edit to `dm.yaml` shows up the next time
you call `run()`. A result is a fixed snapshot: `save()` writes exactly what
that run built, without reading the inputs again. A failed run has no output;
its `save()` writes only the declared run report and then raises with the
run's issues.

When an input links its producer's specification through `schema`, `run()`
builds the producer first, in memory, and `save()` writes the producer's files
before its consumer's.

A specification opened this way has no project, so it cannot call project
functions or check values against the project's codelists. Open it through its
project for that.

## Check a specification without data

`check()` reports problems in a specification without reading study data or
running project code.

=== "R"

    ```r
    yamaa_spec("sdtm/dm.yaml")$check()$issues
    ```

=== "Python"

    ```python
    yamaa.spec("sdtm/dm.yaml").check().issues
    ```

## See the resolved specification

`resolve()` returns the specification after its layers are composed: the
complete normalized model, where each field came from, and where each resource
is, with an entry for each linked producer. R returns a named list and Python
a dict. It reads no study data and runs no project code.

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

## Work with a project

`yamaa_project()` and `yamaa.project()` read nothing either. `spec(name)`
returns one of the project's specifications by its `domain` name, with the
project's codelists and the functions of your language's environment.

=== "R"

    ```r
    project <- yamaa_project("project.yaml")
    adae <- project$spec("ADAE")
    adae$run()$save()
    project$check()$issues    # every specification, no data read
    ```

=== "Python"

    ```python
    project = yamaa.project("project.yaml")
    adae = project.spec("ADAE")
    adae.run().save()
    project.check().issues    # every specification, no data read
    ```

Only specifications listed in the project have names, and each name is unique
across the project. An unknown name raises an error that lists the names the
project has.

The project's `check()` checks every listed specification as `spec(name)`
would. It also reports two listings that share a name, specifications that
would write the same file, and an input that reads another listed
specification's dataset without linking it through `schema`.

## Run a whole standard

=== "R"

    ```r
    project$run("sdtm")$save()    # every SDTM dataset
    project$run("adam")$save()    # every ADaM dataset
    runs <- project$run()         # everything, SDTM before ADaM
    runs$names                    # the specifications, in run order
    runs[["ADSL"]]$output         # one specification's run
    runs$report
    runs$save()
    ```

=== "Python"

    ```python
    project.run("sdtm").save()    # every SDTM dataset
    project.run("adam").save()    # every ADaM dataset
    runs = project.run()          # everything, SDTM before ADaM
    runs.names                    # the specifications, in run order
    runs["ADSL"].output           # one specification's run
    runs.report
    runs.save()
    ```

The argument names a section of `project.yaml`: `"sdtm"`, `"adam"` or `"send"`.
A section the project does not declare runs nothing, and any other name raises
an error. A run builds each of its specifications once, in dependency order,
and hands each result in memory to the specifications that link it through
`schema`. A linked producer outside the chosen section is read from its saved
file, so you can rerun ADaM against finished SDTM without rebuilding it.

`names` lists the run's specifications in run order. `runs[["ADSL"]]` in R and
`runs["ADSL"]` in Python is the run of one of them, with `output`, `issues` and
`report`. It is read-only and has no `save()`: save the project run instead. A
project run's `issues` and `report` stack those of its runs in run order; each
issue names its specification as the `entry` in its `context`, and each report
row names its dataset in `ARTIFACT`.

A failed specification stops only the specifications that read it, which fail
with `producer_not_completed` and build nothing; the others still run.
`save()` checks every target before it writes anything. When every
specification succeeded, it writes each one's report and dataset in run order.
When any failed, it writes no dataset, only the report files of the
specifications that ran, and then raises with every run's issues.

## Create define.xml and submission datasets

`submission()` composes one define.xml for the selected section, or one for
every section that names `define`. Without `datasets`, it reads no data, so
you can draft define.xml before any data exists. With `datasets: json`, it
also writes each saved dataset as Dataset-JSON next to define.xml, so every
link in define.xml names a file that exists.

=== "R"

    ```r
    project$run("sdtm")$save()
    submission <- project$submission("sdtm", created = "2026-09-11T00:00:00")
    submission$issues
    submission$save()
    project$submission()$save()     # every section that names define
    ```

=== "Python"

    ```python
    project.run("sdtm").save()
    submission = project.submission("sdtm", created="2026-09-11T00:00:00")
    submission.issues
    submission.save()
    project.submission().save()     # every section that names define
    ```

A submission is a snapshot like any result, so run and save the section
before you call `submission()`. `created` sets the creation time define.xml
records, a whole-second datetime; without it, the submission uses the current
UTC time. The same files and the same `created` give the same bytes.
`save()` writes the sections in the order `sdtm`, `adam`, `send`, and raises
when a section cannot be written.

## What a result holds

| Member | Holds |
|---|---|
| `output` | The dataset: the columns in `output.columns`, with rows in `output.order_by` order. A data.frame in R and a Polars DataFrame in Python. `NULL` or `None` when the run failed. |
| `issues` | What is wrong, one row per finding, in seven columns. Empty when nothing is wrong. |
| `report` | What the run read, counted and checked: one row for the run, then one row per input, row template, handler, output column and evaluated check. It exists whether or not the specification declares `output.report`; `save()` writes it there, for a failed run too. `NULL` or `None` only when the run never began. |

`issues` tells you what to fix; `report` is the evidence of what was checked. A
failed error-level check appears in both: in `issues` as the reason the run
failed, and in `report` as a violated check. A violated warning appears only in
`report`.

Output columns hold plain values of each column's type:

| Column type | R | Python |
|---|---|---|
| `str` | character | `String` |
| `int` | double | `Int64` |
| `float` | double | `Float64` |
| `date` | `Date` | `Date` |
| `datetime` | `POSIXct` in UTC | `Datetime` without a time zone |

Python keeps the full 64-bit integer range. An R double holds every integer up
to 2^53 exactly; a value R cannot hold exactly, an integer beyond 2^53 or text
containing NUL, raises an error instead of changing.

The issues table has the same seven columns in both languages:

| Column | Holds |
|---|---|
| `phase` | The stage that found the problem, such as `validation`, `derivation` or `verification`. |
| `condition` | The registered condition, such as `unknown_field`. |
| `requirement` | The requirement the condition cites, such as `REQ-0430`, or missing. |
| `spec_paths` | The specification items involved: a list column of text. |
| `context` | The facts of the finding as canonical JSON text; read it with `jsonlite::fromJSON()` or `json.loads()`. |
| `source_span` | The start and end byte offsets of the failing part of an expression, or missing. |
| `operand_route` | The operand steps from the expression's root to the failing node: a list column of text, or missing. |

`source_span` and `operand_route` are missing when the failure recorded no
place in an expression. That differs from a zero-width span, `0` to `0`, and
from an empty route, which is a failure at the root. In R, the list columns
hold character vectors and numeric pairs, with `NULL` for missing; in Python
they are Polars list columns.

The run report has fourteen columns, from `ARTIFACT` and `KIND` to `KEYS` and
`DETAILS`; the
[run report contract](https://github.com/elong0527/yamaa/blob/main/rules/execution/verification.md#req-0392)
defines each one. `COUNT` and `TOTAL` are integers, doubles in R, and every
other column is text.

## When something fails

`check()` and `run()` never raise for problems in a specification or its data;
they return them in `issues`. A problem in the project, such as a project file
that cannot be read, comes back the same way, as the issues of a check or run
that built nothing. These calls raise an error that carries the `issues`
explaining it:

- `save()` of a run, project run or submission that cannot write its files,
  including a failed run;
- `resolve()` of a specification that cannot be composed;
- `spec(name)` when the project cannot be read or has no specification of that
  name, and `runs[["name"]]` or `runs["name"]` for a name the run does not
  have;
- `run()` and `submission()` with a section other than `"sdtm"`, `"adam"` or
  `"send"`;
- `read_odm()` of a file it cannot read.

=== "R"

    ```r
    tryCatch(
      run$save(),
      yamaa_error = function(e) {
        message(conditionMessage(e))
        print(e$issues)
      }
    )
    ```

=== "Python"

    ```python
    try:
        run.save()
    except yamaa.Error as e:
        print(e)
        print(e.issues)
    ```

In R the error is a condition of class `yamaa_error`, and in Python a
`yamaa.Error`. Its `issues` has the seven columns above and is empty when the
message says everything, as for a name a project run does not have or a file
`read_odm()` cannot read. An argument of the wrong type raises a `TypeError` in
Python and an ordinary error in R.

## Read ODM XML

Specifications read tables, not ODM XML. `read_odm()` reads an ODM 1.3 or 2.0
XML file, or the XML member of a TAR archive, plain or gzip-compressed, into
one row per clinical item in file order. Save the table where your
specifications' `input` reads it.

=== "R"

    ```r
    odm <- yamaa_read_odm("edc/odm.xml")
    arrow::write_parquet(odm, "raw/odm.parquet")
    ```

=== "Python"

    ```python
    odm = yamaa.read_odm("edc/odm.xml")
    odm.write_parquet("raw/odm.parquet")
    ```

The table always has the same thirty columns: `ODMVersion`, `FileOID`,
`StudyOID`, `MetaDataVersionOID`, `SubjectKey`, `StudySubjectID`,
`SubjectStatus`, `StudyEventOID`, `StudyEventRepeatKey`, `EventName`,
`StartDate`, `EventStatus`, `EventWorkflowStatus`, `FormOID`, `FormRepeatKey`,
`FormName`, `FormLayoutOID`, `FormStatus`, `FormWorkflowStatus`,
`OpenQueries`, `ItemGroupOID`, `ItemGroupRepeatKey`, `ItemGroupName`,
`TransactionType`, `ItemOID`, `ItemName`, `Value`, `ValuePresent`, `IsNull`
and `SourceOrdinal`. `SourceOrdinal`, the item's position counted from one, is
an integer, a double in R. Every other column is text, character in R, and
`ValuePresent` and `IsNull` hold `"true"` or `"false"`. `Value` keeps an
absent item (missing, `ValuePresent` `"false"`), an explicit null (`IsNull`
`"true"`) and an empty value (`""`) apart. An `odm` expression reads eleven of
these columns; the
[binding contract](https://github.com/elong0527/yamaa/blob/main/rules/specification/binding.md#req-1266)
names them.

`archive_member` names the XML member to read from a TAR archive that holds
more than one. `max_archive_members`, `max_expanded_bytes` and `max_xml_depth`
bound the archive members, the expanded bytes and the element nesting; their
defaults are 10,000 members, 256 MiB and a depth of 64.

## Inputs, outputs and paths

Inputs can be CSV, Parquet, SAS datasets (`.sas7bdat`) or SAS transport v5/v8
files (`.xpt`). A SAS input declares its `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int}}`; the
[SAS source contract](https://github.com/elong0527/yamaa/blob/main/rules/storage/sas.md)
covers missing values, padding, encodings and dates. Outputs are CSV or
Parquet; Dataset-JSON comes from `submission()`.

A relative path resolves from the YAML file that writes it, including an
inherited layer. Set `relative_to: entry` on an input or on `output` to resolve
it from the specification you run instead, so a shared layer can read each
study's inputs and save beside each study:

```yaml
input:
  DM: {path: input/dm.csv, relative_to: entry}
output:
  path: output/adsl.csv
  relative_to: entry
  columns: [USUBJID]
```

On an input, `relative_to` covers its `path` and `schema`; on `output`, the
dataset and its report. Nothing is discovered: there is no project root or data
root to fall back on. The
[resource contract](https://github.com/elong0527/yamaa/blob/main/rules/storage/resources.md)
has the details.

Both packages apply the same fixed
[resource limits](../release.md#resource-limits), such as 10 million rows per
table and 10,000 reported findings, and the same portable numeric functions,
so results and refusals agree across languages.

## Project functions

A specification calls a project function by its logical name. The environment
file defines the function, its parameters, its result type and its tests, and
names the installed package function that implements it. Before a run that
calls functions reads any study data, the package checks the installed versions
against the environment's lock, when it declares one, and runs every test of
each called function. In R, a function receives and returns plain values:
character, double, logical, `Date` and UTC `POSIXct`. The
[project functions contract](https://github.com/elong0527/yamaa/blob/main/rules/operations/functions.md)
has the details.

## Check specification style

The Python package includes a style checker. Its findings are authoring advice,
separate from `check()`, and `--fix` rewrites layout without changing what a
specification means.

```bash
python -m yamaa.style study/adsl.yaml study/adae.yaml
python -m yamaa.style study/
python -m yamaa.style --fix study/adsl.yaml
python -m yamaa.style --ignore line_width study/
```

Findings print as `path:line:column: name message (REQ-NNNN)`. The exit status
is 0 when no finding remains, 1 when findings remain and 2 when an input cannot
be read. The
[style contract](https://github.com/elong0527/yamaa/blob/main/rules/specification/style.md)
lists the checks.

## Quick reference

| Task | R | Python |
|---|---|---|
| Build one dataset | `yamaa_spec(path)$run()$save()` | `yamaa.spec(path).run().save()` |
| Build one project dataset | `project$spec("DM")$run()$save()` | `project.spec("DM").run().save()` |
| Build every SDTM dataset | `project$run("sdtm")$save()` | `project.run("sdtm").save()` |
| Build every ADaM dataset | `project$run("adam")$save()` | `project.run("adam").save()` |
| One dataset of a project run | `runs[["ADSL"]]$output` | `runs["ADSL"].output` |
| Check without data | `spec$check()$issues` | `spec.check().issues` |
| See the resolved specification | `spec$resolve()` | `spec.resolve()` |
| Run report | `run$report` | `run.report` |
| define.xml and Dataset-JSON | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
