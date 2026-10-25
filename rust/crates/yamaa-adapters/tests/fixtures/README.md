# Adapters test fixtures

Provenance and reading notes for the fixtures in this folder, folded from the
former per-fixture notes in #2416. The fixtures of the tests that drive the
application layer moved to `rust/crates/yamaa-application/tests/fixtures`,
with their notes (#3761); that suite reads the CSV profile and source typing
truth here too.

## csv_profile_diagnostics
# CSV profile failure truth

Thirteen independently authored byte containers were run through the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. The specification is the same for every case. `source_bytes` retains exact bytes as text, including CR; the one invalid-UTF-8 row is stored as a tagged hex object. Complete reference reports pin source read observations, ingestion timing, condition/requirement, paths and typed context. Only runtime/backend identity is fixed to the existing native replay convention.

All ten closed R023 CSV failure causes are exercised. Decode precedes BOM/syntax, complete syntax precedes header/width checks, and quoted-prefix UTF-8 coordinates retain record/field identity. The parser algorithm and existing collected-text fixtures are preserved. Resource limits remain boundary policy failures without fabricated language diagnostics.

CSV-10 later admitted a byte-order mark that the input declares, so `source_byte_order_mark` records whether the input declares one: the `bom` case, whose input declares none, gained `declared: false` in both of its contexts. Nothing else in the fixture changed.

Rust and both installed hosts replay all complete reports, then drop the prepared specification and verify owned observations and repeated failed-save gates without recapture or publication. No existing benchmark expectation is changed.

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

`parquet_bytes/` holds two Parquet files as base64 text, since binary files
live only under `benchmarks/`. The artifact encoder saved them with the 1.0
writer, `parquet-rs version 55.2.0`. `table.parquet.b64` (37,776 bytes) is a
fixed table of 1,030 rows in two row groups, with a column of each type that
PARQUET-1 maps and a missing value in each; `empty.parquet.b64` (157 bytes)
has the same columns and no rows. `table_parquet_bytes.rs` saves both tables
again and compares every byte, and requires the footer's `created_by` to name
that writer (D160 on #3030, #3475). Before they were committed, pyarrow read
both back as the tables that the test builds, every float bit for bit.

The encoder wrote these bytes, so they are a regression control, not
independently authored truth. The stability contract in `docs/release.md`
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
