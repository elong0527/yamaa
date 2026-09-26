---
id: storage/resources
title: Resource resolution
status: normative
---

# Resource resolution

## Requirements

### The approved roots

<a id="req-0767"></a>

**REQ-0767.** A run receives exactly one **approved project root**: an existing
local directory fixed for the whole run. The root is the directory holding the
**project configuration** of [REQ-0768](resources.md#req-0768). A run whose entry file sits under no
project configuration, and whose runner names no root, uses the directory
holding the entry file. A runner may name the root.

<a id="req-0768"></a>

**REQ-0768.** A **project configuration** is one `yamaa-project.yaml` file. A run
finds it by walking up from the entry file to the first directory that holds
one, and reads it once, before any specification. The file's directory is the
project root. A runner that names the root takes only the configuration at
that root.

<a id="req-0769"></a>

**REQ-0769.** A run may also receive **approved data roots**: existing local
directories that a rooted path may name. They come from the project
configuration's `data_roots`, from the runner, or from both. Without approved
data roots, only the project root is approved. Every approved root is
canonicalized and opened when selected; later changes to its parent path do
not redirect reads.

<a id="req-0770"></a>

**REQ-0770.** The approved roots are fixed before any specification is read. **No
specification field, no layer [Specification composition](../specification/composition.md) reaches, and no value a specification reads
contributes to the roots.** Only the entry project's own configuration and the
runner do. A producing specification reached through `dataset_class.schema`
uses the same approved roots, never its own configuration. When
[Project functions](../operations/functions.md) also selects a project root
for an implementation environment, it must be the same directory; a mismatch
fails before code activation. An approved data root holds readable data, not
activatable code.

<a id="req-0771"></a>

**REQ-0771.** A runner bounds what a project configuration may approve. A runner
that names data roots makes them the **ceiling**: each root the configuration
declares resolves inside one of them, or the run fails before it reads a
specification. A runner may also decline the configuration's data roots
entirely; every source then resolves inside the project root.

<a id="req-0772"></a>

**REQ-0772.** The entry file resolves inside the approved project root. A layer
[Specification composition](../specification/composition.md) reaches contributes declarations that are read against the same approved
roots. A layer stored outside the project root contributes a `project_path`
that [Specification composition](../specification/composition.md) rebases to the entry file, and the rebased form must satisfy this
contract. A relative path from such a layer is read from the layer's
directory only when that location resolves inside an approved root; otherwise
the approved project root is its first anchor under
[REQ-0781](resources.md#req-0781). A rooted path it writes is decided against
the approved roots like any other.

### The written form

<a id="req-0773"></a>

**REQ-0773.** A `project_path` is either **relative** -- one or more segments
separated by `/` -- or **rooted** -- a leading `/`, or one ASCII letter
followed by `:/`, and then one or more such segments. Path form and syntax are
decided before the filesystem is consulted. A malformed path fails validation.

<a id="req-0774"></a>

**REQ-0774.** A rooted path may name a host location only inside an approved
root under [REQ-0781](#req-0781).

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
  stays put. A relative path may contain both. Both resolve textually
  before the filesystem is consulted. A traversal that stays inside the
  approved project root names one file by one spelling. The canonical
  resolved path below is the snapshot identity. A traversal that climbs
  above the anchor keeps resolving textually against the anchor's canonical
  segments and is re-anchored at the approved root the resolved location sits
  under ([REQ-0781](resources.md#req-0781)), so an explicitly approved data
  root is reachable by a relative spelling. A traversal that resolves inside
  no approved root fails as `resource_path_outside_project`. An escape fails
  identically on every platform whether or not anything exists where it
  points. A rooted path writes neither. It already names its location;
  a dot segment there would only obscure which approved root it names.

<a id="req-0779"></a>

**REQ-0779.** A written path is ASCII under [Text values](../values/text.md), like every other
repository-authored value.

### Resolution and file kind

<a id="req-0780"></a>

**REQ-0780.** A relative `project_path` resolves relative to the directory of
the layer that writes it, as [Name binding](../specification/binding.md) and [Source ingestion](ingestion.md) require. In a resolved
specification it is relative to the entry file. [Specification composition](../specification/composition.md) has already
rebased it, and the rebased form names that same location. When that
resolution reaches no entry, the run retries the path exactly as its layer
wrote it: first against the approved project root, then against each approved
data root in run order, the first success winning. The retry needs the layer's
own spelling, which the rebased form no longer shows, so it is taken from the
provenance [Specification composition](../specification/composition.md) keeps. A rooted `project_path` resolves against the approved root it
names and is unaffected by rebasing, which leaves it exactly as written.
This contract reads that written form.

<a id="req-0781"></a>

**REQ-0781.** Every resolution has an **anchor**. A relative path is anchored
first at the approved root that the location it names from the writing
layer's directory sits under -- the approved project root when the layer is
inside it. If that location is inside no approved root and the writing layer's
directory is outside every approved root too, it is not an anchor. The
run reads nothing there, and the path begins at the next anchor. Only a
resolution that reaches no entry advances to the next anchor: the approved
project root, then the approved data roots in run order. A later anchor that
the written path climbs out of is skipped. Any other condition -- a symlink, a
non-regular file, a traversal from a writing directory inside an approved root
that leaves every approved root, a rejected written form -- is terminal. The
writing layer's directory therefore wins when the entry exists under both it
and another anchor, the project root wins over every data root, and the first
declared data root wins among data roots. A traversal that climbs above an
anchor's root is re-anchored at the approved root whose canonical leading segments the
resolved location repeats, the longest match winning when one approved root
lies inside another, exactly as for a rooted path. A traversal that resolves
inside no approved root fails as `resource_path_outside_project`: it reaches
no location this run approved. A rooted path is anchored at the approved root
whose leading segments it repeats -- either the spelling the runner used or
that root's canonical spelling, compared segment by segment before the
filesystem is consulted, the longest match winning when one approved root lies
inside another. A rooted path that repeats no approved root's leading segments
names no location this run approved and fails as `resource_path_not_relative`:
it is relative to nothing the runner allowed.

<a id="req-1246"></a>

**REQ-1246.** The approved project root is the anchor after the writing
layer's directory, so a layer shared by several studies can declare an input
dataset once -- its identifier, field types, and empty-string convention --
while each study keeps the stored file under its own project root. A study
whose project configuration sits in its own directory makes that directory its
project root. A shared layer's `input/dm.csv` that is absent beside the shared
layer then reads that study's `input/dm.csv`. No specification field chooses
an anchor, and every anchor is an approved root fixed under
[REQ-0770](resources.md#req-0770), so the retry never reaches a location the
run did not approve.

<a id="req-1247"></a>

**REQ-1247.** A relative `path` that names an artifact the run itself
produces, through the `schema` link of [Source ingestion](ingestion.md),
denotes the location of its first anchor. That file does not exist until its
producer publishes it, so there is no missing entry to retry, and the
producer's `output.path` must publish to that location. The producing
specification that `schema` names already exists, and it resolves like any
other relative path.

<a id="req-0782"></a>

**REQ-0782.** Resolution walks the segments below the anchor one at a time from
that anchor. Every component except the last is a directory. The last
component is a regular file.

<a id="req-0783"></a>

**REQ-0783.** **No component below the anchor is a symbolic link**, including
one whose target is inside an approved root. A link is a second name for a
file. A boundary that admits a link must re-derive containment every time
the link changes. The link a validator followed is not necessarily the
link a reader follows. The anchor itself is exempt. It is not a name the
specification chose: the runner selected it, the run canonicalized and opened
it before reading any specification, and every walk begins at that open
directory, so no name above the anchor can be swapped between validation and
ingestion. This is the boundary the approved project root has always drawn --
its own canonical form is taken once and the no-link rejection begins below
it -- so a rooted path moves the anchor without moving the boundary. A run
therefore treats a location spelled through a linked system directory
like any other approved root.

<a id="req-0784"></a>

**REQ-0784.** After the walk, the canonical resolved file is inside the
canonical anchor. The written-form checks and symbolic-link rejection already
imply this. The check must fail closed rather than silently pass a boundary
defect.

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
| `project_path` | Stored resource a run reads; [Resource resolution](resources.md) fixes its written form and confines it to a root the runner approved. |

## Error conditions

<a id="req-0790"></a>

**REQ-0790.** A failure names the written path exactly as the specification
wrote it, the declaring field, and one condition below. A message does not
contain an approved root, a canonical path, a symbolic link's target, or any
other host path the specification did not itself write. A rooted written path
is a host path, but it is the one the specification supplied, so repeating it
discloses nothing its writer did not already state. The approved roots stay
unnamed even when the failure is that a path named none of them. Those
are the values a rejected specification is probing for.

| Condition | Rejects |
|---|---|
| `resource_path_uri_scheme` | a URI scheme, including `file:` and `https:` |
| `resource_path_not_normalized` | a form [REQ-0776](resources.md#req-0776) through [REQ-0778](resources.md#req-0778) rejects |
| `resource_path_not_relative` | a rooted path that names no approved root |
| `resource_path_outside_project` | a traversal that resolves inside no approved root |
| `resource_path_symlink` | a symbolic link at any component below the anchor |
| `resource_path_missing` | a path that reaches no entry |
| `resource_path_not_regular_file` | a directory, FIFO, socket, or device |
| `resource_path_content_changed` | content that changed after validation |

<a id="req-0791"></a>

**REQ-0791.** The written-form conditions are decided in the order [REQ-0773](resources.md#req-0773)
through [REQ-0778](resources.md#req-0778) state them, then the anchor of [REQ-0781](resources.md#req-0781), then the walk, so one
written path reports one condition on every platform.

<a id="req-0792"></a>

**REQ-0792.** Every condition above is decided before any data is read and
reports under the `validation` phase, except `resource_path_content_changed`,
which reports under `ingest`.

<a id="req-0793"></a>

**REQ-0793.** An approved root that does not exist, is not a directory, or,
  for the approved project root, differs from an [Project functions](../operations/functions.md) project root: fail.

<a id="req-0794"></a>

**REQ-0794.** An entry file outside the approved project root: fail.

<a id="req-0795"></a>

**REQ-0795.** A project configuration that is not a mapping, carries an
  unknown field, declares a `data_roots` that is not a list of existing local
  directories, or declares one outside a ceiling the runner named: fail before
  any specification is read. A malformed configuration is a run that was never
  configured, not a run with fewer roots.
