---
id: R021
title: Project Resource Resolution
status: normative
applies_to: [project_path, dataset_source, dataset_class]

---

# Project resource resolution

## Intent

Bound every stored file a run reads to a location its runner approved, and
bind the bytes a validated declaration names to the bytes ingestion receives.

## Boundaries

This rule owns the approved roots, the written form of a `project_path`, the
file kinds a run may read, the content identity that carries from validation
to ingestion, and the errors these produce.

R002 owns dataset declaration and how a name resolves to a value. R014 owns
what a stored field becomes once its bytes are available, including the
producing-specification link. Both resolve a `project_path` here. R017 owns
layer traversal and the rebasing that fixes a resolved specification's written
paths. R006 owns schema notation and structural validation, including its own
`includes`. R019 owns the text a written path contains. R018 owns the runtime
artifact a project function executes and the digest that identifies it.

This rule reaches only the files a run reads. R020 owns `output.path` and the
artifact a run writes.

This rule does not decide which datasets a specification declares, in which
order a run reads them, or what a runner does with a rejected run. It does not
govern `parents` or schema `includes`.

## The approved roots

**R021-1.** A run receives exactly one **approved project root**: an existing
local directory the runner selects before it reads any specification. A runner
that selects none uses the directory holding the entry file. The root is fixed
for the whole run.

**R021-2.** A run may also receive **approved data roots**: existing local
directories the runner selects, before it reads any specification, as the
locations a rooted path may name. They are how one organization keeps code and
data apart -- the project root holds the specifications a study is reviewed
from, an approved data root holds stored data the study reads. A runner that
selects none leaves the approved project root as the only approved root. Every
approved root is fixed for the whole run, and is canonicalized and opened when
it is selected, so nothing that happens to a name above it afterwards moves
what the run reads.

**R021-3.** No field of a specification, and no value a specification reads,
names, replaces, extends, or widens an approved root. A rooted path selects
among the roots a runner already approved; it never adds one, so what a run may
read is decided outside the specification and before the specification is read.
When R018 also selects a project root for an implementation environment, that
root and the approved project root are the same directory; a difference fails
before code activation. An approved data root holds data a run reads, never
code it activates.

**R021-4.** The entry file resolves inside the approved project root. A layer
R017 reaches contributes declarations that are read against the same approved
roots, so composition never enlarges what a run may read. A layer stored
outside the project root contributes a `project_path` that R017 rebases to the
entry file, and the rebased form must satisfy this rule; a relative path from
such a layer therefore cannot reach a readable source, while a rooted path it
writes is decided against the approved roots like any other.

## The written form

**R021-5.** A `project_path` is either **relative** -- one or more segments
separated by `/` -- or **rooted** -- a leading `/`, or one ASCII letter
followed by `:/`, and then one or more such segments. Which form a path is
written in, and whether that form is well formed, is decided before the
filesystem is consulted, so a malformed path fails identically on every
platform and reveals nothing about the host.

- **R021-6.** A rooted path names a host location outright, and that is
  allowed. Code and data are commonly stored apart, and an absolute path is
  how a study connects them. It is not allowed unconditionally: R021-13 still
  requires it to name an approved root, so the run reads no more than the
  runner approved. Portability is a property a submission package must have
  and a packaging step enforces; it is not a property every intermediate study
  layout has.
- **R021-7.** No URI scheme. A specification declares stored files;
  retrieval, caching, and authentication are not part of a derivation. One
  ASCII letter followed by `:/` is a drive, not a scheme, because that is how
  one platform spells a rooted location; every other `scheme:` prefix is a
  scheme, a drive letter followed by anything else included.
- **R021-8.** No `\` anywhere. A backslash is an ordinary filename character
  on one platform and a separator on another, so a path containing one
  denotes two different files.
- **R021-9.** No empty segment and no trailing separator. Each is a
  misspelling with no legitimate layout behind it, and an empty path is one
  empty segment.
- **R021-10.** A `..` segment climbs to the parent directory and a `.` segment
  stays put. A relative path may write both, and both resolve textually
  before the filesystem is consulted: a traversal that stays inside the
  approved project root names one file by one spelling, because the canonical
  resolved path below is the snapshot identity, and a traversal that climbs
  above the entry directory's depth within that root fails as
  `resource_path_outside_project`, so an escape fails identically on every
  platform whether or not anything exists where it points. A rooted path
  writes neither, because it already names its location and a dot segment
  there would only obscure which approved root it names.

**R021-11.** A written path is ASCII under R019, like every other
repository-authored value.

## Resolution and file kind

**R021-12.** A relative `project_path` resolves relative to the directory of
the layer that writes it, as R002 and R014 require. In a resolved
specification it is relative to the entry file, because R017 has already
rebased it. A rooted `project_path` resolves against the approved root it
names and is unaffected by rebasing, which leaves it exactly as written,
because this rule reads that written form.

**R021-13.** Every resolution has an **anchor**. A relative path is anchored
at the approved project root. A rooted path is anchored at the approved root
whose leading segments it repeats -- either the spelling the runner used or
that root's canonical spelling, compared segment by segment before the
filesystem is consulted, the longest match winning when one approved root lies
inside another. A rooted path that repeats no approved root's leading segments
names no location this run approved and fails as `resource_path_not_relative`:
it is relative to nothing the runner allowed.

**R021-14.** Resolution walks the segments below the anchor one at a time from
that anchor. Every component except the last is a directory. The last
component is a regular file.

**R021-15.** **No component below the anchor is a symbolic link**, including
one whose target is inside an approved root. A link is a second name for a
file, so a boundary that admits one must re-derive containment every time the
link changes, and the link a validator followed is not necessarily the link a
reader follows. The anchor itself is exempt because it is not a name the
specification chose: the runner selected it, the run canonicalized and opened
it before reading any specification, and every walk begins at that open
directory, so no name above the anchor can be swapped between validation and
ingestion. This is the boundary the approved project root has always drawn --
its own canonical form is taken once and the no-link rejection begins below
it -- so a rooted path moves the anchor without moving the boundary. A run
therefore decides a location a platform spells through a linked system
directory the same way it decides any other approved root.

**R021-16.** After the walk, the canonical resolved file is inside the
canonical anchor. The written form and the symbolic-link rejection already
imply this; the check is stated because a boundary defect must fail closed
rather than silently.

**R021-17.** A path that reaches no entry is missing. A path that reaches a
directory, FIFO, socket, device, or any other non-regular file is rejected,
because its bytes are not a stored dataset and reading one can block or
consume a stream that cannot be read twice.

## One snapshot per physical file

**R021-18.** A run reads one **immutable byte snapshot** of each physical file
it accepts, and every declaration that reaches that file binds that snapshot.
The key is the file the walk reached -- its canonical resolved path, the
canonical anchor followed by the components the walk accepted -- and never the
spelling a declaration used. A relative and a rooted spelling of one file
therefore produce one key and bind one snapshot.

**R021-19.** Several dataset identifiers may resolve to one physical file. The
declarations remain distinct -- each carries its own identifier and its own
R014 field types -- but they share the one snapshot, so no two of them
observe different bytes.

**R021-20.** An implementation reads the snapshot through the handle it opened
while validating, or records the SHA-256 of the bytes it validated and
verifies that digest before ingestion. It does not re-resolve the written path
and read whatever that path then names. Content that changed between
validation and ingestion fails the run; the replacement is not read.

**R021-21.** Content identity is over bytes. A modification time, inode
number, or size is not the identity, because none of them changes reliably
when content does.

## Rationale

The written-form checks run before the filesystem is consulted so that a
malformed path fails identically on every platform and reveals nothing about
the host. A rooted path is admitted because study code and study data are
commonly stored apart, but it is admitted as a selection among roots the
runner approved rather than as permission to read the host, so R021 still
confines a run to locations decided outside the specification. One key per
canonical file keeps the single-snapshot identity meaningful: the two spellings
of one file bind one snapshot instead of observing different bytes. Symbolic
links are rejected below the anchor because a link there is a second name
whose target can change between validation and ingestion, and they are
irrelevant above it because the anchor is canonicalized and opened before any
specification is read and cannot be swapped afterwards. Error messages name
only what the specification itself wrote for the same reason the checks run
before consulting the filesystem: a rejected specification may be probing for
host layout.

## Errors

**R021-22.** A failure names the written path exactly as the specification
wrote it, the declaring field, and one condition below. A message does not
contain an approved root, a canonical path, a symbolic link's target, or any
other host path the specification did not itself write. A rooted written path
is a host path, but it is the one the specification supplied, so repeating it
discloses nothing its writer did not already state. The approved roots stay
unnamed even when the failure is that a path named none of them, because those
are the values a rejected specification is probing for.

| Condition | Rejects |
|---|---|
| `resource_path_uri_scheme` | a URI scheme, including `file:` and `https:` |
| `resource_path_not_normalized` | a backslash, an empty segment, a trailing separator, an empty path, or a dot segment in a rooted path |
| `resource_path_not_relative` | a rooted path that names no approved root |
| `resource_path_outside_project` | a path that resolves outside its anchor |
| `resource_path_symlink` | a symbolic link at any component below the anchor |
| `resource_path_missing` | a path that reaches no entry |
| `resource_path_not_regular_file` | a directory, FIFO, socket, or device |
| `resource_path_content_changed` | content that changed after validation |

**R021-23.** The written-form conditions are decided in the order R021-7
through R021-10 state them, then the anchor of R021-13, then the walk, so one
written path reports one condition on every platform.

**R021-24.** Every condition above is decided before any data is read and
reports under the `validation` phase, except `resource_path_content_changed`,
which reports under `ingest`.

- **R021-25.** An approved root that does not exist, is not a directory, or,
  for the approved project root, differs from an R018 project root: fail.
- **R021-26.** An entry file outside the approved project root: fail.
