# R and Python API

yamaa has the same small API in R and Python. Two objects mirror the two kinds
of file you write: a **spec** is one specification, which builds one dataset,
and a **project** is a study, which lists its specifications and holds the
metadata they share. Methods on these objects check, resolve, run and package.
Only a result writes files, and always through `save()`.

Every example on this page has an R tab and a Python tab. R is shown first;
pick a language once and every example on the page follows.

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
checks and `output`. `output.path` names the dataset file, and `output.report`
names the run report file.

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

A section needs only `specs` to run. `standard`, `define`, `datasets` and the
`study` block are needed only to create submission files, so a project can
list its specifications before its submission metadata is ready.

An **environment** (`environment.yaml`) holds the code one language installs:
an optional lock file (`uv.lock` for Python, `renv.lock` for R) and the project
functions your specifications call. Write one per language; the project's
`environments` map says which file belongs to which language.

```yaml
schema_version: "1.0"
lock: ../uv.lock
functions:
  bmi: functions/bmi.yaml
```

## Run one specification

`yamaa_spec()` and `yamaa.spec()` read nothing. `run()` builds the dataset in
memory and writes nothing; `save()` writes the dataset to `output.path` and the
run report to `output.report`.

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
you call `run()`. A result is a fixed snapshot: saving it writes exactly what
that run built.

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
is. R returns a named list and Python a dict.

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
across the project. An unknown name fails with the list of names the project
has.

When a specification reads the output of another specification in the same
section, the project builds that one first, in memory: running ADAE builds ADSL
before it. Inputs from another section are read from their saved files.

## Run a whole standard

=== "R"

    ```r
    project$run("sdtm")$save()    # every SDTM dataset
    project$run("adam")$save()    # every ADaM dataset
    runs <- project$run()         # everything, SDTM before ADaM
    runs[["ADSL"]]$output
    runs$report
    runs$save()
    ```

=== "Python"

    ```python
    project.run("sdtm").save()    # every SDTM dataset
    project.run("adam").save()    # every ADaM dataset
    runs = project.run()          # everything, SDTM before ADaM
    runs["ADSL"].output
    runs.report
    runs.save()
    ```

The argument names a section of `project.yaml`: `"sdtm"`, `"adam"` or `"send"`.
A run builds each of its specifications once, in dependency order, and hands
each result in memory to the specifications that read it. Inputs from outside
the chosen section are read from their saved files, so you can rerun ADaM
against finished SDTM without rebuilding it. A project run's `report` and
`issues` combine those of its runs.

## Create define.xml and submission datasets

`submission()` composes one define.xml per section from the project's metadata.
Without `datasets`, it writes define.xml alone and reads no data, so you can
draft define.xml before any data exists. With `datasets: json`, it also writes
each saved dataset as Dataset-JSON next to define.xml, so every link in
define.xml names a file that exists. Run and save the section first.

=== "R"

    ```r
    project$submission("sdtm")$save()
    project$submission()$save()     # every section
    ```

=== "Python"

    ```python
    project.submission("sdtm").save()
    project.submission().save()     # every section
    ```

## What a result holds

| Member | Holds |
|---|---|
| `output` | The dataset: the columns in `output.columns`, with rows in `output.order_by` order. A data.frame in R and a Polars DataFrame in Python. `NULL` or `None` when the run failed. |
| `issues` | What is wrong, one row per finding: `phase`, `condition`, `requirement`, `spec_paths`, `context`, `source_span` and `operand_route`. Empty when nothing is wrong. |
| `report` | What the run read, counted and checked: one row for the run, then one row per input, row template, handler, output column and evaluated check. `save()` writes it to `output.report`, for a failed run too. |

`issues` tells you what to fix; `report` is the evidence of what was checked. A
failed error-level check appears in both: in `issues` as the reason the run
failed, and in `report` as a failed check. Warnings appear only in `report`.

R columns hold plain values: integers arrive as doubles and text as character.
A value R cannot hold exactly, an integer beyond 2^53 or text containing NUL,
raises an error instead of changing.

## When something fails

`check()` and `run()` never raise for problems in a specification or its data;
they return them in `issues`. `save()` and `resolve()` raise when they cannot
do their work, and the error carries the same `issues`.

=== "R"

    ```r
    tryCatch(
      run$save(),
      yamaa_error = function(e) print(e$issues)
    )
    ```

=== "Python"

    ```python
    try:
        run.save()
    except yamaa.Error as e:
        print(e.issues)
    ```

## Read ODM XML

Specifications read tables, not ODM XML. Read the ODM file into a data frame
once and save it where your specifications' `input` reads it.

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

## Quick reference

| Task | R | Python |
|---|---|---|
| Build one dataset | `yamaa_spec(path)$run()$save()` | `yamaa.spec(path).run().save()` |
| Build one project dataset | `project$spec("DM")$run()$save()` | `project.spec("DM").run().save()` |
| Build every SDTM dataset | `project$run("sdtm")$save()` | `project.run("sdtm").save()` |
| Build every ADaM dataset | `project$run("adam")$save()` | `project.run("adam").save()` |
| Check without data | `spec$check()$issues` | `spec.check().issues` |
| See the resolved specification | `spec$resolve()` | `spec.resolve()` |
| Verification report | `run$report` | `run.report` |
| define.xml and Dataset-JSON | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
