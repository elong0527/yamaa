# Adapters test fixtures

Provenance and reading notes for the fixtures in this folder, folded from the
former per-fixture notes in #2416. Fixtures converted from hex-encoded TSV are
now plain JSON with the same rows; staging re-encodes the legacy TSV bytes for
the installed hosts, so staged output is unchanged.

The documents of `binding_diagnostics.tsv`, `grammar_diagnostics.tsv`,
`output_declarations.tsv`, `predicate_diagnostics.tsv`,
`predicate_context_diagnostics.tsv`, `preflight.tsv` and
`window_diagnostics.tsv` are written in 1.0 (#3437). Their reports keep every
recorded value except the paths 1.0 spells differently: a formula reports at
its `derivation`, a bare reference at `derivation.source`, and a row template
by its `id`, such as `rows.first` where 0.5 wrote `rows[0]`.

The specifications in the JSON fixtures are written in 1.0: `tools/migrate`
converted the captured 0.5 text (#3436). Where 1.0 declares a key that 0.5
inferred, names a row or intermediate by its id, or counts an absence under
`absent`, the document and its expectations follow the recorded decision. The
cases whose behaviour only 0.5 had went when the engine stopped reading 0.5
specifications (#3676). The specification text of the `originals` in
`flag_reference.json` keeps its captured 0.5 text, which no test runs: each
benchmark runs as it is written, and only its captured handlers and findings
are compared.

## binding_diagnostics
# Original-document binding findings

`binding_diagnostics.tsv` contains ten independently authored original documents
and complete failed reports, with eleven findings. Python's unchanged reference
`execute_example` produced these new expectations on 2026-10-07. Only runtime,
engine version and backend identity were normalized, consistently with the
existing original-document fixtures. No candidate output is expected truth.

The documents exercise stored-field and output-name failures, qualified numeric
references, grouped-row field access, invalid aggregate driver scope, and
forward, key and cyclic output dependencies. Each uses one declared source with
the exact bytes `ID,V\n1,2\n`. The other-driver aggregate deliberately names
`SECOND.ID` without declaring another source; the reference rejects its driver
scope after ingesting the one declared input. Valid grouped rows with multiple
declared sources remain Unsupported and are not admitted by this diagnostic move.

Rust and both installed hosts compare the complete reports, including source
tables, capture accounting, authored paths, node failures and empty publication
ledgers. Failed results retain their observations after compiled handles are
released. Repeated failed saves neither publish nor recapture.

Eleven binding causes now use the shared core registry. The core guard reaches
grouping and dependency causes through actual scope/dependency services and
the other causes through original-document compilation and binding. Column-group
scope and a missing key derivation retain component coverage; these tests do not
claim new original-document compiler scope. Existing numeric/aggregate grammar
projection is delegated to the core parser diagnostics. Window and named-selection
findings still have their earlier separate core-owned definition projection.

## column_literals
# Original column literal truth

Four independently authored complete reports were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. The successful document pins exact CSV bytes for Unicode/quoting, both i64 limits, a finite float, date/datetime conversion, null and a present empty string. Three failures pin Boolean, fractional integer and invalid date conversion, source observations, completed output prefix and full diagnostics. Runtime/backend identities use the established native replay convention; no existing expected report is regenerated.

The fixed source is `ID\n1\n`. Both installed hosts compile original YAML/JSON, build through shared ports, drop the prepared specification, and replay exact observations and repeated explicit saves. Failed builds cannot publish. Integers outside i64 remain Unsupported before source effects; this does not decide the full-range R public integer representation or admit nested expressions, conversion handlers or additional operation families.

Expected report strings use canonical UTF-8 JSON, matching the installed R report boundary. Source documents retain ASCII JSON escapes; decoded reports and exact publication bytes are unchanged by this fixture serialization choice.

## converted
# Benchmark specifications as 1.0 writes them

`converted/` holds benchmark specifications, and the projects of the two
transport cases, converted to 1.0 by `tools/migrate` for the tests that edit
their text: `specification_sources.rs` and `submission_transport.rs`, and the
`adam_dataset_json_targets_the_integer_its_document_submits` case of
`diagnostics/shared_define.json`, which moved from `submission_transport.rs`.
These copies kept the edits and expectations of these tests in one language
while the corpus moved to 1.0. The tests and the case read inputs and expected
files from the benchmark itself.

## correlated_aggregates
# Aggregate relation controls

`correlated_aggregates.json` contains independently authored original YAML,
source bytes, complete portable observations and exact CSV bytes. Its values
and diagnostics are declared directly, without copying native execution output.
The Rust frontend and both installed host suites consume the same truth.

The sixteen controls cover:

- Enclosing grouped rows, qualified donor matching and completed output-row
  reduction in one run. The grouped expression produces 13 and 12; their
  treatment-group mean is exactly 12.5. The missing group remains missing.
- Exact integer MAX above binary64's consecutive-integer range and COUNT of
  two missing values, which is zero for a nonempty relation.
- Mixed relation and ungrouped identifier admission, required output groups,
  qualified group scope, missing key fields and match variables, and key type
  validation with original operation and expression paths.
- Scalar and list key shorthands, including repeated equal list pairs, and
  explicit empty map/list keys with their complete retained declarations.
- Eager ONLY behind a missing operand, retaining row name, complete raw group context
  and record cardinality before any accepted output or publication.
- A later column ONLY failure preserving the enclosing grouped-row context
  and the current output key identity.

The fixed engine/runtime versions are report comparison controls. Float table
observations use exact binary64 bits; missing cells retain the established
`{"type":"missing","value":null}` wire spelling. Each failed build refuses
save and each successful build is saved twice with identical bytes.

Derived aggregate bindings, between narrowing, correlated current-row filters
and aggregate reads through named intermediates remain explicit follow-on
scope. Existing benchmark expected artifacts are unchanged.

## csv_profile_diagnostics
# CSV profile failure truth

Thirteen independently authored byte containers were run through the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. The specification is the same for every case. `source_bytes` retains exact bytes as text, including CR; the one invalid-UTF-8 row is stored as a tagged hex object. Complete reference reports pin source read observations, ingestion timing, condition/requirement, paths and typed context. Only runtime/backend identity is fixed to the existing native replay convention.

All ten closed R023 CSV failure causes are exercised. Decode precedes BOM/syntax, complete syntax precedes header/width checks, and quoted-prefix UTF-8 coordinates retain record/field identity. The parser algorithm and existing collected-text fixtures are preserved. Resource limits remain boundary policy failures without fabricated language diagnostics.

CSV-10 later admitted a byte-order mark that the input declares, so `source_byte_order_mark` records whether the input declares one: the `bom` case, whose input declares none, gained `declared: false` in both of its contexts. Nothing else in the fixture changed.

Rust and both installed hosts replay all complete reports, then drop the prepared specification and verify owned observations and repeated failed-save gates without recapture or publication. No existing benchmark expectation is changed.

## file_preparation
# Native file preparation qualification

The private Unix file entry point owns a retained approved-root reader and the shared prepared model. It captures entry YAML under the existing 16,777,216-byte admission limit, selects standalone or inherited preparation in Rust, uses the exact package-owned schema closure, and retains every parent capture and successful declaring-file spelling. The engine admits the model and executable vocabulary before study capture. Entry and parent captures count separately from each build's study-read observations.

Inheritance resolves parents through the same approved roots and current fallback policy. Its lexical path rebasing performs no study/output lookup. Relative paths from different layer directories retain the same spelling rule as the existing reference inheritance port. Canonical source identities carry complete layer diagnostic context. The previously qualified captured-document entry point independently confirms the inherited entry-version finding and requirement before any parent authority.

Nine Rust tests cover complete early version/missing-parent/invalid-parent findings, retained entry/parent bytes, cross-directory paths, duplicate parent resolution, declaration-only checking without study capture, links and approved-root escapes. An independently authored unreadable-parent regression fails before the correction and preserves the inheritance port's complete unavailable-parent finding without fallback; study-source unreadability remains opaque under its separate port contract. DAC probes explicitly report a privilege bypass. A selected Unix root containing a literal backslash retains the reader's own canonical entry identity; authored path restrictions, canonical containment and descriptor/link checks remain intact. This regression also fails before correction. The unchanged seventeen-document corpus preserves complete build observations twice and cached capture counts. Successful model preparation retains its immutable YAML model; later builds verify study snapshots and preserve prior owned results after a changed source fails.

Installed Unix Python and R probes replay those seventeen original documents through native file preparation and direct build without host preparation/capture/inspection callbacks. Both hosts save retained successful bytes through explicit native publication targets, compare complete reports and exact independent CSV output twice, and reject failed saves without publication. R runs without Python on PATH and rejects expired or wrong registered handles. Further probes compare complete early findings, lexical views, no-study checks and opaque native content-change failures. Python preserves failed-build save gating before selecting a publication target.

This is a private Unix application capability. Windows native IO, the public specification/environment facade, complete static compilation, external function/codelist preparation, warning and row-template checks, workflow and submission remain open. The separately pending path-policy approval is not implied by this qualification. Existing approved roots, reference truth, assisted inventory tuples and frontend acceptance levels remain unchanged.

## file_publication
# Native explicit-target file publication

The Unix publisher receives an absolute file target explicitly selected by its caller and one declared path that requests must match. It retains the selected parent directory and refuses existing links or non-regular targets. Path and complete-content budgets precede publication effects. Targets ending in `/`, `/.` or `/..` are rejected before path normalization. Generated staging names equal to the target basename are skipped before creating a directory; repeated collisions exhaust the bounded attempt budget without touching the target.

An exclusive nonblocking selected-parent lock coordinates native publishers for the duration of staging, replacement and cleanup. A refusal preserves the prior target and creates no temporary entry. Explicit saves intentionally replace the target currently present; later completed saves win, including when several retained publisher handles select the same target. This is replacement authority rather than a compare-and-swap API. Writers outside the native protocol do not participate in its advisory lock.

The candidate is written into an exclusive private staging directory beneath the selected parent. The adapter verifies its owner, 0700 mode and opened identity, retains its descriptor and creates the candidate exclusively with mode 0666, which the umask narrows as it does for any file `open(2)` creates. It writes and flushes the complete bytes. When the target exists, the candidate then takes the target's permission bits (0777), so it is published with the target's mode (D92). The adapter syncs and closes the file. Candidate identity checks and the final rename use that retained private descriptor. Swapping the staging directory entry in the parent therefore cannot substitute the candidate. Failure paths perform explicit fallible cleanup before returning the publication error. Cleanup verifies the held candidate identity and the observed staging-entry identity before removal. An unconfirmed or substituted entry is preserved. A cleanup failure retains the original operation error, its own cause and the staging location in the returned error. The reported location names the captured parent; outside renames can move that directory. Drop supplies only a final best-effort retry for unwinding and after commit. No fallible publication result follows successful replacement. OUTPUT-8 now explicitly permits this protected staging namespace on the target filesystem.

Eight native integration tests cover complete/empty/UTF-8 bytes, unchanged old hard-link aliases, path/capacity refusal, directory suffixes, link/directory refusal, retained selected-parent authority after rename, lock refusal with explicit retry, intentional replacement by a later completed save and observed permission failure. DAC tests restore permissions and report privileged bypass. A deterministic adapter unit test swaps the temporary entry immediately before rename; it failed by publishing replacement bytes before correction and now publishes the original checked bytes through the retained descriptor. The lock refusal also failed before correction. Four additional unit cases cover target/staging-name collisions and bounded exhaustion, unconfirmed ownership, a substituted staging entry during failed publication, and an actual cleanup permission failure. The collision failed before correction. Failed cleanup preserves the old target, retains the original error and reports the cleanup cause and canonical staging path; it preserves the substituted directory. Permission tests restore the selected parent and report privileged bypass. OUTPUT-8 explicitly reports incomplete cleanup caused by external interference or filesystem failure instead of claiming that no residue remains.

The private R bridge registers handles without dereferencing user pointers. Owned build results save through the native engine gate with no source recapture or R publication callback. Installed replay compares complete reports for all seventeen unchanged original results, exact successful CSV bytes, retained Parquet bytes, repeated saves, capture counters and failed-build gates. It also checks bounded raw transport, target spelling and expired handles with Python absent from PATH. The bridge's roxygen comments document these private closures.

Study-source approved roots retain their separate existing rules. The public facade, Windows publication, general publication diagnostic projection and complete migration acceptance remain open. The private staging directory and its writer protocol are qualified on the checked hosts; wider filesystem/security support is not a release-matrix promise.

The Windows implementations use retained handles in [windows_file.rs](../../src/windows_file.rs) and [file_publication_windows.rs](../../src/file_publication_windows.rs). The installed Python and R suites cover original-file access and publication on Windows.

## first_available
# Original first_available selection

Eleven independently authored complete reports cover ordered raw-value selection,
driver-backed filtered operands, missing fallback, omitted fallback, empty lists,
static unknown fields in skipped operands, inapplicable scalar filters, distinct
value conflicts and conversion failures. Six successful reports include exact
CSV bytes. Every expected source/output cell, diagnostic, node, read and artifact
was authored independently, then compared with actual reference Python execution.
No candidate output generated the truth.

All operand references and predicates bind before evaluation, including skipped
operands. Execution stops at the first present raw value and converts only that
result. Missing fallback is a literal rather than a handler and records no handler
count. A selected conversion failure does not advance to later sources. Distinct
value conflicts retain the enclosing expression path; static findings retain the
individual operand path. Filters reuse the existing predicate and collection
services. Key-phase, row-template, secondary and intermediate selection operands
remain outside this compiler slice. Existing predicate parity gaps remain open.

The unchanged negative-source-trivial-filter benchmark receives a separate full
report authored from its committed input and LOOKUP-8 error declaration. Installed
hosts compare the complete report with new and cached captures and retained save
gates. It is private-route evidence; the assisted inventory remains unchanged.

## grammar_diagnostics
# Numeric and aggregate grammar diagnostic truth

`grammar_diagnostics.tsv` holds seven authored original JSON documents (valid
YAML) and complete independent failure reports. Each reads the held CSV bytes
`ID\n1\n` before binding reports its grammar finding. The cases exercise all
three numeric and four aggregate grammar causes, including written lowercase
function spelling, argument count, prohibited comparison and nested reduction.

The independent Python reference `execute_example` produced the reports on
2026-10-07 from the reference source at
`c813b08d98f002e827c2e9fbe1832e965297d24a`. The candidate did not generate them.
Only backend and runtime/engine version identity were normalized to the existing
prototype labels. Source tables, capture accounting, authored paths, node
outcomes and complete contexts remain as observed. No artifact is published.
Existing benchmark expected outputs are unchanged.

Rust and both installed hosts compare these complete reports, retain failed
results after releasing the compiled handle and require repeated saves to fail
without publication or recapture. These are private original-document tests,
not qualification of the replacement public facade or an inventory promotion.

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

## odm_deferred
# Deferred ODM defaults

Seven independently authored reports and CSV artifacts retain NAME-7 to
NAME-13 semantics when a column default depends on a completed row-number window.
They cover record occurrence scope, missing items, group hierarchy scope,
selectors followed by source-only predicates, an unselected CASE branch, and
complete ambiguity context with the authored row-template identity. Record
occurrences use the completed sequence key.

Expected reports and CSV bytes were authored independently of the evaluator.
Each replay compares the complete report and source ledger and publishes a
successful held artifact repeatedly. Existing fourteen ODM fixtures remain
unchanged. The same fixture is staged for installed Python and standalone R.

## odm_reads
# Independently authored ODM reads

These fourteen literal reports and CSV artifacts follow NAME-7 to
NAME-13.
Eight separate complete preflight failures in `odm_preflight.json` also cover
INPUT-4.
They were authored independently of the new Rust evaluator; no native output
generated their expectations. Historical benchmark expected files are unchanged.

The controls exercise implicit-key feeding records, missing items, all three
qualifiers, source-only predicates, ASCII field binding, ignored vendor fields,
group scope over the original relation, record occurrence scope, column overrides,
strict repeated-record failure even when values differ, complete identifying-field
differences, row and column ambiguity contexts, required identifier counts and
storage coordinates, missing fields, and ambiguous stored names. Eight additional
controls require context, unknown input, vendor predicate, types, and ordinal
failures to retain their complete authored diagnostics before any source read.
Core source-interpretation controls separately carry `odm` spellings in dictionary
data keys and item-like literal values. The preflight scan must preserve ordinary
source admission for those expressions. Mapping execution belongs to its own
migration slice and is combined during installed inventory qualification.

Every case compares the full report, retained source table, source read ledger,
diagnostics, and artifact bytes. Failed results cannot publish; successful results
publish twice without another capture.

## original_all_or_none
# Original all_or_none reports

Eleven independently authored complete reports cover all-present/all-missing and
empty success, partial presence, repeated column names, seven offending rows,
unknown fields, fewer than two distinct columns and duplicate identities. Three
successful cases retain exact CSV bytes. Every source cell, typed output cell,
diagnostic, verification record, node, read and artifact was authored independently
and compared with actual reference Python execution. Candidate output never writes
this truth.

Declaration failures retain the prior unique-check record. Failed checks retain
later row-count records, short reported key samples and the full offending-key log.
No failed result exposes output or allows publication. Both installed hosts replay
new/cached captures and repeated retained saves. The unchanged negative-paired-dates
benchmark has separate independently authored complete truth, including declared
date source cells and the portable error in its committed expected/error.yaml.
Nontext names remain schema-admission failures; warning severity and column
verifications remain outside this bounded slice.

## original_assertions
# Original assertion reports

Seventeen independently authored complete reports cover successful and failed
assertions, inactive/unknown guards, missing requirements, empty output, unknown
and qualified output fields, grammar findings, incompatible predicate types and
duplicate check identities. Incompatible when samples precede later require grammar
or unknown-field findings, and multiple unknown names retain lexicographic precedence. They retain the completed unique-check prefix and
later row-count records where evaluation completes. Three successes include exact
CSV bytes. Every source/derived cell, finding, verification record, node, read and
artifact was authored independently and compared with actual reference Python
execution. No candidate output generated this truth.

Assertions bind only declared output columns, including names behind inactive
branches. Declaration/sample validation precedes actual-row checks, and both
predicates evaluate even when the guard would exempt the row. Core owns deferred
unknown-field and grammar findings; a valid when declaration is retained as a
sample-validation checkpoint before a later require finding. Failed checks and
invalid declarations retain prior records and expose no publication output.
Nontext predicates, unsupported literal/regex policy and the existing special
predicate grammar/context gaps remain outside this slice. Warning severity and
column verifications remain unsupported.

## original_column_matches
# Original column matches truth

`original_column_matches.json` holds sixteen independently authored complete reports for raw YAML and captured CSV. Every field and six successful exact CSV artifacts were enumerated and compared with actual reference Python execution before the candidate was exercised. Candidate observations never write expected truth. Only backend and runtime/engine versions are normalized.

Search is unanchored unless the authored pattern supplies anchors. Missing values pass. Unicode scalar matching retains the portable contract; the lookahead and backreference cases fall outside the portable subset and are deferred invalid patterns. Wrong column kinds and invalid syntax are deferred until the owning converted column checkpoint, even for empty input; earlier conversion failures take precedence. A held record survives a later conversion failure. Compound cases pin an earlier invalid pattern and a repeated verification ID before a later pattern that exceeds compilation policy: the earlier TEXT-6/CHECK-3 remains deferred at its owning checkpoint. Later declaration vocabulary is still scanned before study capture, while unreachable payload compilation stops. Invalid pattern findings contain only the original pattern at the pattern path and keep earlier completed records.

An additional reference assessment found that empty patterns fail schema minimum length before source capture; it is outside this checkpoint fixture. The unchanged negative-sex-code and negative-matches-bad-pattern documents have separately authored full reports. Existing committed expected CSV and error files are unchanged.

Both installed hosts replay all sixteen cases twice with cached source capture, exact complete observations, success publication bytes and failed-save gates. R stays inside the Python-free PATH guard.

## original_column_values
# Original column value reports

`original_column_values.json` contains twenty independently authored complete reports for raw YAML and captured CSV bytes. The author enumerated every field and six successful exact CSV artifacts, then compared the truth with actual reference Python execution before running the candidate. Candidate observations never write expected truth. Only backend and runtime/engine versions are normalized for that reference comparison.

Allowed-value cases pin one-time declaration conversion, missing values, empty lists, invalid or missing permitted literals, column conversion before declaration validation and retained records through a later conversion error. Range cases pin inclusive finite bounds, mixed integer/float comparison, missing values, empty or reversed bounds and invalid column type on empty output. Length cases pin Unicode scalar counts with an accented character and emoji, missing values, a zero bound and invalid column type even on empty output.

Rust and installed Python/R replay complete fresh/cached reports, exact CSV saves and failed-save gates. Invalid declarations keep the completed key-check prefix and assert no column-check record. The bounded slice admits error severity, finite range bounds, signed-64-bit literals and at most 64 permitted values per check. Other column families, warning severity, wider literals and row-template column checks remain Unsupported or explicit resource refusals before study capture.

## original_conversion_handlers
# Original column conversion handler truth

Five independently authored documents were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Complete reports pin Boolean recovery, explicit null replacement, an unused invalid replacement, missing input without firing, and failed replacement conversion. Exact CSV and every handler count, including zero, are retained. Runtime/backend identities use the established native replay convention; no existing expectation is regenerated.

Ordinary-column wrappers compile handlers into the existing core `ConversionHandler` and engine recovery service. Handlers add no expression dependencies and are converted only when reached. Row-template wrappers share the same admitted scalar handlers; their independent cases and effective-default counter ordering are documented in the `original_row_conversion_handlers` section of this README. Other handler families remain Unsupported. Rust and both installed hosts drop the prepared specification, retain complete observations and replay explicit saves without recapture; failures cannot publish. R replay remains inside its no-Python PATH block.

## original_mapping_cut
# Original mapping and cut truth

These twenty authored specifications and complete reports were written from
COLUMN-10, COLUMN-11, COLUMN-14, EXPR-11, EXPR-12 and EXPR-13.
No candidate or reference evaluator authored an expected value. The truth owns
source bytes, typed observations, declared zero/reached handler counts, failure
identity and geometry, and repeated exact CSV publication. It covers roots,
row templates, lazy cases, both interval closures, numeric order above 2^53,
empty inputs and typed dictionary literals.

Static collision and malformed boundary diagnostics additionally have complete
independent core/adapter checks before study ports. Dictionary file capture is
an explicit follow-on planning boundary; it is not silently expanded by a host.

## original_not_missing
# Original column presence reports

`original_not_missing.json` is independently authored complete-report truth for ten raw YAML documents and their captured CSV bytes. The author enumerated every report field and four successful CSV artifacts, then compared them with reference Python executions before running the candidate. Candidate output never writes expected truth. Only backend and engine/runtime versions are normalized for that reference comparison.

The cases cover present and missing values, empty output, a failed check preceding a later conversion, successful records retained through a later conversion error, a missing key checked before output-key validation, duplicate IDs after a failed prefix, multiple failures in one column, separate column/dataset ID namespaces, and a key declared after an ordinary column although keys complete first. A duplicate declaration replaces the primary diagnostic while preserving its earlier failed record. Every check on one column completes before that column's error gate.

Rust and installed Python/R replay the full reports, repeat with cached-source accounting, save each successful CSV twice and reject failed saves. R runs the cases inside its Python-free PATH guard. The fixtures cover error-severity `not_missing` in implicit key grain; other column families, warning severity and row-template column checks remain Unsupported before study capture. A separate null-arguments probe reaches the existing schema-admission family and is not promoted as portable verification truth.

## original_row_conversion_handlers
# Original row conversion handler truth

Eight independently authored original documents were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Complete reports and exact CSV pin row recovery, explicit null, unused invalid replacement, missing input, failed replacement, a default shared by two templates, an override alongside its default and a completely overridden default. The latter has no counter entry; repeated effective defaults share one entry with combined firings. Counter order follows effective row declarations before ordinary columns.

This extends existing conversion recovery without changing engine evaluation. Rust and both installed hosts drop the prepared specification, retain complete reports and repeat explicit saves without recapture; failures cannot publish. R replay stays inside its no-Python PATH block. No existing fixture is regenerated. Nested expressions and other handler families remain Unsupported.

## original_source_bindings
# Original source bindings

Twelve independently authored original YAML documents, source records, complete
reports, handler counts and exact CSV bytes cover scalar driver and secondary
source bindings. The truth was written from COLUMN-2, COLUMN-11, COLUMN-12, COLUMN-14,
LOOKUP-1 and LOOKUP-10. No candidate or reference execution generated the expectations.

Cases cover eligibility before ordered selection, stable first/last ties, explicit
null placement with descending order, empty matches with zero handler counts,
present missing values without absence handling, driver collection selection,
lazy intermediate and secondary first_available operands, duplicate donor
records with equal values, exact int/float comparisons beyond 2^53, and an
unused invalid absence replacement. Successful cases are saved twice; every document is run twice
with fresh attempt-local choices and handler counts. Negative results cannot save.

The source.condition wording in issue #1980 has no grammar definition.
`yaml/schema_expression_core.yaml` source_binding_class, NAME-6 and COLUMN-12
define variable, filter, order_by, keep and absent. Condition belongs to flag_class.
Existing unknown-field schema diagnostics remain authoritative. Single-value
ordered sources and key-phase source options remain explicitly unsupported in
this first slice. Mapping/cut and complete cross-family handler projection remain
open work under #1980; this evidence does not close that work package.

## output_declarations
# Output declaration truth

`output_declarations.tsv` contains five authored original JSON documents (valid
YAML) and their complete failure reports. Each document reads `source.csv` with
the held bytes `ID\n1\n`, derives `ID` and `VALUE`, then reaches output declaration
validation. The combined case preserves the ordered profile, undeclared column,
duplicate column and internal key findings. It does not reject during preflight.

The independent Python reference `execute_example` produced these reports on
2026-10-07 from the reference source at
`c813b08d98f002e827c2e9fbe1832e965297d24a`. The candidate Rust engine did not
generate them. Only backend and runtime/engine version identity were normalized
to the existing prototype fixture labels. Source tables, capture accounting,
diagnostics, node outcomes, verification and artifact observations are retained.
No artifact is published. Existing committed benchmark truth is unchanged.

The installed Python and R tests compare the complete reports, retain each
failed result after the compiled handle is released, and require repeated save
attempts to fail without publication or recapture. This covers the private
original-document use case; it does not qualify the replacement public API.

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

## predicate_diagnostics
# Original predicate diagnostic truth

The malformed window filter `ID >` was independently authored and captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Its complete failed report pins FORMULA-1, the authored filter path, character position 4 and one captured/ingested `ID,V\n1,2\n` source. Rust and both installed hosts preserve this report after dropping preparation, without recapture or successful save.

The shared core diagnostic also represents the existing named-selection causes and predicate parser causes, with typed context and canonical registry mappings. Existing independent lookup reports and locked predicate syntax fixtures retain their exact observations. No old expectation is regenerated.

Two additional original window filters now have independently captured complete
failed reports in `predicate_context_diagnostics.tsv`. They were captured on
2026-10-11 from the unchanged Python reference at
`976c38b27c6fd7dbb33e6f03bbb219a9596ed21c`, using the same admitted document and
`ID,V\n1,2\n` source as the earlier malformed filter. Only report identity fields
(`backend`, `runtime_version`, `engine_version`) select the native fixture
identity; diagnostics, capture history, source tables and failed node outcome
come from reference execution. No existing expectation was regenerated.

Core now projects original-document grammar findings as the reference
`predicate` and character `position` context. Rich regex and temporal facts stay
on the retained immutable parser cause and in standalone syntax responses. The
Rust complete-report tests retain both results after dropping preparation,
repeat failed-save gates and observe exactly one source capture. This new
fixture does not claim a fresh installed Python/R host qualification.

The three previously documented forms now have these statuses:

| Filter | Unchanged reference observation | Native status |
| --- | --- | --- |
| `ID LIKE '%' ESCAPE 'xx'` | FORMULA-1, position 19; predicate/position context | Approved FORMULA-7 under #1973 item 6; position 19 and predicate/position context are preserved. The historical reference citation remains recorded. |
| `STR_CONTAINS(ID, '(')` | FORMULA-8, position 17; predicate/position context | Matches the new complete original report; standalone syntax retains regex byte/reason |
| `ID = DATE '2026-02-30'` | FORMULA-1, position 10; predicate/position context | Matches the new complete original report; standalone syntax retains temporal kind/error |

FORMULA-7 owns invalid ESCAPE literals in `rules/formulas.md`, and the
locked predicate syntax fixture already preserves that native mapping. The
unchanged Python reference was rerun on 2026-10-11 and still reports FORMULA-1.
The delegated decision in #1973 item 6 selects FORMULA-7 for new shared execution.
This disposes of the citation difference without rewriting either locked
observation. FORMULA-3 now explicitly describes parse-time `invalid_expression`
under FORMULA-1 and retained temporal cause/standalone context, preserving the
original rejected date's complete report. Installed qualification remains a
separate gate; this record does not turn a pending tuple into a pass.

## preflight
# Original-document preflight truth

`preflight.tsv` contains five authored JSON documents (valid YAML) and sixteen
ordered findings from the independent Python reference at main
`c813b08d98f002e827c2e9fbe1832e965297d24a`. They exercise missing and duplicate
derivations, unknown row columns and keys, domain/input collisions, unavailable
drivers, conflicting row construction and empty/duplicate/foreign groups.

Truth was recorded on 2026-10-07 using `load_specification` against the shipped
schema and `preflight_execution`; each `ExecutionPlanningError.diagnostics`
record was serialized with `model_dump(mode="json")`. Only the existing
`specification/prototype` transport envelope was added. Neither native bindings
nor candidate diagnostics were used to obtain expectations. No study file was
opened. Existing benchmark expectations remain unchanged.

Both installed hosts and the original-document adapter compare each envelope's
status and complete findings. The core registry test also reaches all twelve preflight causes through
actual compilation, including the existing Parquet redundant-type requirement.
Null rules and unavailable dataset values are intentional reference truth;
empty lists and ordered repeated/listed row names must remain intact. This is
preparation evidence, not public API or `shared_run` inventory qualification.

## row_column_checks
# Row-template column checks

The 48 complete reports in `row_column_checks.json` were authored from independently enumerated and reference-qualified presence, allowed-value, range, length and pattern reports. Adding row templates preserves their exact logical output or failure. Two additional cases construct the same two output rows from two explicit templates over one source row; source observations are enumerated separately. Three row-assignment conversion failures explicitly remove unvisited column-check records because construction stops before the first check. All 48 complete reports and successful CSV bytes were compared with actual reference execution while imports of the candidate native module were forbidden, before Rust replay. Candidate observations never write expected truth. JSON uses literal UTF-8, matching the complete byte comparison used by the R binding; converting JSON escapes preserves every expected value.

The checks cover declaration order, keys declared later, separate ID namespaces, multiple failed checks, empty output, invalid declarations, type conversion, Unicode, portable regex patterns, earlier findings before later limits, and retained records before a later global-column conversion error. Multiple-template cases pin total constructed row counts and offending output keys. Existing expected fixtures remain unchanged.

The compiler retains each declared column's check group in its immutable model for both row and non-row plans. Both plans attach the same admitted groups to the existing engine checkpoints. Rows finish construction and conversion before column checking; completed global columns then advance the declared prefix. Key validation and dataset checks keep their existing later phases. Static declaration queries use the same column paths, targets, ID namespaces and portable findings.

This extends error-severity column checks over the existing bounded row language. Warning severity and governed sidecars, row-output dependencies, broader row expressions, intermediate row drivers and the public facade remain separate migration work. The assisted inventory is unchanged.

## row_filters
# Candidate row filters

The 27 complete reports in `row_filters.json` were enumerated independently and compared with actual Python reference execution while native imports were forbidden, before native replay. Twenty-one successful reports retain exact CSV and six grammar, source-reference or phase failures retain complete source observations and failed-save gates. Candidate observations never write expected truth. Existing goldens and the assisted inventory remain unchanged.

The compiler admits filters on converted row columns and direct driver fields for ungrouped rows. Group-reference findings precede filter and derivation findings, including a combined failure with both invalid group and filter. Grouped filters read completed candidate columns, including admitted row-local defaults, and reject source-record reads. Grouped filter reads promote admitted defaults into each template's row scope (SPEC-6). Typed phase findings preserve authored paths, identifiers, row IDs and available/required phases, including the reference vocabulary's absent requirement identifier. Executable predicates, conversion order, three-valued truth and filtering remain existing engine services. All row-filter text is charged before compiling any expression.

Truth covers comparisons, boolean composition, missing/unknown values, empty input, Unicode expressed with valid ASCII YAML escapes, post-conversion reads versus original source values, template declaration order, checks after filtering, grouped MEAN/COUNT and inherited defaults. An initial independently authored template-order expectation was corrected to declaration-order construction before native replay. Grammar and unknown driver-field reports retain existing canonical causes.

Qualified lookup state, broader row derivations and default dependencies remain unsupported. Ungrouped filter promotion of a column default not already selected by a template remains explicitly unsupported pending reconciliation: SPEC-6 names grouped filters as the promoting context, while the reference also promotes ungrouped filter reads. This slice retains that gap without fabricating a phase failure for those forms. Public frontend and warning-severity gates remain open.

`row_filter_admission.json` now retains two independently authored complete
failure reports for integer literals behind constant boolean branches. The
approved VALUE-5 predicate carrier rule requires `integer_overflow` during
parsing, with the complete predicate and character position. The findings remain
deferred until source ingestion, preserve the source table and failed-save gate,
and cannot be hidden by boolean short circuiting. The historical reference
reports derivation overflow with an absent rule; that reference remains
unqualified for this family. Benchmark expected files are unchanged. Parser and
executable-plan resource refusals remain implementation policies.

## row_reductions
# Grouped row MEAN and COUNT

The 25 complete reports in `row_reductions.json` were independently enumerated before native replay and compared with actual Python reference execution while candidate native imports were forbidden. Twenty-two successes retain exact CSV bytes; three scope failures retain complete source observations, the authored aggregate path, expression and driver-scope reason. Candidate observations never write expected truth. Existing expected fixtures and the assisted inventory remain unchanged.

Explicit source typing fixes numeric inputs independently of output conversion. MEAN cases cover integer and float inputs, skipped missing values, all-missing and empty groups, signed zero, ordered forward/reverse floating accumulation, integer output conversion and multiple groups. Float bits are enumerated separately from CSV spelling; the CSV profile renders integral floats without `.0`, including `-0`. COUNT cases cover records versus present values, raw text and Unicode, all-missing and empty groups, typed numeric fields and multiple groups. Invalid relation and unqualified-operand cases stop after ingestion, retain the source table and reject saving. Two combined cases attach a presence check to grouped MEAN and an all-missing value COUNT, pinning total constructed-row accounting and zero as a present result before later global columns.

The shared core compiler lowers admitted grouped row reductions to existing immutable engine expressions: ordered numeric SUM/MEAN, record counts and present-value counts. It retains authored expression/source identities and driver scope. No new reducer executes in a binding or adapter. Both installed hosts replay complete reports twice with fresh/cached capture accounting, exact successful saves and failed-save gates.

This qualifies only direct grouped driver fields and COUNT of the driver's qualified record star. Other reducers, nested arithmetic, reduction filters/keys, row-output dependencies, intermediate row drivers, warning severity and the public frontend remain separate work. The original row-conversion and column-check suites continue to protect their existing phases and declared checkpoints.

## schema_kinds
# Decoded schema kind witnesses

Historical note: the `schema_layer_admission.tsv` fixture described here was
removed in #2416 together with the private schema-requirements tooling that
was its only reader; this section is retained as provenance for that suite.

The new `portable-kinds-existing-wire` row in `schema_layer_admission.tsv`
contains fourteen independent queries. Each decoded kind (`null`, `boolean`,
`integer`, `float`, `text`, `sequence`, `mapping`) is rejected once as an
inheritance layer and once by an incompatible descriptor type. This distinguishes
the existing layer labels (`NoneType`, `list`, `dict`) from the existing
descriptor labels (`null`, `sequence`, `mapping`). Both installed hosts replay
the complete stateless response and the captured schema response.

The new expected response was authored from Python's unchanged
`_validate_layer` and `_validate_type` reference functions on 2026-10-07. The
schema and compilation metadata are retained from the pre-existing independent
`layer-column-fragment` fixture. No native output was used as expected truth,
and all seven pre-existing rows are unchanged.

Core findings carry `DocumentKind` in `SchemaContext::ValueKind` or
`SchemaContext::LayerKind`. A named schema class or an authored type expression
remains text; identifiers such as `column_class` and `list[str]` describe the
language rather than a host class. Only the adapter projects the retained wire
vocabulary. The eight-byte maximum portable kind name is charged to the shared
diagnostic text budget even though the enum owns no string. The existing bounded
wire projection charges the actual rendered text separately.

## source_capture
# Explicit source-port failure replies

Eight independently authored complete reports exercise the source-port contract
already qualified in #1773: missing and non-regular resources, at the first or
second lookup source request, with new or cached prior bytes. The reports are
constructed from that declared contract and FILE-7 without invoking the
candidate engine. They are source-port truth, not a claim that the production
filesystem preflight or original-YAML inventory frontend has qualified.

The terminal failure retains validation phase, authored path/dataset context and
zero new snapshots; any captured prefix remains in order with its actual counter.
No table, artifact, callback or verification is accepted after capture failure.
Both installed bindings replay the same complete truth through an explicit closed
failure reply, retain failed owned results and refuse every save. Opaque thrown
host errors, interruptions and malformed replies are qualified separately.

## source_filters
# Original filtered source expressions

Thirteen independently authored complete reports and exact CSV bytes cover record
eligibility, empty selections, null predicates, repeated identical raw values,
unqualified/wrong-dataset filters, absent stored fields, repeated identifiers, BETWEEN subjects, unreachable absent fields, predicate grammar and
filters over one completed output value. Every expected cell, finding, source
read, node and artifact was authored independently and then compared with an
actual reference Python execution. Only backend/runtime identifiers differ.
No candidate output generated this truth.

Filters bind after complete ingestion, retain authored paths and inspect every
identifier, including unreachable branches. LOOKUP-8 keeps eligibility within
the source dataset. LOOKUP-8 rejects filters over single output/intermediate
values before parsing that inapplicable filter. The existing core predicate and
collection services evaluate eligibility before reading candidate values.
Filtered key assignments, secondary source expressions and source ordering
remain explicitly unsupported before study effects in this bounded slice.
The existing predicate rule/context gaps remain separate and unqualified.

## source_inspection
# Declared-source metadata preflight

Seven independently authored complete reports cover missing/nonregular first and
later resources, two mixed pairs and all three missing lookup sources. Expected
issues follow FILE-7 in source declaration order. The reference Python runner
was independently exercised with actual removed files/directories and compared
against every field, after identifying only the backend and runtime versions.
The candidate engine did not produce this truth.

Every failure occurs before study bytes: no source read, table, callback,
verification or artifact is observed. Both installed bindings replay this truth;
Python additionally uses descriptor-backed metadata inspection with reference
semantic imports forbidden. Opaque errors, malformed replies and interrupts
stop later authority and are tested separately. Existing explicit byte callbacks
may omit metadata inspection and retain their already qualified capture timing.
This slice does not qualify environment activation or actual R filesystem ports.

## source_typing
# Original source typing truth

Four independently authored documents were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Complete failed reports pin an unknown declared field, unknown-field admission before a bad conversion, stored row/field conversion order despite reversed declarations, and invalid temporal text. Each row retains exact source bytes in the `source_bytes` text column, authored paths, requirements, original failed text and source capture observations. No existing expectation is regenerated.

Core admits immutable type declarations and converts the admitted CSV into owned normalized values. Missing declarations fail in written order before conversion; conversion follows stored row and field order without result handlers. The adapter checks the existing Arrow resource bounds before calling conversion and constructs the physical Arrow table afterward. Core owns both typed-source diagnostic causes; structural defects and policy limits have no fabricated language finding.

Rust and both installed hosts compare complete independent reports after releasing preparation and block repeated failed saves without recapture or publication. Existing exact-width integer, float bits, temporal precision, present/missing text and source-finding collection probes retain their observations. This is private original-document evidence; production filesystem ports, public frontend cutover and inventory promotion remain separate gates.

## static_verification_checks
# Static verification check truth

Twenty independently enumerated issue payloads inspect known verification declaration findings over admitted original YAML without supplying study bytes or a study-data capability. The original raw documents and diagnostic vocabulary come from separately authored full-report truth already compared against reference execution. The static projection adds an explicit ordered column/dataset case and a Unicode pattern case before candidate replay. Five `not_missing` cases are authored from CHECK-4 and CHECK-5 (#3030 D208): a `max_fraction` above one or below zero fails at the field, worded as a `row_count` fraction is, a `when` naming no column or a qualified field fails as `unknown_field` with that identifier, since a `when` reads completed columns alone, and a `when` over a later column with `max_fraction: 0` has no static finding.

Unicode is written with the required escape in ASCII YAML source and decoded into UTF-8 issue text; the source policy is unchanged.

The payload contains only phase, condition, rule, spec_paths and context; context is JSON text encoded once by the shared Rust adapter. Held checks and data-dependent violations have no static issue. Known deferred declarations appear even where a build would first fail conversion or arithmetic. This inspection does not change build checkpoints, promise successful execution, report source-dependent binding findings, or qualify the public filesystem facade.

Both installed hosts compare complete UTF-8 payloads twice while study callbacks are forbidden. A fake engine port separately pins zero study effects and unchanged earlier build failure. The encoder checks the exact byte ceiling, refusal one byte below it and reuse after refusal.

The compound invalid-pattern/later-limit source comes from the independently authored complete build report. Its sole static finding keeps the first pattern path and JSON context without entering study authority. Direct compiled-model coverage also pins an empty verification ID under CHECK-3, including repeated queries.

## run_report_reference
The `specs/schema-verification-log.json` report comes from the installed,
retained Python reference engine's `execute_example(..., backend="python")` on
the `benchmarks/schema-verification-log` document and inputs. Its rendered CSV
files were compared byte for byte with the committed goldens before retaining
the report. Only the backend label and runtime/engine identity placeholders
were changed to the same fixture convention used by the other reports. No
candidate Rust execution supplies this expected data.

When the warning and verification logs became one run report, the benchmark's
report golden `expected/adsl-checks.csv` was re-authored by hand from the
retired log goldens, the specification and its input. The first `artifacts`
entry here is that golden's observation: its exact content, records, byte
length, columns and types. The retired warning-log entry was removed and every
other field is unchanged.

Native and installed Python/R tests compare the complete report, including
source reads, all source and derived cells, all three checks and the actual two
published artifacts in report and primary order. Installed tests also retain
their actual public reports after this comparison. These witnesses qualify this
document; they do not close the remaining public API report support gates.

## window_diagnostics
# Original window diagnostic truth

Two independently authored original documents were captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Complete failed reports pin zero row-value offset and required rank order, including exact requirement IDs, authored paths, context and one captured/ingested `ID,V\n1,2\n` source. No existing expectation is regenerated.

The common core diagnostic owns window context and its canonical cause mapping. Rust and both installed hosts compare complete retained reports after dropping the prepared handle and block repeated save attempts without publication or source recapture. R replay runs with Python unavailable. Named-selection and window predicate findings also project through the common core diagnostic; their existing locked syntax mappings and separately documented original-report gaps are retained.

The three unchanged benchmark documents `negative-formula-flag`,
`negative-row-aggregate` and `negative-row-no-prior` also retain independently
captured complete reports in `specs/`. Their committed error declarations match
exactly. The reference captures and ingests one source before these findings,
despite historical comments describing earlier rejection. Native replay preserves
the actual complete report timing, cached repeats and failed save gates. This is
private original-document evidence; it does not remove the five assisted-route
inventory gaps or promote a public frontend tuple.
