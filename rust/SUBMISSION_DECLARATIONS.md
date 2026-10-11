# yamaa submission declaration metadata

`yamaa_core::submission_document::decode` admits one complete set of study and
submission declarations from an immutable normalized `Document`. This is the
metadata prerequisite tracked by #1888 under #1758. Existing schema admission
continues to own original schema failures; this decoder preserves its exact
borrowed input on every structural or resource refusal.

The closed owned result retains study ID, name, description and protocol name;
all shipped implementation-guide names, versions and Draft/Final status; and
sdtm, adam and send sections in original root mapping order. Each section keeps
specifications and document references in declaration order, Define-XML path,
document ID/kind/link/title, Dataset-JSON flag, context, language, and existing
file OID, metadata version, originator, stylesheet and source-system-version overrides. Paths remain
text declarations. The decoder performs no filesystem operation.

Existing omitted defaults are false for `dataset_json`, Submission for
`context`, en for `xml_lang`, and an empty document list. Required fields remain
required. Explicit null values never become omitted defaults. A present study
or section is decoded completely; an absent study or section stays absent.
The [shared Define composition](DEFINE_GENERATION.md) checks generator
prerequisites and family compatibility after this declaration admission.

Every owned field carries its normalized container, key and value occurrence
indices. Defaulted omitted fields have no key or value occurrence. These facts
describe the supplied normalized document. They do not assert a written file
origin, or claim that a present normalized default was authored in YAML. Unknown
and non-text fields also retain exact container/key facts on refusal. The owned
accepted values remain available after the original document is dropped.
Malformed document-list items retain their actual sequence container, absent
mapping key and item node, alongside the section/documents/index semantic path.
The section's `documents` mapping occurrence continues to identify the sequence.

A non-owning pass reserves cumulative field, declaration, node, list member,
text, depth and work capacity before model ownership. The same budget covers
all selected subtrees, every root key, failed lookups and existing default text;
it is never replaced between sections. A fixed maximum of 64 bounds the native
walk, in addition to the caller's depth limit. Work and escaped-text capacity
are conservatively reserved for the entire admission and owned decode. Escaping
reserves at most ten bytes per UTF-8 byte plus quotes for every selected scalar
and scanned key. No renderer participates in this component.

The structural pass counts the entire finding set and all closed path segments
before allocating any finding buffer. Any capacity failure returns a typed
resource refusal beside the exact original input, with no fitting prefix. If
structural findings exist, no model strings or lists are built. Otherwise one
owned decode builds the admitted complete model from the same immutable input.
Paths use fixed closed field/index segments; they require no diagnostic strings.
Supported invalid scalar causes expose current REQ-1046, REQ-1068, REQ-1070,
REQ-1073 and REQ-1074 identities. Other choices, missing fields, original shapes
and transport defects acquire no fabricated requirement cause.

This model supplies no execution plan, activation, callable resolution, study
or artifact read, rendering, publisher or Save capability. It does not derive
generator identities, compose submission semantics, bind codelists or capture
recursive specification files. The separate [shared Define use case](DEFINE_GENERATION.md) supplies those
services for the bounded column and value-level metadata profile. Specification
submission admission owns typed row overrides and their nonempty text IDs,
retains original occurrence facts, and reserves the complete selected metadata
set before cloning. Composition performs inheritance and effective semantics.
The old Define-XML implementation and independently authored XML/JSON expected bytes remain
until the complete replacement and transition qualify.

The approved #1887 decision permits a nonempty `source_system_version` string
in each section. The decoder owns its exact bounded text and normalized
occurrences; explicit null, empty and non-text values refuse without selecting a
default. Omission keeps the field absent. `Section::source_versions` borrows one
selected metadata version for both XML and JSON alongside the actual installed
generator version supplied by the calling package. An empty installed version
refuses even when an override is present; core's crate version is never substituted.

This model owns declaration admission and version selection. The shared
column and typed value-level metadata generator and independently completed
fixtures advance #1758; full contract qualification and coordinated reference
loader/schema retirement remain open. Existing independent XML/JSON
truth stays unchanged. Producer publication and Windows R retain their own
implementation and qualification gates.
