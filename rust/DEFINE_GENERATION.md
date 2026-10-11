# Shared Define generation

`yamaa.define(environment, standard, created=...)` and
`yamaa_define(environment, standard, created=...)` transport one native owned
use case. The selector is exactly `sdtm`, `adam` or `send`. Shared core
composition admits the selected study, section, resolved specification
metadata and terminology; shared writers produce Define-XML 2.1.11 and
Dataset-JSON 1.1.0. Hosts supply the actual installed package version and
convert owned issues and result handles. R performs no Python call.

Preparation captures environment metadata, codelist sources and the selected
specifications with their parent layers. Relative paths use each original
declaring YAML location. The use case acquires no activation, callback or
execution port. Ordinary specification input files and supporting document
links are never opened. Only an explicit `dataset_json: true` request captures
the already published CSV or Parquet artifacts at each original `output.path`.
Its JSON target is adjacent, with the same basename and `.json` extension.
The current package-reference rules require each JSON target and Define target
to share a directory; parent traversal in either reference fails before any
published dataset is captured.

Composition checks the complete ordered dataset set, output metadata, family,
keys, origins and derivation refutations, method/document references, types,
lengths and codelist binding. Only bound lists and their source standards are
emitted. A collected Subject/Investigator column requires one annotated CRF;
omitted origin document references select that CRF. Submission context retains
its alias and transport-name requirements. Literal supporting links may name
files that do not exist yet.

Row-level `submission` overrides inherit every absent field from the selected
column declaration. Each covered row requires a declared `<DOMAIN>TESTCD`
string literal and an effective derivation for every selected output column.
Row IDs remain arbitrary nonempty text. Identical declarations for one
(column, test code) pair describe one value; conflicting declarations refuse.
Effective family, origin/refutation, method/document, type and terminology
admission is shared with the base columns. One value list per column links
ordered per-value items and where clauses using the bundled XSD's structure.
When Dataset-JSON is requested, each held row's discriminator selects the
effective per-value codelist and inventory vocabulary; an unmatched code
retains the column declaration.

An omitted FileOID is `study.id.section.Define-XML_2.1`; omitted metadata-version
ID and name are `study.id.section` and `study.name section metadata`. Section
keys are lower case. Explicit admitted overrides replace those defaults.
FileOID is nonempty text and retains URL-style spellings in the existing DM
golden. Generated leaf IDs retain XML ID admission separately. SourceSystem
is `yamaa`; the section's nonempty `source_system_version` selects XML and JSON
metadata, defaulting to the actual installed package version. Installed
qualification records the actual generator version independently of that
metadata override. Explicit creation time is a whole-second civil datetime;
omission captures current UTC during preparation.

The closed XML construction fixes element/attribute order, required structure
and reference forms. Shared generation checks its scalar schema facets,
generated identifiers, references and XML character/whitespace admission
before returning bytes. Installed Python witnesses independently validate the
three original fixture outputs against the unchanged official offline XSD
and DM JSON against the unchanged Dataset-JSON schema. DM also compares both
outputs with the committed goldens byte for byte. AE/VS use independently authored fixed-time
schema and structure expectations, without generating new expected bytes or
importing the reference renderer.

The separate LB metadata completion preserves its original specification and
CSV. It supplies complete column facts and an explicit SDTM-valid Sponsor
source for the creatinine override; the original Assigned/Investigator claim
remains an independent family-rejection control. Fixed-time LB XML structure,
offline XSD, Dataset-JSON schema/rows and selected-value codelist refusal have
independently authored witnesses in both installed hosts.

The complete output set is rendered and target-admitted before publication.
Metadata or artifact failure leaves every target untouched. Save preflights
every target again, compares retained physical parent identities and filenames
for the complete set, publishes JSON in specification order and XML last, and
uses held bytes without reopening metadata or published datasets. Each file
has atomic replacement under the existing publisher. A later operational
failure retains the original cause and any completed files, marks its failed
target and stops the remainder. There is no package-wide rollback. Python
raises `DefineError`; R signals `yamaa_define_error`; both carry the shared
current issue rows. Successful repeated Save returns false without writes;
failed results remain failed.

Lexical package references retain the written filename spelling. Windows uses
the file adapter's ordinal case comparison for lexical collision and package
parent admission. Save also refuses distinct spellings of one physical target
before any write. On macOS the publisher queries the held parent's case
sensitivity for ASCII names. A shared-parent pair containing a non-ASCII basename
is conservatively refused because this bounded publisher has no Unicode
normalization comparer; that profile limit is not a broader release exclusion.
Metadata-only supporting leaves may remain absent during composition.

Existing descriptor, regular-file, link, held-byte snapshot and resource-budget
checks remain. Captured source bytes are cumulatively bounded at 64 MiB; metadata
also retains the existing 1,024-snapshot limit and explicit text/work capacities.
Composition admits at most 1,024 selected datasets, 524,288 document nodes and
16 MiB of selected specification text, with a cumulative 268,435,456-unit
conservative semantic-work reservation before cloning metadata. Each JSON table
has at most 16,777,216 cells, and vocabulary comparison reserves bounded
cumulative work before scanning values. Complete retained rendered publication bytes are
bounded at 64 MiB and a capacity refusal returns an issue before any write.

This slice advances column and typed value-level metadata generation alongside
the original reference inputs. Full language acceptance and transition remain
open under #1758. The historical renderer/loaders, `define.yaml`, schema,
benchmark tooling and expected bytes are preserved for the transition gate.
They are not invoked by either new installed entry point. This implementation
does not close the complete issue or authorize release cutover.
