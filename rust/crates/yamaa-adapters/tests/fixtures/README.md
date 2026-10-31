# Adapters test fixtures

Provenance and reading notes for the fixtures in this folder, with the
qualification notes of the file resolver and the file publisher, folded from
the former per-fixture notes in #2416. The fixtures of the tests that drive the
application layer moved to `rust/crates/yamaa-application/tests/fixtures`,
with their notes (#3761); that suite reads the CSV profile and source typing
truth here too.

## csv_profile_diagnostics
# CSV profile failure truth

Thirteen independently authored byte containers were run through the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. The specification is the same for every case. `source_bytes` retains exact bytes as text, including CR; the one invalid-UTF-8 row is stored as a tagged hex object. Complete reference reports pin source read observations, ingestion timing, condition/requirement, paths and typed context. Only runtime/backend identity is fixed to the existing native replay convention.

All ten closed R023 CSV failure causes are exercised. Decode precedes BOM/syntax, complete syntax precedes header/width checks, and quoted-prefix UTF-8 coordinates retain record/field identity. The parser algorithm and existing collected-text fixtures are preserved. Resource limits remain boundary policy failures without fabricated language diagnostics.

CSV-10 later admitted a byte-order mark that the input declares, so `source_byte_order_mark` records whether the input declares one: the `bom` case, whose input declares none, gained `declared: false` in both of its contexts. Nothing else in the fixture changed.

Rust and both installed hosts replay all complete reports, then drop the prepared specification and verify owned observations and repeated failed-save gates without recapture or publication. No existing benchmark expectation is changed.

## file_publication
# Native explicit-target file publication

The Unix publisher receives an absolute file target explicitly selected by its caller and one declared path that requests must match. It retains the selected parent directory and refuses existing links or non-regular targets. Path and complete-content budgets precede publication effects. Targets ending in `/`, `/.` or `/..` are rejected before path normalization. Generated staging names equal to the target basename are skipped before creating a directory; repeated collisions exhaust the bounded attempt budget without touching the target.

An exclusive nonblocking selected-parent lock coordinates native publishers for the duration of staging, replacement and cleanup. A refusal preserves the prior target and creates no temporary entry. Explicit saves intentionally replace the target currently present; later completed saves win, including when several retained publisher handles select the same target. This is replacement authority rather than a compare-and-swap API. Writers outside the native protocol do not participate in its advisory lock.

The candidate is written into an exclusive private staging directory beneath the selected parent. The adapter verifies its owner, 0700 mode and opened identity, retains its descriptor and creates the candidate exclusively with mode 0666, which the umask narrows as it does for any file `open(2)` creates. It writes and flushes the complete bytes. When the target exists, the candidate then takes the target's permission bits (0777), so it is published with the target's mode (D92). The adapter syncs and closes the file. Candidate identity checks and the final rename use that retained private descriptor. Swapping the staging directory entry in the parent therefore cannot substitute the candidate. Failure paths perform explicit fallible cleanup before returning the publication error. Cleanup verifies the held candidate identity and the observed staging-entry identity before removal. An unconfirmed or substituted entry is preserved. A cleanup failure retains the original operation error, its own cause and the staging location in the returned error. The reported location names the captured parent; outside renames can move that directory. Drop supplies only a final best-effort retry for unwinding and after commit. No fallible publication result follows successful replacement. OUTPUT-8 now explicitly permits this protected staging namespace on the target filesystem.

Eight native integration tests cover complete/empty/UTF-8 bytes, unchanged old hard-link aliases, path/capacity refusal, directory suffixes, link/directory refusal, retained selected-parent authority after rename, lock refusal with explicit retry, intentional replacement by a later completed save and observed permission failure. DAC tests restore permissions and report privileged bypass. A deterministic adapter unit test swaps the temporary entry immediately before rename; it failed by publishing replacement bytes before correction and now publishes the original checked bytes through the retained descriptor. The lock refusal also failed before correction. Four additional unit cases cover target/staging-name collisions and bounded exhaustion, unconfirmed ownership, a substituted staging entry during failed publication, and an actual cleanup permission failure. The collision failed before correction. Failed cleanup preserves the old target, retains the original error and reports the cleanup cause and canonical staging path; it preserves the substituted directory. Permission tests restore the selected parent and report privileged bypass. OUTPUT-8 explicitly reports incomplete cleanup caused by external interference or filesystem failure instead of claiming that no residue remains.

The private R bridge registers handles without dereferencing user pointers. Owned build results save through the native engine gate with no source recapture or R publication callback. Installed replay compares complete reports for all seventeen unchanged original results, exact successful CSV bytes, retained Parquet bytes, repeated saves, capture counters and failed-build gates. It also checks bounded raw transport, target spelling and expired handles with Python absent from PATH. The bridge's roxygen comments document these private closures.

Study-source approved roots retain their separate existing rules. The public facade, Windows publication, general publication diagnostic projection and complete migration acceptance remain open. The private staging directory and its writer protocol are qualified on the checked hosts; wider filesystem/security support is not a release-matrix promise.

The Windows implementations use retained handles in [windows_file.rs](../../src/windows_file.rs) and [file_publication_windows.rs](../../src/file_publication_windows.rs). The installed Python and R suites cover original-file access and publication on Windows.

## native_file_resources
# Native file resource qualification

The local file resolver (`file_resolver.rs`) selects approved directory roots once and retains their descriptors; every read goes through its inspect and capture requests, and `file_resources.rs` only holds and verifies the snapshots. Source paths walk single components without following links, verify each opened identity and recheck the complete parent chain. Relative lookup retains declaring-directory, project and explicit-data-root order; only a genuinely absent entry permits fallback. Permission and file-kind failures stop before later candidates. The existing approved-root policy remains in force pending the separate #1751 approval.

A first capture checks metadata size before allocation, reads in 64 KiB chunks through at most one byte beyond the trusted ceiling, compares a fresh bounded read directly, and retains immutable bytes. Repeated spellings of one canonical path share a snapshot, and each accepted alias is verified directly on reuse; a hard link is another canonical path and so its own snapshot. Changed or over-limit replies add neither a snapshot nor an alias. The policy bounds 64 selected roots, 65,536 UTF-8 bytes per authored path, 1,024 snapshots/path keys/retained written-path witnesses and 67,108,864 retained content bytes. These capacities do not bound total process memory.

Seventeen native file tests and a numeric-inode-reuse test cover exact byte ceilings, cached growth/shrink, empty captures, root/fallback order, links and escaped paths, accepted aliases, replaced root spellings, normalized approved-root aliases and terminal opaque permission failures. Old captured path keys retain their immutable snapshots and fail closed; a numeric file identity is never a snapshot key, so a never-captured path always obtains fresh bytes. The permission, same-key alias, stale snapshot and numeric-inode regressions fail against preceding candidates before correction. Permission tests explicitly report when process privileges bypass DAC instead of asserting an unavailable restriction. Metadata resolution and captures from declaring-file views share selected roots and held bytes; each accepted base/path spelling remains a separate verification witness.

The private R bridge supplies raw UTF-8 paths, raw bytes and capture counters through registered handles. It performs no YAML interpretation, source typing or table inference. Installed R qualification replays all seventeen unchanged original corpus reports twice with real file inspection/capture, the seven independently authored zero-read metadata failures, the unchanged complete Parquet success and nineteen Parquet failures, exact CSV publication, and cached save gates. Python is absent from PATH throughout. Byte-policy checks also cover copied R buffers, wrong arguments, changed contents, symbolic links, approved-root escapes and serialized expired handles.

This qualifies a Unix study-source adapter. Entry/layer/environment/codelist capture, public file APIs, production atomic publication, Windows native file transport and callback retirement remain separate work. Existing truth and reference-assisted inventory levels do not change.

The Windows implementations use retained handles in [windows_file.rs](../../src/windows_file.rs) and [file_publication_windows.rs](../../src/file_publication_windows.rs). The installed Python and R suites cover original-file access and publication on Windows.

## odm
# ODM XML reader documents

`odm/odm13.xml` and `odm/odm20.xml` are the two documents of the retired Python
ODM reader suite, kept byte for byte, so the cases ported into `odm_xml.rs`
apply the same edits to the same text. The installed Python and R `read_odm`
suites read the same two files, staged as `host-fixtures/odm`, against the
`read-odm` family of `rust/tests/host-fixtures/cases.json`.
TAR and gzip inputs are written at test time by `support/odm_files.rs`; no
binary archive is committed. Expected values are the ones the Python suite
asserted or ones read from the documents by hand; no Rust output generated them.

## parquet_bytes
# Saved Parquet bytes

`parquet_bytes/` holds three Parquet files as base64 text, since binary files
live only under `benchmarks/`. The artifact encoder saved them with the 1.0
writer, `parquet-rs version 55.2.0`. `table.parquet.b64` (37,776 bytes) is a
fixed table of 1,030 rows in two row groups, with a column of each type that
PARQUET-1 maps and a missing value in each; `empty.parquet.b64` (157 bytes)
has the same columns and no rows; `nulls.parquet.b64` (15,003 bytes) has the
same columns and row groups, each column missing in a pattern that the
definition levels encode differently (#3878). `table_parquet_bytes.rs` saves
the tables again and compares every byte, and requires the footer's
`created_by` to name that writer (D160 on #3030, #3475). Before they were
committed, pyarrow read each back as the table that the test builds, every
float bit for bit. The native writer replaced parquet 55.2.0 in #3878 and
saves the same bytes; `nulls.parquet.b64` was saved by parquet 55.2.0 under
the same settings before it left.

The encoder wrote these bytes, so they are a regression control, not
independently authored truth. The stability contract in `rust/interface/README.md`
keeps them through every 1.x release: a release replaces them only for a
defect fix that names the rule it restores, and a move to another Parquet
release must leave them unchanged.

No test writes them, in CI or anywhere else. To replace one, run
`cargo test --locked -p yamaa-adapters --test main table_parquet_bytes::`
from `rust/`. Its failure prints the saved bytes of each fixture that differs
as base64 lines of 76 characters, between `-----BEGIN <fixture>-----` and
`-----END <fixture>-----`; those lines, without the two markers, are the new
fixture. `created_by` stays an authored literal in the test.

## source_typing
# Original source typing truth

Four independently authored documents were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Complete failed reports pin an unknown declared field, unknown-field admission before a bad conversion, stored row/field conversion order despite reversed declarations, and invalid temporal text. Each row retains exact source bytes in the `source_bytes` text column, authored paths, requirements, original failed text and source capture observations. No existing expectation is regenerated.

Core admits immutable type declarations and converts the admitted CSV into owned normalized values. Missing declarations fail in written order before conversion; conversion follows stored row and field order without result handlers. The adapter checks the existing Arrow resource bounds before calling conversion and constructs the physical Arrow table afterward. Core owns both typed-source diagnostic causes; structural defects and policy limits have no fabricated language finding.

Rust and both installed hosts compare complete independent reports after releasing preparation and block repeated failed saves without recapture or publication. Existing exact-width integer, float bits, temporal precision, present/missing text and source-finding collection probes retain their observations. This is private original-document evidence; production filesystem ports, public frontend cutover and inventory promotion remain separate gates.
