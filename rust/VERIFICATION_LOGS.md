# Shared verification logs

The closed compiler admits error and warning severity for its implemented column
and dataset checks. A warning declaration requires `output.warning_log`
(REQ-0391). The two optional sidecars independently select CSV or Parquet from
their extensions. Their locations retain the original declaring-file provenance;
exact and normalized collisions fail before study access (REQ-0756/1180).

Completed native check records determine the fixed governed tables in REQ-0393
and REQ-1173-1179. Verification rows preserve the evaluated prefix, including held
checks and violations. A warning violation continues execution without becoming
a fatal issue. Warning rows retain all offending keys, independently of the
five-key public issue sample. JSON cell text is compact, sorted and ASCII escaped;
full integers do not pass through binary64. Count columns remain signed int64,
including in a declared empty log. Neither host computes check or log semantics.

Build owns completed log tables, Arrow IPC views, exact file bytes and complete
artifact observations without publishing. Host properties create fresh data
frames. On successful builds, explicit Save preflights the complete physical
target set, then publishes verification, warning and primary in order. It admits
and prepares every possible prefix report before the first write. A native
operational failure or unwind retains the entered boundary, original payload and
actual successful prefix. A retry repeats only publication of held bytes.

A failed build can retain a verification log without granting primary or warning
publication. Explicit Save may publish that diagnostic sidecar and still raises
the failed-build result. Later warning-table, primary encoding and host-view
preparation failures revoke warning/primary authority while preserving a completed
verification capability. Nothing rolls back an earlier successful write. As in
the shared publisher, unsupported Unicode name comparisons on macOS refuse a
multi-target set before any write rather than assuming its targets differ.

Admission limits are 4,096 completed records, 65,536 cells per log-table request,
8 MiB of log text and 1,048,576 JSON nodes with maximum depth 64. Each encoded
file and log observation has an 8 MiB ceiling. Report/copy admission shares the
existing 16 MiB policy across retained enrichment copies and across a Save's
possible prefix reports. Single-domain Save owns at most four such reports.
These are explicit capacity policies, not a promise about process RSS or recovery
from allocation failure.

The unchanged `schema-verification-log` document has three unchanged CSV goldens.
The native source fixture manifest lists that original document, input and three
goldens explicitly. Python and R source archives preserve those exact bytes so
the unpacked source verification-log tests can exercise their own fixture files.
Its complete expected report comes from the retained installed Python reference
engine, with identity labels replaced by fixture placeholders. Native and fresh
installed Python/R suites compare all source and derived cells, check records,
resource reads, exact saved bytes and the complete published report. Additional
tests cover warning continuation followed by a later failure, typed empty logs,
full keys, Unicode IDs, fresh owned views, expired original inputs, target refusal,
late errors/unwinds and retries. Parquet tests use governed logical rows and types.

Producer graphs prepare each entered node's logs after its actual check prefix
and before primary encoding or dependent entry. Native owners retain those tables,
exact bytes and original declaration locations. Successful Save admits the whole
graph target set and publishes each node's verification, warning and primary
before its dependents. Failed graphs grant only completed verification requests;
explicit diagnostic Save still raises the failed-build result. An entered native
publication error reports REQ-0764 with its actual node, output field and target.
Admission refusals, preflight failures and opaque unwinds retain their distinct
causes. Repeated Save replaces publication state and preserves execution evidence.

Graph limits share record admission across nodes and cell/text/JSON/encoded-log
admission across potential node/log slots. Cumulative publication admission
covers 64 nodes, 1 MiB of authored/selected paths, 16 MiB of held artifact bytes
and 64 MiB of work. All possible entered/completed prefix reports share 16 MiB
before the first publisher opens. Lexical duplicate targets fail during metadata
preparation; retained physical parent/entry comparisons run before any Save write.
Output parents can remain absent during metadata preparation and Build.

Installed graph witnesses retain complete Build and repeated Save reports and
saved bytes after checked input/YAML deletion. They compare the unchanged original
log document and CSV goldens, plus independently authored alias/diamond warning
records, exact int64 keys and actual failure prefixes. These supplemental proofs
leave the broader original-producer M1 criterion open. The R graph accessor uses
the paired public result state on Unix and Windows. Actual installed Windows R
qualification is required by [the Windows gate](R_WINDOWS.md). The graph
denied-write permission witness remains Unix-only; R graph case-alias witnesses
remain macOS-only.
Broader compiler, API, native ownership and release gates remain open.
