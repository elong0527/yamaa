# R and Python API

yamaa has the same small API in R and Python. Use it to create, verify and save
SDTM and ADaM datasets and define.xml files.

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
| define.xml and transport files | `project$submission("sdtm")$save()` | `project.submission("sdtm").save()` |
| Read ODM XML | `yamaa_read_odm(path)` | `yamaa.read_odm(path)` |
| Lint specifications | `yamaa_lint(paths)` | `yamaa.lint(paths)` |

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

For example, `sdtm/dm.yaml` builds SDTM DM, one row per subject, from the ODM
table saved in [Read ODM XML](#read-odm-xml).

```yaml
# sdtm/dm.yaml
schema_version: "1.0"
domain: DM
keys: [STUDYID, USUBJID]
input:
  ODM: ../raw/odm.parquet

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
    derivation: {literal: DM}

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
          then: {literal: YEARS}
        - otherwise: {literal: null}
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
    run$output    # generated dataset
    run$issues    # identified errors
    run$report    # run report
    run$save()    # save run report and dataset; returns their paths
    ```

=== "Python"

    ```python
    spec = yamaa.spec("sdtm/dm.yaml")
    run = spec.run()
    run.output    # generated dataset
    run.issues    # identified errors
    run.report    # run report
    run.save()    # save run report and dataset; returns their paths
    ```

## Check a specification without data

`check()` reports problems in a specification without reading study data or
running project code. It does not open the input datasets, so some problems, such
as a column name that no input has, appear only when you call `run()`.

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
is. A producing specification linked through `schema` is a resource of its
consumer, not a separate entry. R returns a named list and Python a dict. It
reads no study data and runs no project code.

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

## Project execution

`run()` builds every specification in the section you name, `"sdtm"`, `"adam"`
or `"send"`, in dependency order. Without a name, it builds every section. As
with `spec()`, a later `run()` reuses what earlier runs of the same project
object compiled, and compiles again any specification whose files changed.
Its `save()` writes each specification's run report and dataset in run order,
or only the run reports when any specification failed, and returns the paths it
wrote. It checks every target before it writes anything.

=== "R"

    ```r
    project$run("sdtm")$save()    # all SDTM datasets
    project$run("adam")$save()    # all ADaM datasets
    runs <- project$run()         # all SDTM and ADaM datasets
    runs$names                    # the specifications, in run order
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
returns the paths it wrote.

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

Specifications read tables, not ODM XML. `read_odm()` reads an ODM 1.3 or 2.0
XML file into one row per clinical item in file order. Save the table where your
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

## Lint specifications

`lint()` checks specification files against the
[style guide](style.md) and returns one row per finding: the file, the line
and column where the departure starts, the finding, its spec path and a
message. A directory names the specification files beneath it. With `fix`,
it first applies the proved layout fixes, replacing a file only when it reads
as the same values with the same comments, and returns what remains.

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

## Inputs, outputs and paths

Inputs can be CSV, Parquet, SAS datasets (`.sas7bdat`) or SAS transport v5/v8
files (`.xpt`). A SAS input declares its `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int}}`. Outputs are
CSV or Parquet; Dataset-JSON and SAS Transport version 5 files come from
`submission()`.

A relative path resolves from the YAML file that writes it, so
`../raw/odm.parquet` in `sdtm/dm.yaml` is the study's `raw/odm.parquet`.

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
