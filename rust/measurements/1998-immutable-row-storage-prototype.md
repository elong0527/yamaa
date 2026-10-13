# Immutable report row storage prototype

This amendment keeps the encoded report ceiling at 16 MiB and the retained
projection allowance at 64 MiB. Source, output, identity, key, text and execution
ceilings are unchanged. It is a review prototype, not cutover approval.

Each table's rows are serialized once in original row/column read order into a
validated immutable JSON array. The wire cell tags, integer decimal strings,
exact float bits, missing values, civil dates, escaping, table metadata and
report arrays are unchanged. Held node, aggregate and Save reports retain an
`Arc<RawValue>` for each row block; report cloning copies metadata and bounded
row descriptors rather than cell maps. Installed Python/R observation transport
serializes these held blocks directly. Materialized `Value` observations remain
available to Rust controls and legacy service consumers.

The row writer charges actual requested capacity growth before reserve. Three
times requested buffer capacity bounds the old/new reallocation overlap,
conversion to a boxed string and copying into the shared allocation. Encoded
bytes and validation traversal are charged as work; cell count is admitted
before traversal. The writer stops at the existing encoded ceiling. These
bounds describe requested storage, not allocator overhead or process RSS.
Shared ownership is charged for its descriptors and metadata, while previously
admitted immutable row bytes are retained once. Complete wire reports are
checked against the encoded ceiling before admission completes.

Every ordinary Save owns all artifact entries before its first publisher and
moves only already-owned entries afterward. Producer Save also pre-owns every
possible status/error code/location field. It checks the complete artifacts,
complete entered array with false publication flags, and the longest admitted
error representation before any publisher. Shorter entered/artifact prefixes
and true flags cannot exceed that representation. Completion moves owned
status/error fields and truncates bounded arrays; it adds no report allocation
or serialization refusal after publication begins. Original operational
failures, unwind payloads, entered/completed prefixes, failed-build diagnostic
logs and fresh retry semantics remain beside the same held owner.

The focused pre-composition controls passed 113 adapter unit controls, 35
producer integration controls, four project report controls and 68 complete
specification report controls. Expected study artifacts were not changed.
Resource fixture sizes were adjusted where the old controls relied on retaining
per-cell maps: the root output copy refusal now uses 1,200 rows, the later-node
report refusal uses a 16,384-byte caller allowance, and the fixed formatter
refusal uses a 4,300,000-byte string. Its failed source-only prefix now fits and
is checked completely rather than forced to repeat the obsolete map charge.
New controls cover all scalar tags/bits, shared backing lifetime, original cell
failure prefix, row-buffer equality/one-over admission, complete-prefix encoded
equality/one-over and moving the original owned error string.

Exact-head installed 1k/10k/100k frontier evidence and final accepted engine
composition will be recorded separately. Shared-host timing remains diagnostic.
The optional projection capacity trace is temporary prototype instrumentation;
it must be removed before release integration. This record does not authorize
larger quotas or retire any reference baseline.
