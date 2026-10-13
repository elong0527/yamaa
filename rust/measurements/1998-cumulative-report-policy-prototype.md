# yamaa cumulative report policy prototype

This isolated, unapproved frontier slice keeps the encoded ceiling at 16 MiB
and tests a 96 MiB retained projection ceiling (six retained bytes per encoded
budget byte). It does not approve cutover, larger source/execution limits or
release numerical budgets.

Generic producer ReportPort implementations retain the previous equal node
fractions. NativeReport instead owns a fresh cumulative Budget and remaining
actual encoded bytes for each Build. It initializes both from the caller's
actual report limit, charges every entered diagnostic/node report cumulatively,
and subtracts actual complete compact serialization length before releasing its
artifact to a dependent. It never resets remaining capacity when selecting a
later node. Complete aggregate reports and every possible publication prefix
still receive their own exact encoded admission before any publisher.

Root Arrow, public Option<T> columns and IPC conversion allocate no JSON cell
maps. Their three real copy passes now charge 64 bytes per visited typed cell,
2048 bytes per column per pass, complete schema-name admission and four retained
bytes per input UTF-8 byte per pass. The fixed cell charge covers at most twice
the 24-byte Option<String> vector slot, primitive/null/offset buffers and bounded
IPC capacity; the column reserve covers builder minimums and schema owners.
Across the three passes, twelve payload bytes cover exact strings and grown
Arrow/IPC buffers coexisting with borrowed input. Scalar integer/float/date
conversion creates no owned decimal or bit text here. Immutable report row
buffers retain their separate actual requested-capacity admission.

Meaningful controls retain original failures, no publication before admission,
every real publication error code/location, all complete prefixes and retries.
Two typed admission controls pin exact retained-byte equality/one-over and
actual encoded lengths. The public root-copy refusal fixture uses an indexed
string key and missing string cells so its complete graph/report succeeds while
three real copy passes exceed the one-million-cell copying allowance; no
quadratic work or fictitious JSON map is used to force refusal. Caller report
limits of one byte and 8192 bytes still stop first/later nodes respectively and
keep their original earlier artifact evidence.

Local controls: 117 adapter units, 11 application controls and 35 producer
controls pass; strict all-target core/engine/adapter/Python Clippy and dependency,
diagnostic and release-API guards pass. Installed progressive truth/two-Save
frontiers and actual RSS/encoded/retained/work charges are still required before
this candidate policy can be reviewed for approval.
