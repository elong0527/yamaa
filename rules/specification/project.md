---
id: specification/project
title: Project file
status: normative
---

# Project file

## Requirements

### The project file

<a id="req-1331"></a>

**REQ-1331.** A **project file** names one study's implementation
environments, terminology and submission sections. The caller passes it
explicitly, and it is admitted against `schema_project.yaml`, whose
`schema_version` selects the schema bundle. No project file is discovered
from a specification, a directory or a host default, and a specification
cannot select, replace or override its project. A run without a project file
has no implementation environment, no terminology and no dataset inventory.

### Environments

<a id="req-1332"></a>

**REQ-1332.** `environments` maps a host language key, `python` or `r`, to
that language's environment file under
[Project functions](../operations/functions.md). The key states the language
of the file it names, and an environment file declares no language of its
own. Each host reads only the entry its own key names; the other entry is
neither opened nor validated, so one project serves both hosts with one copy
of its study metadata. An absent entry leaves that host without an
implementation environment, and a specification that calls a project
function then cannot execute under
[REQ-0694](../operations/functions.md#req-0694).

### Study metadata

<a id="req-1333"></a>

**REQ-1333.** `codelists` supplies the study terminology
[Controlled terminology](../submission/terminology.md) owns, `study` names the
study, and each `sdtm`, `adam` or `send` section lists the specifications of
that family with the submission metadata
[Define-XML](../submission/define-xml.md) composes. A section needs only
`specs`. `study` and the section's `standard` and `define` are required only
when Define-XML composes the section, under
[REQ-1337](../submission/define-xml.md#req-1337), so a project can list its
specifications before its submission metadata is ready. A run does not open a
section's document links or generate a Define-XML document.

### Paths

<a id="req-1334"></a>

**REQ-1334.** Every relative path the project file writes, namely each
environment link, codelist source, section specification and Define-XML
target, resolves from the directory of the project file. Every relative path
an environment file writes, namely its lock and its function definition
files, resolves from that environment file. Absolute local paths and parent
segments have their [Resource resolution](../storage/resources.md) meaning.
An absent file fails at its one selected location, without discovery or a
fallback search. The project file does not change where a specification's own
resources resolve under [REQ-0772](../storage/resources.md#req-0772).

### Specification names

<a id="req-1338"></a>

**REQ-1338.** The **name** of a project specification is the `domain` value of
a specification that a section lists in `specs`. Only listed specifications
have names: a producer that a `schema` link reaches without a listing has none.
Names are unique across the project, every section included. Two listings
with one name, one file listed twice included, fail with
`duplicate_specification_name` at the project path of each listing, such as
`sdtm.specs[0]`. Every project operation reports that finding; a project run
or submission then builds nothing, and selecting a specification by name
fails.

<a id="req-1339"></a>

**REQ-1339.** Selecting a project specification by name accepts only a name
[REQ-1338](#req-1338) defines. An unknown name fails with
`unknown_specification_name`, naming the requested name and, in sorted order,
every name the project defines. A selected specification is checked, run and
resolved exactly as it is when the caller names its file together with the
project file.

### Checking a project

<a id="req-1340"></a>

**REQ-1340.** Checking a project statically checks every listed
specification, in section order (`sdtm`, `adam`, `send`) and then listing
order, exactly as checking that specification with the project file does:
with the project's terminology and implementation environment, and without
study data or project code. Each finding names the listed specification it
belongs to in its `entry` context and the document it comes from in its
`source` context. The check also reports the findings of
[REQ-1338](#req-1338), those of [REQ-1344](#req-1344) with every listed
specification in scope, and the collisions
[REQ-0756](../storage/publication.md#req-0756) finds among the files of every
listed specification.

### Project runs

<a id="req-1341"></a>

**REQ-1341.** A project run builds the specifications that one selected
section lists, or those of every declared section in the order `sdtm`, `adam`,
`send`. These specifications are the run's **run scope**. Each builds exactly
once, as its own run with its own output, findings and run report, and the run
reports it under its name.

<a id="req-1342"></a>

**REQ-1342.** In a project run, a `schema` link to a run-scope specification is
a dependency: the producer runs first, and the consumer reads the producer's
held serialized artifact under [REQ-1247](../storage/resources.md#req-1247)
rather than its file. Specifications run producer first; independent
specifications keep run-scope order. A dependency cycle among run-scope
specifications fails each specification on it with `producer_workflow_cycle`.

<a id="req-1343"></a>

**REQ-1343.** In a project run, a `schema` link to a specification outside the
run scope does not rebuild that producer. The consumer reads the file its
`path` names, the producer's saved artifact, and checks it against the
producer's output contract under [REQ-0522](../storage/ingestion.md#req-0522)
and [REQ-0535](../storage/ingestion.md#req-0535). An absent file fails with
`resource_path_missing`. The consumer may not write the files it reads:
[REQ-0756](../storage/publication.md#req-0756) compares its `output.path` and
`output.report` locations with the producer's, as it would for a producer the
consumer builds.

<a id="req-1344"></a>

**REQ-1344.** An input without `schema` whose resolved `path` names the
`output.path` location of a run-scope specification fails with
`unlinked_producer`, naming that specification. Without the link the input
would read the artifact an earlier run saved, not the one this run builds. An
input that names no run-scope artifact is an ordinary source.

<a id="req-1345"></a>

**REQ-1345.** A run-scope specification that fails does not stop the
specifications that do not depend on it. A consumer whose run-scope producer
failed or did not run does not run: it fails with `producer_not_completed`,
naming that producer, and builds no output and no run report. Every
specification that ran keeps its own output, findings and run report.

### Saving a project run

<a id="req-1346"></a>

**REQ-1346.** [REQ-0756](../storage/publication.md#req-0756) compares the
`output.path` and `output.report` locations of every run-scope specification,
and a collision fails the project run before any specification builds. Saving
a project run validates every target before touching any of them, then
publishes specification by specification in run order. When every
specification succeeded, each publishes its declared report immediately
before its primary artifact, as [REQ-0757](../storage/publication.md#req-0757)
orders the files of one specification. When any specification failed, the
save publishes no primary artifact: it replaces, in run order, the report file
of every specification that ran and declares one, then fails with the
findings of every run. A failure while replacing one file stops the
publication there. A project run that built no specification has nothing to
save, and saving it fails.

### Project submissions

<a id="req-1347"></a>

**REQ-1347.** A project submission composes one
[Define-XML](../submission/define-xml.md) document for the selected section,
or one for each declared section that names `define`, in the order `sdtm`,
`adam`, `send`, together with the [Dataset-JSON](../submission/dataset-json.md)
files of a section that sets `datasets: json`. Its findings are those of its
sections in that order. Saving validates the targets of every section first,
and two sections that name one file fail the save before any file is written.
It then publishes the sections in that order; the first section whose
publication fails stops the save. A submission that composed no section has
nothing to save, and saving it fails.

### Interface behavior

<a id="req-1335"></a>

**REQ-1335.** The `project_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `project_class.schema_version` | Schema bundle the project file is written against; [Schema language](../reference/schema-language.md) requires an exact match. |
| `project_class.environments` | Environment file of each host language, by language key. |
| `project_class.codelists` | Study terminology sources, inline or by path; [Controlled terminology](../submission/terminology.md) defines them. |
| `project_class.study` | Study the specifications belong to; Define-XML requires it. |
| `project_class.sdtm` | Tabulation specifications and their submission metadata. |
| `project_class.adam` | Analysis specifications and their submission metadata. |
| `project_class.send` | Nonclinical tabulation specifications and their submission metadata. |

<a id="req-1336"></a>

**REQ-1336.** The `environments_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `environments_class.python` | Environment file the Python host reads. |
| `environments_class.r` | Environment file the R host reads. |

## Error conditions

<a id="req-1348"></a>

**REQ-1348.** Each project finding below reports under the `validation` phase
and is decided before the specification it concerns reads study data.
`duplicate_specification_name` names the project path of each listing, and
`unlinked_producer` and `producer_not_completed` name the input field of the
consumer they fail:

| Condition | Rejects |
| --- | --- |
| `duplicate_specification_name` | two listings whose specifications share one name ([REQ-1338](#req-1338)) |
| `unknown_specification_name` | a name that no listed specification has ([REQ-1339](#req-1339)) |
| `unlinked_producer` | an input without `schema` that reads a run-scope artifact ([REQ-1344](#req-1344)) |
| `producer_not_completed` | a consumer whose run-scope producer failed or did not run ([REQ-1345](#req-1345)) |
