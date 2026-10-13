# yamaa application phase measurements prototype

This separate, unapproved adapter/native slice forwards the existing engine
application observer through the original file preparation, project activation,
source capture/decode/bind, dataset, report/output conversion and producer node
services. It preserves default observations and all public signatures.

Every ordinary public file Build captures owner-local `std::time::Instant`
offsets in uint64 nanoseconds before original entry. Existing private held-owner
observations transport accepts an optional `include_measurements` flag; the
unchanged default returns the canonical report bytes. The opt-in envelope keeps
that report and typed `application-measurements/1` evidence separate. Reading it
afterward cannot execute a source, function, report service or publisher.

Preparation and Build-service exits describe the selected Rust port Result.
Build-service success can return a held failed-build owner; `build_accepted`
separately records the authoritative held public output. Dataset/Input/Activation/
Output exits and function returns come only from their actual Results. Unwound
callbacks remain entered with no invented return. Classified resource refusals
copy only existing typed Resource/limit/optional-required fields. Generic
capacity and opaque source failures remain unclassified.

Retention is bounded: 64 canonical nodes, 1 MiB total identity UTF-8, 1024 stage
records, fixed 65-by-6 function count aggregates, 64 classified refusals and 16
Save records. There is no per-call ledger. Overflow, unavailable clock,
unmatched exits/returns or incomplete original execution reject only measurement
admission. They do not alter original results, owned errors, partial reports or
publication. Save entries/exits are captured around actual held Save calls;
refusals, failures and retries retain separate actual attempts.

Exact-source adapter controls compare ordinary and instrumented canonical
report/output/log bytes and issues, delete input/spec before held Save, preserve
original IO error ownership, and retry twice. Failed builds retain ancillary log
Save behavior. Three bounded collector controls cover 100,000 actual entered/
returned calls, no timestamp fabrication after execution, overflow and unmatched
unwind events. These are functional controls, not release timing distributions.

The Python build and strict all-target adapter/Python Clippy pass locally. The
adapter replay covers 118 unit controls, 11 application controls, 35 producer
controls and 9 project controls. R compilation and installed 21-tuple phase
qualification remain required; no R/local performance claim is made. Internal
allocator copy counts, process startup, shipped-schema initialization and OS
cache state are still unavailable. Public getter invocation counts are recorded
by the separate worker and are not estimates of internal allocations.
