# Project file

This file is a profile: it is optional, and it applies only to a caller that
passes a project file. A project file names one study's function files, host
locks, terminology and submission sections, and lists the specifications of
each section. Checking, running and packaging a project operate on the
specifications its sections list. A specification built without a project
file is unaffected, except that it has no project functions, no terminology
and no dataset inventory, so each binding that needs one fails
([CT-13](terminology.md#ct-13)).

## The project file

<a id="project-1"></a>

### PROJECT-1. A project file is passed explicitly and names one study's functions, host locks, terminology and submission sections

A project file names one study's function files and host locks, its
terminology and its submission sections. The caller passes it explicitly; it
declares `yamaa` and `type: project`
([SCHEMA-2](../reference/schema-language.md#schema-2)) and is admitted against
the project schema. No project file is discovered from a specification, a
directory or a host default, and a specification cannot select, replace or
override its project. A run without a project file has no project functions,
no terminology and no dataset inventory.

Evidence: `schema-functions`, `sdtm-dm-metadata`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::a_project_lists_function_files_and_holds_each_host_lock`, `rust/crates/yamaa-adapters/tests/declaration_project_files.rs::the_converted_project_files_lower_with_their_written_positions`, `rust/tests/host-fixtures/cases.json::a_listed_function_file_and_codelist_file_that_do_not_exist`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_project_file_that_declares_schema_version_is_refused_as_language_0_5`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/language_header.json::a_project_file_that_declares_neither_header_declares_no_version`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_project_file_that_is_absent_fails_at_the_callers_path`, `rust/crates/yamaa/tests/project_operations.rs::a_project_spelled_through_a_linked_directory_keeps_its_authority`, `rust/crates/yamaa/tests/caller_paths.rs::a_project_path_that_reaches_no_regular_file_fails_with_its_file_rule_on_every_route`.

A project file declares `yamaa`, `type`, `functions`, `environments`,
`codelists`, `study`, and the `sdtm`, `adam` and `send` sections, each optional
except `yamaa` and `type`. A codelist source is written inline or as the path
of a codelist file, which declares `type: codelists`
([CT-1](terminology.md#ct-1)). A project file whose content does not validate
against the project schema fails, before any activation or study read, with
the conditions of [SCHEMA-12](../reference/schema-language.md#schema-12) and
[SCHEMA-13](../reference/schema-language.md#schema-13) at its own paths; the
project file has no condition of its own. A project file that cannot be read
is a path the caller passes: before any activation or study read, it fails
with `resource_path_missing`, `resource_path_not_regular_file` or
`resource_path_symlink`, naming the path as given
([FILE-1](../reference/files.md#file-1)). A specification checked or run
without a project file fails validation at each function call, codelist
binding and inventory vocabulary it declares, all of which only a project file
supplies ([CT-13](terminology.md#ct-13)).

A project file that declares `schema_version` is refused under
[SCHEMA-2](../reference/schema-language.md#schema-2) before any file it names
is read.

<a id="project-2"></a>

### PROJECT-2. `environments` holds each host language's lock, and each host reads only its own

`environments.<host>.lock`, under the host language key `python` or `r`, names
that host's packaging lock, which [FUNC-3](functions.md#func-3) verifies; a
host without a lock needs no entry. Each host reads only the entry its own key
names; the other entry is neither opened nor validated. Function files are
language-neutral ([FUNC-4](functions.md#func-4)), so one project serves both
hosts with one copy of its study metadata and its functions.

Evidence: `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file`, `python/tests/installed/installed_public_environment.py::test_each_host_reads_only_its_own_lock`, `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::an_engine_without_a_host_language_reads_no_environments_entry`.

An absent entry means the host declares no lock, and its calls run without a
package-version audit ([FUNC-3](functions.md#func-3)); a call in a host with no
implementation fails under [FUNC-1](functions.md#func-1).

<a id="project-3"></a>

### PROJECT-3. A section needs only `specs`, and the study metadata a run does not read is required only by Define-XML

`codelists` supplies the study terminology
[Controlled terminology](terminology.md) owns, `study` names the study, and
each `sdtm`, `adam` or `send` section lists in `specs` the specifications of
that model with the submission metadata [Define-XML](define-xml.md) composes.
A section needs only `specs`. `study` and the section's `standards` and
`define` are required only when Define-XML composes the section
([DEFINE-28](define-xml.md#define-28)). A run does not open a section's
document links or generate a Define-XML document.

Evidence: `sdtm-dm-metadata`, `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::define_reports_the_missing_study_guide_and_target_before_reading_specifications`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::define_reports_a_missing_target_before_reading_specifications`.

A project can therefore list its specifications and run them before its
submission metadata is ready. A section's `transport` requests the transport
files of [Dataset-JSON](dataset-json.md#json-1) or
[SAS Transport](xpt.md#xpt-1), which only a project submission writes
([PROJECT-13](#project-13)). A section's `standards` is a list, its first
entry the guide a specification binds by default; a specification selects
another by name in `submission.standard`
([DEFINE-8](define-xml.md#define-8)).

<a id="project-4"></a>

### PROJECT-4. A project file's relative paths resolve from the project file

Every relative path the project file writes, namely each function file, lock,
codelist source, section specification and Define-XML target, resolves from
the directory of the project file. Absolute local paths and parent segments
have their [File resolution](../reference/files.md) meaning.

Fails: `resource_path_missing`.
Evidence: `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file`, `rust/crates/yamaa-adapters/tests/declaration_project_files.rs::a_project_reads_a_codelist_file_it_lists_relative_to_itself`, `python/tests/installed/installed_public_environment.py::test_project_codelists_keep_project_locations_and_missing_files_fail_there`.

An absent file fails at its one selected location, without discovery or a
fallback search. The project file does not change where a specification's own
resources resolve under [FILE-1](../reference/files.md#file-1).

## Specification names

<a id="project-5"></a>

### PROJECT-5. A listed specification's name is its `domain`, unique across the project

The name of a project specification is the `domain` value of a specification
that a section lists in `specs`. Only listed specifications have names: a
producer that a `schema` link reaches without a listing has none. Names are
unique across the project, every section included. Two listings with one name,
one file listed twice included, fail with `duplicate_specification_name` at
the project path of each listing, such as `sdtm.specs[0]`.

Fails: `duplicate_specification_name`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::names_select_listed_specifications_and_refuse_unknown_or_shared_names`, `rust/crates/yamaa-adapters/tests/project_scope.rs::listings_follow_section_run_order_and_names_are_their_domains`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::one_file_listed_twice_shares_its_name_with_itself`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_name_two_listings_share_is_refused_at_both`.

Every project operation reports that finding; a project run or submission then
builds nothing, and selecting a specification by name fails.

<a id="project-6"></a>

### PROJECT-6. Selecting a project specification by name accepts only a name the project defines

Selecting a project specification by name accepts only a name
[PROJECT-5](#project-5) defines. An unknown name fails with
`unknown_specification_name`, naming the requested name and, in sorted order,
every name the project defines. A selected specification is checked, run and
resolved exactly as it is when the caller names its file together with the
project file.

Fails: `unknown_specification_name`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::names_select_listed_specifications_and_refuse_unknown_or_shared_names`, `rust/crates/yamaa/tests/project_operations.rs::a_project_run_refuses_a_name_it_did_not_build`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::an_unknown_specification_name_lists_the_available_names_sorted`.

Selecting one specification's run from a project run by name accepts only a
name of that run. Any other name, one that another section lists included,
fails with `unknown_specification_name`, naming the requested name and, in
sorted order, every name the run has.

## Selecting a section

<a id="project-14"></a>

### PROJECT-14. Selecting a project section accepts only a section the project declares

A project run or submission that selects a section accepts only a section the
project file declares. Any other name fails with `unknown_section` before any
specification builds, naming the requested name and, in sorted order, every
section the project declares, whether the name is outside `sdtm`, `adam` and
`send` or names one of them that the project does not declare.

Fails: `unknown_section`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_section_the_project_does_not_declare_refuses_the_call`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_section_the_project_does_not_declare_lists_the_declared_sections_sorted`, `python/tests/installed/installed_public_contract.py::test_project_refusals_are_issues_and_unknown_sections_raise`, `python/tests/installed/installed_public_define.py::test_every_declared_define_section_is_composed_without_a_section`.

The project file is read first: a project file that cannot be read reports
its own findings as the operation's issues, as every project operation does,
and no section is checked. Section names compare exactly, so `SDTM` names no
section. A declared section whose `specs` is empty is selected, and a project
run of it builds nothing ([PROJECT-12](#project-12)). Without a selected
section, a project run builds every declared section
([PROJECT-8](#project-8)) and a submission composes every declared section
that names `define` ([PROJECT-13](#project-13)).

The finding has no spec path. Its `section` context is the requested name,
and its `sections` context lists the declared sections.

## Checking a project

<a id="project-7"></a>

### PROJECT-7. Checking a project checks every listed specification in section and listing order, with the project findings

Checking a project statically checks every listed specification, in section
order (`sdtm`, `adam`, `send`) and then listing order, exactly as checking
that specification with the project file does: with the project's terminology
and implementation environment, and without study data or project code. The
check also reports the project findings of [PROJECT-5](#project-5),
[PROJECT-10](#project-10) with every listed specification in scope, and
[PROJECT-12](#project-12) among the files of every listed specification.

Evidence: `rust/crates/yamaa/tests/project_operations.rs::project_check_reports_every_specification_and_the_project_findings`, `python/tests/installed/installed_public_environment.py::test_project_check_validates_original_studies_without_study_inputs`.

Each finding names the listed specification it belongs to in its `entry`
context and the document it comes from in its `source` context.

Every project finding reports under the `validation` phase and is decided
before the specification it concerns reads study data.
`duplicate_specification_name` names the project path of each listing, and
`unlinked_producer` and `producer_not_completed` name the input field of the
consumer they fail.

## Project runs

<a id="project-8"></a>

### PROJECT-8. A project run builds each specification of its run scope once, a linked producer before its consumers

A project run builds the specifications one selected section lists, or those
of every declared section in the order `sdtm`, `adam`, `send`: its run scope.
Each builds exactly once, as its own run with its own output, findings and run
report, reported under its name. A `schema` link to a run-scope specification
is a dependency: the producer builds first, and a consumer whose `path` names
the producer's `output.path` location reads the held serialized artifact
instead of that file. Independent specifications keep run-scope order.

Fails: `producer_workflow_cycle`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_section_run_builds_only_its_scope_and_a_full_run_hands_producers_over_in_memory`, `rust/crates/yamaa-adapters/tests/project_scope.rs::plans_order_their_scope_producer_first`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::unlinked_inputs_and_a_cycle_refuse_no_whole_run`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_link_cycle_refuses_the_first_specification_in_it`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_link_cycle_refuses_the_second_specification_in_it`.

The selected section must be one the project declares
([PROJECT-14](#project-14)). The project run is the caller that orders these
builds. The link itself only types a stored artifact
([INPUT-10](../inputs.md#input-10)): a specification built on its own never
builds a producer, and a project run never builds a producer outside its run
scope ([PROJECT-9](#project-9)). The held artifact is read like a stored one,
checked against the producer's output contract under
[INPUT-11](../inputs.md#input-11), and the consumer's report records its
encoded length ([CHECK-24](../verification.md#check-24)).

A dependency cycle among run-scope specifications fails each specification on
it with `producer_workflow_cycle`, at the `input.<dataset>.schema` of its link
to the next specification on the cycle; its `cycle` context names the
specifications from the first one entered back to itself. No specification on
the cycle builds.

<a id="project-9"></a>

### PROJECT-9. A producer outside the run scope is not built, and its consumer reads the saved file

In a project run, a `schema` link to a specification outside the run scope
does not build that producer. The consumer reads the file its `path` names,
the producer's saved artifact, and checks it against the producer's output
contract under [INPUT-11](../inputs.md#input-11). An absent file fails as a
missing input at `input.<dataset>.path`.

Fails: `resource_path_missing`, `producer_contract_mismatch`, `artifact_path_collision`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_producer_outside_the_scope_is_read_from_its_saved_artifact_and_checked`, `rust/crates/yamaa/tests/project_operations.rs::a_specification_may_not_write_the_saved_artifact_it_reads`, `rust/crates/yamaa-adapters/tests/project_scope.rs::saved_links_read_held_or_saved_artifacts_under_the_producer_contract`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_linked_artifact_neither_held_nor_saved_is_a_missing_file`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_saved_artifact_with_another_header_mismatches_the_producer_contract`.

The consumer may not write the files it reads: its `output.path` and
`output.report` locations may not name the producer's, as
[OUTPUT-9](../output.md#output-9) states for every linked producer.

<a id="project-10"></a>

### PROJECT-10. An input that reads a run-scope artifact without a `schema` link fails

An input without `schema` whose resolved `path` names the `output.path`
location of a run-scope specification fails with `unlinked_producer`, naming
that specification. Without the link the input would read the artifact an
earlier run saved, not the one this run builds. An input that names no
run-scope artifact is an ordinary source.

Fails: `unlinked_producer`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::an_input_that_reads_a_run_scope_artifact_without_a_link_fails_alone`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_specification_outside_a_cycle_and_its_unlinked_reads_is_not_refused`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::in_the_adam_scope_alone_the_sdtm_artifact_is_an_ordinary_source`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_run_scope_artifact_read_without_a_link_is_an_unlinked_producer`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_project_check_reports_an_unlinked_input_with_every_listing`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::an_input_that_reads_a_run_scope_artifact_without_a_link_names_its_producer`.

The finding fails only the consumer that declares the input, at its
`input.<dataset>.path`; the other specifications of the run scope still build.

<a id="project-11"></a>

### PROJECT-11. A failed run-scope specification stops only the specifications that depend on it

A run-scope specification that fails does not stop the specifications that do
not depend on it. A consumer whose run-scope producer failed or did not build
does not run: it fails with `producer_not_completed`, naming that producer, and
builds no output and no run report. Every specification that ran keeps its own
output, findings and run report.

Fails: `producer_not_completed`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_failed_specification_stops_only_its_consumers_and_saves_reports_alone`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_producer_the_consumer_does_not_read_blocks_nothing`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/project_scope.json::a_consumer_is_blocked_by_the_producer_that_failed`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_consumer_of_a_producer_that_did_not_complete_names_the_producer`.

The finding is reported at the consumer's `input.<dataset>.schema` for each
link to a producer that did not complete.

## Saving a project run

<a id="project-12"></a>

### PROJECT-12. Saving a project run validates every target first, then publishes specification by specification in run order

No two `output.path` or `output.report` locations of the run-scope
specifications may name one file; a collision fails the project run with
`artifact_path_collision` before any specification builds. Saving validates
every target before touching any, then publishes specification by
specification in run order, each declared report immediately before its
primary artifact. When any specification failed, the save publishes no primary
artifact: it replaces, in run order, the report file of every specification
that ran and declares one, then fails with every run's findings.

Fails: `artifact_path_collision`, `nothing_to_save`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::every_target_is_validated_before_the_first_file_is_written`, `rust/crates/yamaa/tests/project_operations.rs::a_file_that_cannot_be_staged_stops_the_save_before_any_target_is_replaced`, `rust/crates/yamaa/tests/project_operations.rs::a_save_that_would_write_nothing_fails`, `rust/crates/yamaa-adapters/tests/project_scope.rs::the_preflight_refuses_two_results_that_write_one_file`, `rust/crates/yamaa-adapters/tests/project_scope.rs::report_only_saves_write_the_report_file_and_no_primary_artifact`, `rust/tests/host-fixtures/cases.json::nothing_to_save_issues`.

Locations compare as [OUTPUT-9](../output.md#output-9) compares the two
paths of one specification, two spellings of one file included. Each
specification orders its own files as [OUTPUT-10](../output.md#output-10)
states. Every file is staged beside its target before the first target is
replaced, so a file that cannot be staged stops the save with no target
replaced. A failure while replacing one file stops the publication there: the
files already replaced remain, and no later target is touched. A project run
that built no specification has nothing to save, and saving it fails with
`nothing_to_save`.

## Project submissions

<a id="project-13"></a>

### PROJECT-13. A project submission composes each selected section's Define-XML document in section order

A project submission composes one [Define-XML](define-xml.md) document for the
selected section, or one for each declared section that names `define`, in the
order `sdtm`, `adam`, `send`, together with the transport files of a section
that declares `transport` ([JSON-1](dataset-json.md#json-1)). Its findings are
those of its sections in that order. Saving publishes the sections in that
order; the first section whose publication fails stops the save.

Fails: `nothing_to_save`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_submission_composes_the_selected_section`, `rust/crates/yamaa/tests/project_operations.rs::a_submission_without_a_section_composes_each_section_that_names_define_in_order`, `rust/crates/yamaa/tests/project_operations.rs::two_sections_that_write_one_file_save_neither`, `rust/crates/yamaa/tests/project_operations.rs::a_later_section_whose_target_cannot_be_written_saves_nothing`, `rust/crates/yamaa/tests/project_operations.rs::a_save_that_would_write_nothing_fails`, `rust/crates/yamaa/tests/project_operations.rs::a_submission_gives_every_section_one_creation_time_and_records_it`, `python/tests/installed/installed_public_define.py::test_every_declared_define_section_is_composed_without_a_section`, `rust/tests/host-fixtures/cases.json::nothing_to_save_issues`.

Every section composes with one creation time: the `created` the submission is
given or, when it is omitted, the current UTC time, read once before the
project file is read. So every document and transport file of the submission
carries the same time ([DEFINE-3](define-xml.md#define-3)), and the result
records it as `created`.

Saving first admits and compares the targets of every section it publishes: a
target that cannot be written, or two sections that name one file, fail the
save before any file is written, and the section that holds the target reports
and records that failure. A section that did not compose publishes nothing and
stops the save at its turn, after the sections before it. A submission that
composed no section has nothing to save, and saving it fails with
`nothing_to_save`. Each section publishes its transport files before its
document under [JSON-14](dataset-json.md#json-14).

The selected section must be one the project declares
([PROJECT-14](#project-14)), so a section that the project does not declare is
refused before any Define-XML document is composed.

<a id="project-15"></a>

### PROJECT-15. A project submission composes a specification that declares a run report only with the artifact and inputs that report records

When a project submission composes a section that lists a specification
declaring `output.report`, it reads that report before any transport file.
The report's `run` row must record a successful run that wrote the
specification's `output.path` and as many rows as the artifact holds; the
artifact must hold rows exactly when `submission.has_no_data` is not declared;
and each declared input's `input` row must record the byte length and record
count the input has now. Any mismatch fails with `stale_artifact`.

Fails: `stale_artifact`.
Evidence: `negative-stale-artifact`, `rust/crates/yamaa-core/src/submission_staleness.rs::each_mismatch_names_its_reason_and_the_facts_it_compared`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs::a_saved_report_reads_back_its_run_and_its_inputs`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_matching_report_composes`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_specification_without_a_report_is_not_compared`, `rust/crates/yamaa/tests/project_operations.rs::a_submission_composes_the_artifacts_their_run_reports_record_and_refuses_stale_ones`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_report_that_records_another_artifact_than_the_section_composes_is_stale`.

The comparison uses declared identities only, the facts the report records
under [CHECK-24](../verification.md#check-24), and no timestamp or digest: the
report's `ARTIFACT` against `output.path` as the run records it, with inherited
paths respelled relative to the entry under [LAYER-10](../inheritance.md#layer-10),
the `run` row's `COUNT` against the rows the artifact holds, and each input
row's `COUNT` and `DETAILS.bytes` against the records the input's profile
decodes from its bytes now and their length. Declaring a report adds nothing
to it, and a specification that declares no `output.report` composes without
a comparison.

The comparison follows every fault of the document itself and precedes the
transport files. Specifications compare in entry order, each its `run` row,
then its artifact, then its inputs in declaration order, and the first
mismatch is reported. The report is read under the profile its extension
selects ([CHECK-22](../verification.md#check-22)). A declared report or input
that cannot be read fails as any declared file does
([FILE-7](../reference/files.md#file-7)), and an artifact as a published one
does ([JSON-5](dataset-json.md#json-5)). A file that is not a run report
records no run, so it fails with `stale_artifact`.

The finding is reported at the listing's `output.report`, such as
`sdtm.specs[0].output.report`. Its context names the dataset (`dataset`), the
report as declared (`report`) and the mismatch (`reason`). `recorded` holds a
value as the report records it and `actual` the value composition reads now:

| `reason` | Mismatch | Further context |
| --- | --- | --- |
| `report_invalid` | the file is not a run report | none |
| `artifact_path` | the `run` row names another `output.path` | `recorded`, `actual` |
| `run_failed` | the `run` row records a failed run, which saved no artifact | `recorded`: the run's condition |
| `row_count` | the artifact holds another number of rows | `recorded`, `actual` |
| `has_no_data` | the artifact holds rows and `has_no_data` is declared, or holds none and it is not | `has_no_data`, `actual`: the rows |
| `input_missing` | a declared input has no `input` row | `input` |
| `input_bytes` | the input's byte length differs | `input`, `recorded`, `actual` |
| `input_records` | its record count differs, `actual` `null` when its bytes no longer decode | `input`, `recorded`, `actual` |

Running the specification again saves a report and an artifact that agree
([PROJECT-12](#project-12)).
