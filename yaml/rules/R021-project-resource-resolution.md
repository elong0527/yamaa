---
id: R021
title: Project Resource Resolution
status: normative
applies_to: [project_path, dataset_source, dataset_class]
depends_on: [R006, R017, R019]
---

# Project resource resolution

## Intent

Bound every stored file a run reads to one approved project, and bind the
bytes a validated declaration names to the bytes ingestion receives.

## Boundaries

This rule owns the approved project root, the written form of a
`project_path`, the file kinds a run may read, the content identity that
carries from validation to ingestion, and the errors these produce.

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

## The approved project root

A run receives exactly one **approved project root**: an existing local
directory the runner selects before it reads any specification. A runner that
selects none uses the directory holding the entry file. The root is fixed for
the whole run.

No field of a specification, and no value a specification reads, names,
replaces, extends, or widens the root. When R018 also selects a project root
for an implementation environment, the two are the same directory; a
difference fails before code activation.

The entry file resolves inside the root. A layer R017 reaches contributes
declarations that are read against that same root, so composition never
enlarges what a run may read. A layer stored outside the root contributes a
`project_path` that R017 rebases to the entry file, and the rebased form must
satisfy this rule; such a layer therefore cannot contribute a readable source.

## The written form

A `project_path` is one or more segments separated by `/`. It is checked
before the filesystem is consulted, so it fails identically on every platform
and reveals nothing about the host.

- No leading separator, drive letter, or `\\` prefix. A rooted path names a
  location the project does not own.
- No URI scheme. A specification declares stored project files; retrieval,
  caching, and authentication are not part of a derivation.
- No `\` anywhere. A backslash is an ordinary filename character on one
  platform and a separator on another, so a path containing one denotes two
  different files.
- No `..` segment. Traversal is the whole of the escape this rule exists to
  reject, and a normalized path never needs one.
- No `.` segment, no empty segment, and no trailing separator. Each is a
  second spelling of one file, and two spellings defeat the single-snapshot
  identity below.

A written path is ASCII under R019, like every other repository-authored
value.

## Resolution and file kind

A `project_path` resolves relative to the directory of the layer that writes
it, as R002 and R014 require. In a resolved specification it is relative to
the entry file, because R017 has already rebased it.

Resolution walks the path one segment at a time from that directory. Every
component except the last is a directory. The last component is a regular
file.

**No component is a symbolic link**, including one whose target is inside the
approved root. A link is a second name for a file, so a boundary that admits
one must re-derive containment every time the link changes, and the link a
validator followed is not necessarily the link a reader follows.

After the walk, the canonical resolved file is inside the canonical approved
root. The written form and the symbolic-link rejection already imply this;
the check is stated because a boundary defect must fail closed rather than
silently.

A path that reaches no entry is missing. A path that reaches a directory,
FIFO, socket, device, or any other non-regular file is rejected, because its
bytes are not a stored dataset and reading one can block or consume a stream
that cannot be read twice.

## One snapshot per physical file

A run reads one **immutable byte snapshot** of each physical file it accepts.
Every declaration that resolves to that file binds that snapshot.

Several dataset identifiers may resolve to one physical file. The declarations
remain distinct -- each carries its own identifier and its own R014 field
types -- but they share the one snapshot, so no two of them observe different
bytes.

An implementation reads the snapshot through the handle it opened while
validating, or records the SHA-256 of the bytes it validated and verifies that
digest before ingestion. It does not re-resolve the written path and read
whatever that path then names. Content that changed between validation and
ingestion fails the run; the replacement is not read.

Content identity is over bytes. A modification time, inode number, or size is
not the identity, because none of them changes reliably when content does.

## Errors

A failure names the written path exactly as the specification wrote it, the
declaring field, and one condition below. A message does not contain the
approved root, a canonical path, a symbolic link's target, or any other host
path, because those are the values a rejected specification is probing for.

| Condition | Rejects |
|---|---|
| `resource_path_not_relative` | a leading separator, a drive letter, a `\\` prefix, or a backslash |
| `resource_path_uri_scheme` | a URI scheme, including `file:` and `https:` |
| `resource_path_parent_traversal` | a `..` segment |
| `resource_path_not_normalized` | a `.` segment, an empty segment, or a trailing separator |
| `resource_path_symlink` | a symbolic link at any component |
| `resource_path_outside_project` | a resolved file outside the approved root |
| `resource_path_missing` | a path that reaches no entry |
| `resource_path_not_regular_file` | a directory, FIFO, socket, or device |
| `resource_path_content_changed` | content that changed after validation |

The written-form conditions are decided in the order listed, so one written
path reports one condition on every platform.

Every condition above is decided before any data is read and reports under the
`validation` phase, except `resource_path_content_changed`, which reports
under `ingest`.

- An approved root that does not exist, is not a directory, or differs from an
  R018 project root: fail.
- An entry file outside the approved root: fail.
