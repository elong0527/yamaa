---
id: storage/resources
title: Resource resolution
status: normative
---

# Resource resolution

## Requirements

### Declaring files

<a id="req-0772"></a>

**REQ-0772.** The caller explicitly supplies the entry file and any implementation
environment file. These caller arguments follow ordinary host path resolution.
Every document-declared relative path resolves from the file that writes it,
including an inherited layer or a producing specification. Composition retains
the original declaring file and written spelling. No project configuration is
discovered or read to select resource locations.

### The written form

<a id="req-0773"></a>

**REQ-0773.** A `project_path` is either **relative** -- one or more segments
separated by `/` -- or **rooted** -- a leading `/`, or one ASCII letter
followed by `:/`, and then one or more such segments. Path form and syntax are
decided before the filesystem is consulted. A malformed path fails validation.

<a id="req-0774"></a>

**REQ-0774.** An absolute local path may name any location permitted by the
operating system and the caller's sandbox. yamaa does not require that location
to lie under a project root or data root. This rule adds no URI or network
resource protocol.

<a id="req-0775"></a>

**REQ-0775.** A `project_path` must not contain a URI scheme. One ASCII
letter followed by `:/` is a drive path, not a scheme; every other `scheme:`
prefix is prohibited.

<a id="req-0776"></a>

**REQ-0776.** A `project_path` must not contain `\`.

<a id="req-0777"></a>

**REQ-0777.** A `project_path` must not contain an empty segment or trailing
separator. An empty path has one empty segment and is invalid.

<a id="req-0778"></a>

**REQ-0778.** A `..` segment climbs to the parent directory and a `.` segment
stays put. Relative and absolute paths may contain both. They resolve textually
before the filesystem is consulted. A relative path starts at the canonical
directory of its declaring file. Traversal may leave that directory without a
yamaa containment failure. A parent step at a filesystem root stays at that root.

<a id="req-0779"></a>

**REQ-0779.** A written path is ASCII under [Text values](../values/text.md), like every other
repository-authored value.

### Resolution and file kind

<a id="req-0780"></a>

**REQ-0780.** A relative `project_path` resolves only from the directory of
the file that writes it, as [Name binding](../specification/binding.md) and
[Source ingestion](ingestion.md) require. An inherited declaration keeps that
location. Rebasing in a resolved specification names the same location from the
entry file; it does not change the original declaring file or written spelling.
A rooted path names its absolute local location. A missing entry fails there.
There is no retry from the entry directory, a project root, a data root or the
process working directory.

<a id="req-0781"></a>

**REQ-0781.** Every resolved location has a physical directory **anchor** and
retained component witnesses. The caller's entry parent is canonicalized and
opened once; aliases the caller used for that selected directory retain the
same authority. A declaration outside that parent is walked from its filesystem
root. The anchor is an IO consistency mechanism, not an application containment
policy or fallback search location. Python and R use the same resolution and
native traversal rules on every supported platform.

<a id="req-1246"></a>

**REQ-1246.** A shared layer's `input/dm.csv` names the file beside that shared
layer. If that file is absent, a study's own `input/dm.csv` is not substituted.
The study must explicitly supply a declaration that names its own file.
`yamaa-project.yaml` and old project/data-root settings do not affect this
resolution. Missing-file diagnostics retain the original declaring field and
written path.

<a id="req-1247"></a>

**REQ-1247.** A path that names an artifact the run itself produces, through the
`schema` link of [Source ingestion](ingestion.md), denotes its declaration's
location even before that file exists. The producer's `output.path` must name
the same location. The producing specification that `schema` names already
exists and resolves from the file that declares that link. Workflow execution
consumes held serialized producer bytes; explicit Save publishes those bytes
under [Publication](publication.md).

<a id="req-0782"></a>

**REQ-0782.** Resolution walks the segments below the anchor one at a time from
that anchor. Every component except the last is a directory. The last
component is a regular file.

<a id="req-0783"></a>

**REQ-0783.** **No component below the anchor is a symbolic link**, including
one whose target is otherwise readable. The caller-selected entry parent is
exempt: its canonical directory and descriptor are selected before any
specification is read. Declared paths do not select exempt directories. A path
outside that parent retains the no-link check on every component below its
filesystem root. Each walk verifies the retained directory and file entries so
that a name substitution cannot redirect a read or publication silently.

<a id="req-0784"></a>

**REQ-0784.** The canonical resolved identity agrees with the directory
descriptor and component witnesses the walk accepted. The check fails closed
on an inconsistent witness. It does not enforce a project/data-root boundary.

<a id="req-0785"></a>

**REQ-0785.** A path that reaches no entry is missing. A path that reaches a
directory, FIFO, socket, device, or any other non-regular file is rejected.
Its bytes are not a stored dataset, and reading one can block or
consume a stream that cannot be read twice.

### One snapshot per physical file

<a id="req-0786"></a>

**REQ-0786.** A run reads one **immutable byte snapshot** of each accepted
physical file. Every declaration that reaches the file binds that snapshot. The
key is the file the walk reached: its canonical resolved path, the canonical
anchor, and the components the walk accepted. The key is never the spelling a
declaration used. A relative and a rooted spelling of one file therefore
produce one key and bind one snapshot.

<a id="req-0787"></a>

**REQ-0787.** Several dataset identifiers may resolve to one physical file. The
declarations remain distinct -- each carries its own identifier and its own
[Source ingestion](ingestion.md) field types -- but they share the one snapshot, so no two of them
observe different bytes.

<a id="req-0788"></a>

**REQ-0788.** An implementation reads the snapshot through the handle opened
while validating, or records the validated bytes and verifies that the bytes
are unchanged before ingestion. The implementation does not re-resolve the
written path and read its later target. Content that changes between validation
and ingestion fails the run; the replacement is not read.

<a id="req-0789"></a>

**REQ-0789.** Content identity is over bytes. A modification time, inode
number, or size is not the identity. None of them changes reliably
when content does.

### Interface behavior

<a id="req-1151"></a>

**REQ-1151.** The `project_path` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `project_path` | Local resource a run reads or publishes; a relative spelling starts at its original declaring file and an absolute spelling names its local location under [Resource resolution](resources.md). |

## Error conditions

<a id="req-0790"></a>

**REQ-0790.** A failure names the written path exactly as the specification
wrote it, the declaring field, and one condition below. A message does not
contain a canonical path, a symbolic link's target, or any other host path the
specification did not itself write. An absolute written path supplied by the
specification may be repeated. Resolved paths retained as provenance
are not substituted for the authored path in these diagnostics.

| Condition | Rejects |
|---|---|
| `resource_path_uri_scheme` | a URI scheme, including `file:` and `https:` |
| `resource_path_not_normalized` | a form [REQ-0776](resources.md#req-0776) through [REQ-0778](resources.md#req-0778) rejects |
| `resource_path_symlink` | a symbolic link at any component below the anchor |
| `resource_path_missing` | a path that reaches no entry at its declared location |
| `resource_path_not_regular_file` | a directory, FIFO, socket, or device |
| `resource_path_content_changed` | content that changed after validation |

`resource_path_not_relative` and `resource_path_outside_project` are historical
conditions from the retired containment policy. The replacement route emits
neither for an absolute path or parent traversal that leaves an entry directory.
Their old observations remain in the reference qualification record.

<a id="req-0791"></a>

**REQ-0791.** The written-form conditions are decided in the order [REQ-0773](resources.md#req-0773)
through [REQ-0778](resources.md#req-0778) state them, then the anchor of [REQ-0781](resources.md#req-0781), then the walk, so one
written path reports one condition on every platform.

<a id="req-0792"></a>

**REQ-0792.** Every condition above is decided before any data is read and
reports under the `validation` phase, except `resource_path_content_changed`,
which reports under `ingest`.
