# File resolution

A specification names the files it reads and publishes with local paths. These
rules fix how a path is written, the directory it resolves from, the files it
may reach, and the one snapshot a run reads of each file.

## Resolution base

<a id="file-1"></a>

### FILE-1. A document's relative paths resolve from the file that declares them, never from a discovered root

The caller explicitly supplies the entry file and any project file
([PROJECT-1](../profiles/project.md#project-1)), which lists the study's
function files; these caller arguments follow ordinary host path resolution.
By default, every relative path a document declares resolves from the file
that writes it, including an inherited layer, a producing specification and a
project file. No project configuration is discovered, and a project file does
not select a specification's resource locations.

Evidence: `schema-inheritance-project-root`, `rust/crates/yamaa-application/tests/file_resources.rs::inherited_parent_keeps_its_own_input_and_output_locations_without_configuration`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::project_configuration_is_ignored_and_missing_input_has_no_fallback`, `rust/crates/yamaa/tests/caller_paths.rs::a_specification_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`, `rust/crates/yamaa/tests/caller_paths.rs::a_project_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`, `rust/crates/yamaa/tests/caller_paths.rs::a_parent_path_that_reaches_no_regular_file_is_the_check_s_issue_named_as_given`.

A caller's path argument, the entry, the project file or the parent a
fragment is checked on ([LAYER-4](../inheritance.md#layer-4)), is resolved as
the host resolves a path, except that the file it names must be a regular file
and must not be a symbolic link. One that reaches no entry fails with
`resource_path_missing`, and one that reaches a directory or another entry
that is not a regular file fails with `resource_path_not_regular_file`
([FILE-7](#file-7)); one that is a link, a junction or another reparse point
fails with `resource_path_symlink` ([FILE-8](#file-8)). Each failure names the
path as the caller gave it, at no field ([FILE-11](#file-11)), on every route
that reads the path and through whatever resolver serves it. Any other
failure to read the path is the engine's refusal, `engine_rejected`.

A dataset or output declaration may explicitly select its entry file with
`relative_to: entry` under [FILE-2](#file-2). Composition retains the original
declaring file and written spelling.

With the default `relative_to: declaring`, a shared layer's `input/dm.csv`
names the file beside that shared layer. If that file is absent, a study's own
`input/dm.csv` is not substituted. A shared input with `relative_to: entry`
explicitly names each study entry's own `input/dm.csv`. A project file,
`yamaa-project.yaml` and old project or data-root settings affect neither
selection. Missing-file diagnostics retain the original declaring field and
written path.

<a id="file-2"></a>

### FILE-2. `relative_to` selects the declaring file's or the entry file's directory as a relative path's base

A relative path uses its declaration's `relative_to` base: `declaring` selects
the directory of the file that writes the path, and `entry` selects the
directory of the specification's entry file. On an input dataset the setting
applies to both `path` and `schema`; on `output` it applies to `path` and
`report`. Other document resources use their
declaring file. A rooted path names its absolute local location regardless of
`relative_to`.

Evidence: `rust/crates/yamaa-application/tests/file_resources.rs::entry_relative_shared_program_uses_each_study_input_and_saves_held_artifacts`, `rust/crates/yamaa-application/tests/file_resources.rs::composed_relative_input_base_can_change_without_redeclaring_the_inherited_path`, `rust/crates/yamaa-application/tests/resolved_artifact.rs::two_studies_and_their_schema_links_match_the_actual_direct_preparation`, `python/tests/installed/installed_file_resources.py::test_entry_relative_schema_links_select_each_study_producer_wrapper`, `rust/crates/yamaa-adapters/tests/declaration_language_header.rs::an_entry_relative_schema_resolves_from_the_entry_in_every_layer`.

The schema fixes the closed values and the default. An inherited input or
output uses its composed setting, even when another layer supplied the path
([LAYER-10](../inheritance.md#layer-10)).

A `schema` link selects the producing specification's file, which may inherit
shared layers. A shared consumer can select a study's producer wrapper with
`relative_to: entry`. The producer is built as its own entry, so its own paths
resolve from its own file and settings, never from its consumer's entry
directory.

Composition retains the original declaring file and written spelling.
Materialized paths name the selected location from the entry file. A missing
entry fails at that one selected location. There is no retry from another
directory, a project root, a data root or the process working directory.

<a id="file-3"></a>

### FILE-3. A schema-linked artifact is an ordinary input at its own path, and its producer is never run

A linked input's artifact `path` and its `schema` link both use the input's
selected `relative_to` base under [FILE-2](#file-2). The producing
specification that `schema` names must exist; it is read as metadata only.
The artifact at `path` is an ordinary input that must exist when the consumer
is built. The consumer neither runs the producer nor compares the producer's
`output.path` with its own `path`.

Evidence: `adam-results-ae-incidence`, `sdtm-dm-race-ethnicity`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_csv_artifact_its_producer_saved_first`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_parquet_artifact_its_producer_saved_first`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_stored_artifact_after_its_producers_source_is_gone`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_built_before_its_producer_is_saved_reads_a_missing_csv_artifact`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_built_before_its_producer_is_saved_reads_a_missing_parquet_artifact`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/workflow.json::a_linked_artifact_path_may_differ_from_the_producer_output_path`.

Checking reads no study data, so a link whose artifact does not exist yet
passes `check`, and building fails with the ordinary missing-input finding at
`input.<dataset>.path`. The caller builds and saves the producer as its own
entry first; its explicit Save publishes the artifact under
[Output](../output.md).

A project run is a caller that orders these builds. A producer in its run
scope builds first, and the consumer reads that producer's held serialized
artifact in place of the file at `path`
([PROJECT-8](../profiles/project.md#project-8)). A producer outside the run
scope is not built, and the consumer reads the saved file at `path`
([PROJECT-9](../profiles/project.md#project-9)).

## Written form

<a id="file-4"></a>

### FILE-4. A path is relative or rooted, decided from its text before the filesystem is consulted

A `project_path` is either relative, one or more segments separated by `/`, or
rooted: a leading `/`, or one ASCII letter followed by `:/`, and then one or
more such segments. Path form and syntax are decided before the filesystem is
consulted, and a malformed path fails validation. A written path is ASCII
under [Values](../values.md), like every other repository-authored
value.

Evidence: `rust/crates/yamaa-core/tests/source_csv_precision.rs::written_form_checks_keep_normative_order_and_allow_root_dot_and_parent_paths`, `rust/crates/yamaa-adapters/src/file_metadata.rs::absolute_spelling_covers_both_root_forms_lexically`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::parent_and_absolute_local_paths_build_and_save_held_bytes`, `rust/crates/yamaa-adapters/src/file_resolver.rs::rooted_spellings_split_into_one_root_and_their_segments_on_every_host`, `rust/crates/yamaa-application/tests/network_shares.rs::a_specification_on_a_network_share_builds_and_saves_through_the_share`, `rust/crates/yamaa-application/tests/host_resources.rs::a_parent_on_another_share_is_refused_rather_than_read_from_the_entry_root`.

An absolute local path may name any location permitted by the operating system
and the caller's sandbox. yamaa does not require that location to lie under a
project root or data root. This rule adds no URI or network resource protocol.

The paths a caller passes, the entry and the project file, may name a Windows
network share (`\\server\share\study\spec.yaml`) or a mapped drive. A declared
path keeps the written forms above and never names a share itself; a relative
path, or a drive path that the operating system maps to a share, resolves on
that share.

An inherited layer's relative input and output paths name locations beside
that layer, unless `relative_to: entry` selects the entry, and composition
writes them from the entry ([LAYER-10](../inheritance.md#layer-10)). A
parent step never leaves the entry's root, so a location beside a layer on
another drive is written as its absolute path. A location beside a layer on a
network share that the entry is not on has no written path: composition
refuses that layer with `parent_on_another_share`
([LAYER-10](../inheritance.md#layer-10)) rather than resolving the location
below the entry's root.

<a id="file-5"></a>

### FILE-5. A path has no URI scheme, backslash, empty segment or trailing separator

A `project_path` must not contain a URI scheme: one ASCII letter followed by
`:/` is a drive path, not a scheme, and every other `scheme:` prefix is
prohibited. A `project_path` must not contain `\`, an empty segment or a
trailing separator; an empty path has one empty segment and is invalid.

Fails: `resource_path_uri_scheme`, `resource_path_not_normalized`.
Evidence: `negative-path-url`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_bare_input_path_with_a_backslash_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::an_expanded_input_path_with_a_backslash_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_bare_input_path_with_a_drive_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::an_expanded_input_path_with_a_drive_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_bare_input_path_with_an_empty_segment_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::an_expanded_input_path_with_an_empty_segment_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_bare_input_path_with_a_trailing_slash_is_refused_before_its_source_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::an_expanded_input_path_with_a_trailing_slash_is_refused_before_its_source_profile`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::uri_grammar_preserves_local_absolute_parent_and_windows_drive_paths`, `rust/crates/yamaa-application/tests/host_resources.rs::a_malformed_parent_spelling_is_refused_as_written_on_every_route`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_an_empty_segment_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_a_trailing_slash_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_on_a_network_share_root_is_refused_as_written`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/local_paths.json::a_parent_with_a_backslash_is_refused_as_written`.

A URI scheme, including `file:` and `https:`, fails with
`resource_path_uri_scheme`. A backslash, an empty segment or a trailing
separator fails with `resource_path_not_normalized`.

<a id="file-6"></a>

### FILE-6. `.` and `..` segments resolve textually from the base and may leave it

A `..` segment climbs to the parent directory and a `.` segment stays put.
Relative and absolute paths may contain both, and they resolve textually
before the filesystem is consulted. A relative path starts at the canonical
directory [FILE-2](#file-2) selects, and traversal may leave that directory
without a yamaa containment failure. A parent step at a filesystem root stays
at that root.

Evidence: `schema-inheritance-project-root`, `rust/crates/yamaa-application/tests/file_resources.rs::parent_and_absolute_dot_aliases_share_bytes_and_detect_later_changes`, `rust/crates/yamaa-application/tests/resolved_artifact.rs::absolute_and_parent_relative_paths_are_facts_and_never_fallback_searches`.

## Reaching a file

<a id="file-7"></a>

### FILE-7. Resolution walks each component from a directory anchor and must reach one regular file

Every resolved location has a physical directory anchor and retained component
witnesses. Resolution walks the segments below the anchor one at a time from
that anchor: every component except the last is a directory, and the last
component is a regular file. A path that reaches no entry is missing. A path
that reaches a directory, FIFO, socket, device or any other non-regular file
is rejected.

Fails: `resource_path_missing`, `resource_path_not_regular_file`.
Evidence: `negative-path-missing`, `negative-path-directory`, `rust/crates/yamaa-application/tests/file_resources.rs::absent_artifact_location_has_no_fallback_and_nonregular_input_is_terminal`, `rust/crates/yamaa-adapters/src/file_resolver.rs::walks_report_the_kind_of_the_entry_that_stops_them`, `rust/crates/yamaa/tests/resolver_kinds.rs::every_reported_kind_raises_its_file_rule_condition`, `rust/crates/yamaa/tests/caller_paths.rs::a_specification_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`, `rust/crates/yamaa/tests/caller_paths.rs::a_project_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`, `rust/crates/yamaa/tests/caller_paths.rs::every_kind_a_caller_resolver_reports_for_a_caller_path_raises_its_file_rule_condition`, `rust/crates/yamaa/tests/save_targets.rs::a_save_refuses_a_target_its_walk_cannot_reach_at_the_field_that_declares_it`.

The caller's entry parent, and the parent of the project file the caller
passes, are each canonicalized and opened once; aliases the caller used for a
selected directory retain the same authority. A declaration outside those
parents is walked from its filesystem root: `/`, a drive or a Windows network
share. The anchor is an IO
consistency mechanism, not an application containment policy or fallback
search location. The same resolution and traversal rules apply on every
supported platform.

The resolver that serves a name, the local file resolver unless the caller
supplies another, reports the kind of entry the name reaches: a regular file,
or the first entry that stops it. The engine raises this rule's and
[FILE-8](#file-8)'s conditions from that report alone, so every resolver can
report a missing name, a directory or another entry.

The canonical resolved identity agrees with the directory descriptor and
component witnesses the walk accepted. The check fails closed on an
inconsistent witness. It does not enforce a project or data-root boundary.

<a id="file-8"></a>

### FILE-8. No component below the anchor is a symbolic link

No component below the anchor is a symbolic link, including one whose target
is otherwise readable. The caller-selected parents of [FILE-7](#file-7), the
entry parent and the project file's parent, are exempt: their canonical
directories and descriptors are selected before any specification is read, and
declared paths do not select exempt directories. A path outside those parents
retains the no-link check on every component below its filesystem root.

Fails: `resource_path_symlink`.
Evidence: `negative-path-symlink`, `rust/crates/yamaa-application/tests/file_resources.rs::outside_parent_and_file_links_remain_terminal`, `rust/crates/yamaa-application/tests/file_resources.rs::retired_python_junctions_reject_descent_and_keep_caller_selected_root_authority`, `rust/crates/yamaa-adapters/src/file_resolver.rs::walks_report_junctions_and_reparse_points_as_links`, `rust/crates/yamaa-application/tests/network_shares.rs::links_on_a_network_share_stay_refused`, `rust/crates/yamaa/tests/resolver_kinds.rs::every_reported_kind_raises_its_file_rule_condition`, `rust/crates/yamaa-application/tests/declaration_project_files.rs::a_symbolic_link_in_a_listed_path_fails_as_any_file_a_run_reads`, `rust/crates/yamaa-application/tests/host_resources.rs::a_linked_parent_is_refused_at_parent_on_every_route`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::dictionary_symlink_failure_names_authored_field_without_host_identity`, `rust/crates/yamaa/tests/caller_paths.rs::a_specification_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`, `rust/crates/yamaa/tests/caller_paths.rs::every_kind_a_caller_resolver_reports_for_a_caller_path_raises_its_file_rule_condition`, `rust/crates/yamaa/tests/save_targets.rs::a_save_refuses_a_target_its_walk_cannot_reach_at_the_field_that_declares_it`, `rust/crates/yamaa/tests/save_targets.rs::a_kind_a_caller_resolver_reports_for_a_target_is_refused_as_the_walk_refuses_it`.

Each walk verifies the retained directory and file entries, and a name
substitution cannot redirect a read or publication silently.

A Windows junction or any other reparse point is a symbolic link for this
rule, on a local drive and on a network share alike. Whatever resolver serves
a name reports which kind of link it reached, and the engine refuses each from
that report.

On a network share, the walk sees a link only as the file server reports it.
A link the server reports as a reparse point, as Windows file servers do, is
refused. A server that resolves links itself, such as Samba with its default
`follow symlinks = yes`, serves the link's target in its place, and the walk
reads that target as the entry it reached.

## One snapshot per file

<a id="file-9"></a>

### FILE-9. A run reads one immutable byte snapshot of each physical file, whatever spelling reaches it

A run reads one immutable byte snapshot of each accepted physical file, and
every declaration that reaches the file binds that snapshot. The key is the
file the walk reached: its canonical resolved path, the canonical anchor and
the components the walk accepted, never the spelling a declaration used. A
relative and a rooted spelling of one file therefore produce one key and bind
one snapshot.

Evidence: `sdtm-fa-odm-multitest`, `rust/crates/yamaa-application/tests/file_resources.rs::parent_and_absolute_dot_aliases_share_bytes_and_detect_later_changes`, `rust/crates/yamaa-adapters/src/file_resources.rs::repeated_alias_verification_keeps_original_bytes_and_checks_every_witness`, `rust/crates/yamaa-adapters/src/file_resources.rs::a_hard_link_rewritten_between_captures_is_a_change`, `rust/crates/yamaa-adapters/src/file_resources.rs::a_hard_link_rewritten_after_its_captured_name_is_renamed_is_a_change`.

Several dataset identifiers may resolve to one physical file. The declarations
remain distinct, each with its own identifier and its own
[Inputs](../inputs.md) field types, but they share the one
snapshot, and no two of them observe different bytes.

<a id="file-10"></a>

### FILE-10. Content that changes between validation and ingestion fails the run

An implementation reads the snapshot through the handle opened while
validating, or records the validated bytes and verifies that the bytes are
unchanged before ingestion. It does not re-resolve the written path and read
its later target. Content that changes between validation and ingestion fails
the run; the replacement is not read. Content identity is over bytes: a
modification time, inode number or size is not the identity.

Fails: `resource_path_content_changed`.
Evidence: `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::changed_source_retains_original_snapshot_and_ingest_issue_and_refuses_both_saves`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::already_captured_changed_metadata_keeps_original_snapshot_and_native_failure`, `rust/crates/yamaa-adapters/src/file_resources.rs::reused_numeric_identity_without_a_current_physical_witness_is_a_fresh_capture`, `rust/crates/yamaa-application/tests/submission_service.rs::a_file_changed_before_verification_fails_at_its_field_before_any_target`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::a_file_changed_before_the_entry_is_verified_fails_at_its_field`, `rust/crates/yamaa/tests/plan_reuse.rs::a_changed_specification_compiles_again_and_runs_the_change`.

Before a run activates a project function or reads an input record, it
verifies every file its validation read: the specification, its layers,
dictionaries and linked producing specifications, and the project file with
the function files, lock and codelists it declares. A file that changed fails
the run with `resource_path_content_changed`, naming the field that declared
it and the path as written. The run neither reads the replacement nor
validates it in its place. A later run of the same host object reuses what an
earlier run validated only while every one of those files is unchanged, and
validates again otherwise.

## Path failures

<a id="file-11"></a>

### FILE-11. A path failure names the written path and declaring field and reports exactly one condition

A failure names the written path exactly as the specification wrote it, the
declaring field and one condition below. A message does not contain a
canonical path, a symbolic link's target or any other host path the
specification did not itself write. The written form is decided first, in a
fixed order, then the anchor, then the walk. Every condition is decided before
any data is read and reports under the `validation` phase, except
`resource_path_content_changed`, under `ingest`, and a save's refusal of a
target it walks to, under `output`.

Fails: `resource_path_uri_scheme`, `resource_path_not_normalized`, `resource_path_symlink`, `resource_path_missing`, `resource_path_not_regular_file`, `resource_path_content_changed`.
Evidence: `negative-path-symlink`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::written_form_checks_keep_normative_order_and_allow_root_dot_and_parent_paths`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_missing_input_path_names_its_dataset_and_the_path_as_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_path_that_is_not_a_regular_file_names_its_dataset_and_the_path_as_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_path_with_a_uri_scheme_names_its_dataset_and_the_path_as_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_path_with_a_backslash_is_not_normalized`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_path_with_an_empty_segment_is_not_normalized`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_path_through_a_symbolic_link_names_its_dataset_and_the_path_as_written`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::an_input_whose_content_changed_while_it_was_read_fails_at_ingest`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_missing_target_under_output_at_its_declaring_field`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_target_that_is_not_a_regular_file_under_output_at_its_declaring_field`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_symbolic_link_target_under_output_at_its_declaring_field`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_missing_target_the_caller_names_at_no_field`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_target_the_caller_names_that_is_not_a_regular_file_at_no_field`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/registry.json::a_save_refuses_a_symbolic_link_target_the_caller_names_at_no_field`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_files.json::every_missing_listed_file_and_lock_fails_in_the_order_they_are_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_files.json::a_listed_function_file_that_is_a_directory_fails_at_its_listing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_files.json::a_listed_path_with_a_backslash_fails_in_the_project_before_any_file_is_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_files.json::a_listed_path_with_a_uri_scheme_fails_in_the_project_before_any_file_is_read`, `rust/crates/yamaa/tests/save_targets.rs::the_report_file_is_refused_at_its_own_field_and_a_later_save_retries`, `rust/crates/yamaa/tests/save_targets.rs::a_submission_save_refuses_its_files_at_the_section_define`, `rust/crates/yamaa/tests/save_targets.rs::a_table_saved_to_a_path_its_caller_names_is_refused_at_no_field`, `rust/crates/yamaa/tests/caller_paths.rs::a_listed_specification_the_walk_cannot_reach_fails_at_its_listing_as_written`, `rust/crates/yamaa/tests/caller_paths.rs::a_submission_refuses_a_listed_specification_the_walk_cannot_reach_at_its_listing`, `rust/tests/host-fixtures/cases.json::a_parent_path_that_is_not_normalized_at_the_ancestor_that_writes_it`.

An absolute written path supplied by the specification may be repeated.
Resolved paths retained as provenance are not substituted for the authored
path in these diagnostics. A failure at a layer's `parent` or `include[i]`
also names, as `source`, the layer that writes that field, which may be a
layer above the entry ([LAYER-2](../inheritance.md#layer-2)). A path the
caller passes, the entry, the project file or a given parent, has no declaring
field: its failure names the path as the caller gave it, at no field
([FILE-1](#file-1)).

A save walks to each file it publishes as it publishes the file
([OUTPUT-8](../output.md#output-8)). A directory on the way that does not
exist fails with `resource_path_missing`; one that is not a directory, or a
target that is a directory or another entry that is not a regular file, fails
with `resource_path_not_regular_file`; and a link on the way or at the target
fails with `resource_path_symlink`. Each reports under `output`, at the field
that declares the file and naming it as written: a run's `output.path` or
`output.report`, or a submission section's `define`, which places every file
of the section. A file the caller names, such as a table saved to a path the
caller gives, is named as given, at no field.

The written-form conditions are decided in the order the written-form rules
state them: the form of [FILE-4](#file-4), then a URI scheme, a `\`, and an
empty segment or trailing separator under [FILE-5](#file-5), then the dot
segments of [FILE-6](#file-6). The anchor of [FILE-7](#file-7) follows, then
the walk, so one written path reports one condition on every platform.

| Condition | Rejects |
|---|---|
| `resource_path_uri_scheme` | a URI scheme, including `file:` and `https:` |
| `resource_path_not_normalized` | a form [FILE-5](#file-5) or [FILE-6](#file-6) rejects |
| `resource_path_symlink` | a symbolic link at any component below the anchor |
| `resource_path_missing` | a path that reaches no entry at its declared location |
| `resource_path_not_regular_file` | a directory, FIFO, socket, or device |
| `resource_path_content_changed` | content that changed after validation |
