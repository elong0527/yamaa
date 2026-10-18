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
