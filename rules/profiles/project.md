# Project file

This file is a profile: it is optional, and it applies only to a caller that
passes a project file. A project file names one study's implementation
environments, terminology and submission sections, and lists the
specifications of each section. Checking, running and packaging a project
operate on the specifications its sections list. A specification built without
a project file is unaffected, except that it has no implementation
environment, no terminology and no dataset inventory.

## The project file

<a id="project-1"></a>

### PROJECT-1. A project file is passed explicitly and names one study's environments, terminology and submission sections

A project file names one study's implementation environments, terminology and
submission sections. The caller passes it explicitly, and it is admitted
against `schema_project.yaml`, whose `schema_version` selects the schema
bundle. No project file is discovered from a specification, a directory or a
host default, and a specification cannot select, replace or override its
project. A run without a project file has no implementation environment, no
terminology and no dataset inventory.

Fails: `project_environment_missing`, `project_environment_invalid`.
Evidence: `schema-functions`, `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/project_scope.rs::an_unreadable_project_fails_as_a_check_reports_it`, `rust/crates/yamaa/tests/project_operations.rs::a_project_spelled_through_a_linked_directory_keeps_its_authority`.
Replaces: REQ-1331, REQ-1335.

A project file declares `schema_version`, `environments`, `codelists`,
`study`, and the `sdtm`, `adam` and `send` sections, each optional except
`schema_version`. A project file that cannot be read fails with
`project_environment_missing` ([FUNC-1](functions.md#func-1)), and one that
does not validate against `schema_project.yaml` fails with
`project_environment_invalid` ([FUNC-2](functions.md#func-2)), before any
activation or study read. Its `schema_version` follows
[SCHEMA-2](../reference/schema-language.md#schema-2).

<a id="project-2"></a>

### PROJECT-2. `environments` links each host language's environment file, and each host reads only its own

`environments` maps a host language key, `python` or `r`, to that language's
environment file under [Project functions](functions.md). The key states the
language of the file it names, and an environment file declares no language of
its own. Each host reads only the entry its own key names; the other entry is
neither opened nor validated, and one project serves both hosts with one copy
of its study metadata.

Fails: `project_environment_missing`.
Evidence: `python/tests/installed/installed_public_environment.py::test_each_host_reads_only_its_own_environment_entry`, `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::a_project_without_an_environment_for_the_host_leaves_other_languages_unread`.
Replaces: REQ-1332, REQ-1336.

An absent entry leaves that host without an implementation environment. A
specification that calls a project function then cannot execute, and the call
fails with `project_environment_missing` under [FUNC-1](functions.md#func-1).

<a id="project-3"></a>

### PROJECT-3. A section needs only `specs`, and the study metadata a run does not read is required only by Define-XML

`codelists` supplies the study terminology
[Controlled terminology](terminology.md) owns, `study` names the study, and
each `sdtm`, `adam` or `send` section lists in `specs` the specifications of
that family with the submission metadata [Define-XML](define-xml.md) composes.
A section needs only `specs`. `study` and the section's `standard` and `define`
are required only when Define-XML composes the section
([DEFINE-28](define-xml.md#define-28)). A run does not open a section's
document links or generate a Define-XML document.

Evidence: `sdtm-dm-metadata`, `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::define_reports_every_missing_study_section_guide_and_target_before_reading_specifications`.
Replaces: REQ-1333.

A project can therefore list its specifications and run them before its
submission metadata is ready. A section's `datasets` requests the transport
files of [Dataset-JSON](dataset-json.md#json-1) or
[SAS Transport](xpt.md#xpt-1), which only a project submission writes
([PROJECT-13](#project-13)).

<a id="project-4"></a>

### PROJECT-4. A project file's relative paths resolve from the project file, and an environment file's from the environment file

Every relative path the project file writes, namely each environment link,
codelist source, section specification and Define-XML target, resolves from
the directory of the project file. Every relative path an environment file
writes, namely its lock and its function definition files, resolves from that
environment file. Absolute local paths and parent segments have their
[File resolution](../reference/files.md) meaning.

Fails: `resource_path_missing`.
Evidence: `rust/crates/yamaa-core/tests/preparation_project_preparation.rs::lock_and_function_paths_resolve_from_the_environment_file_and_codelists_from_the_project`, `python/tests/installed/installed_public_environment.py::test_project_codelists_keep_project_locations_and_missing_files_fail_there`.
Replaces: REQ-1334.

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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::names_select_listed_specifications_and_refuse_unknown_or_shared_names`, `rust/crates/yamaa-adapters/tests/project_scope.rs::listings_follow_section_run_order_and_names_are_their_domains`, `rust/crates/yamaa-core/tests/support/diagnostic/project_scope.rs`.
Replaces: REQ-1338.

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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::names_select_listed_specifications_and_refuse_unknown_or_shared_names`, `rust/crates/yamaa-core/tests/support/diagnostic/project_scope.rs`.
Replaces: REQ-1339.

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
Replaces: REQ-1340, REQ-1348.

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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_section_run_builds_only_its_scope_and_a_full_run_hands_producers_over_in_memory`, `rust/crates/yamaa-adapters/tests/project_scope.rs::plans_order_their_scope_producer_first`, `rust/crates/yamaa-adapters/tests/project_scope.rs::unlinked_inputs_cycles_and_collisions_are_project_findings`.
Replaces: REQ-1330, REQ-1341, REQ-1342.

The project run is the caller that orders these builds. The link itself only
types a stored artifact ([INPUT-10](../inputs.md#input-10)): a specification
built on its own never builds a producer, and a project run never builds a
producer outside its run scope ([PROJECT-9](#project-9)). The held artifact is
read like a stored one, checked against the producer's output contract under
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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_producer_outside_the_scope_is_read_from_its_saved_artifact_and_checked`, `rust/crates/yamaa/tests/project_operations.rs::a_specification_may_not_write_the_saved_artifact_it_reads`, `rust/crates/yamaa-adapters/tests/project_scope.rs::saved_links_read_held_or_saved_artifacts_under_the_producer_contract`.
Replaces: REQ-1343.

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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::an_input_that_reads_a_run_scope_artifact_without_a_link_fails_alone`, `rust/crates/yamaa-adapters/tests/project_scope.rs::unlinked_inputs_cycles_and_collisions_are_project_findings`, `rust/crates/yamaa-core/tests/support/diagnostic/project_scope.rs`.
Replaces: REQ-1344.

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
Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_failed_specification_stops_only_its_consumers_and_saves_reports_alone`, `rust/crates/yamaa-adapters/tests/project_scope.rs::blocked_consumers_name_each_failed_producer`, `rust/crates/yamaa-core/tests/support/diagnostic/project_scope.rs`.
Replaces: REQ-1345.

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

Fails: `artifact_path_collision`.
Evidence: `rust/crates/yamaa/tests/project_operations.rs::every_target_is_validated_before_the_first_file_is_written`, `rust/crates/yamaa/tests/project_operations.rs::a_save_that_would_write_nothing_fails`, `rust/crates/yamaa-adapters/tests/project_scope.rs::the_preflight_refuses_two_results_that_write_one_file`, `rust/crates/yamaa-adapters/tests/project_scope.rs::report_only_saves_write_the_report_file_and_no_primary_artifact`.
Replaces: REQ-1346.

Locations compare as [OUTPUT-9](../output.md#output-9) compares the two
paths of one specification, two spellings of one file included. Each
specification orders its own files as [OUTPUT-10](../output.md#output-10)
states. A failure while replacing one file stops the publication there: the
files already replaced remain, and no later target is touched. A project run
that built no specification has nothing to save, and saving it fails.

## Project submissions

<a id="project-13"></a>

### PROJECT-13. A project submission composes each selected section's Define-XML document in section order

A project submission composes one [Define-XML](define-xml.md) document for the
selected section, or one for each declared section that names `define`, in the
order `sdtm`, `adam`, `send`, together with the transport files of a section
that declares `datasets` ([JSON-1](dataset-json.md#json-1)). Its findings are
those of its sections in that order. Saving publishes the sections in that
order; the first section whose publication fails stops the save.

Evidence: `rust/crates/yamaa/tests/project_operations.rs::a_submission_composes_the_selected_section`, `rust/crates/yamaa/tests/project_operations.rs::a_submission_without_a_section_composes_each_section_that_names_define_in_order`, `rust/crates/yamaa/tests/project_operations.rs::two_sections_that_write_one_file_save_neither`, `rust/crates/yamaa/tests/project_operations.rs::a_save_that_would_write_nothing_fails`, `python/tests/installed/installed_public_define.py::test_every_declared_define_section_is_composed_without_a_section`.
Replaces: REQ-1347.

Saving first compares the targets of every section: two sections that name
one file fail the save before any file is written. A section whose own target
cannot be admitted is left to its own publication, which reports and records
that failure. A submission that composed no section has nothing to save, and
saving it fails. Each section publishes its transport files before its
document under [JSON-14](dataset-json.md#json-14).
