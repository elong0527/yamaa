# yamaa specification submission declarations

`yamaa_core::specification_submission_document::decode` owns complete closed
dataset and ordered column submission declarations from one immutable normalized
`Document`. This is #1893's metadata prerequisite under #1758. It neither invokes
the ordinary execution compiler nor relaxes its unsupported submission gates.
Existing schema transport and source preparation keep their separate authority.

The dataset keeps label, class, subclass, structure, repeating, reference-data
flag, domain and plain or structured comment beside `has_no_data`. Omitted
`has_no_data` and `reference_data` default to false; explicit null refuses.
Absence of the dataset declaration stays absent. An absent column list also
stays absent in this selected metadata view; ordinary schema admission still
requires the full specification's columns.

Each column keeps its declared name, type and optional label, in original list
order, whether or not it has submission metadata. Submission fields preserve
core, mandatory, role, declared Define-XML type, length, significant digits,
display format, codelist, inventory-vocabulary flag, origin, method and comment.
The complete shipped closed vocabularies are admitted without selecting a
submission family or resolving declared types. Integer metadata remains exact
canonical arbitrary-width decimal text, including negative and zero values;
later semantic admission owns range and combination checks.

Origin metadata keeps type, source, description and ordered document references.
Methods and comments retain their plain-string or structured alternatives.
Plain strings may be empty under the current schema; required structured
description/text fields are nonempty. Structured methods retain optional name,
type (Computation by default), formal context/code and document references.
Document references keep plain identifiers or structured document/pages fields;
pages keep PhysicalRef or NamedDestination type, refs and optional title.
Absent document lists are empty. Omitted optional booleans remain absent rather
than silently acquiring false. Explicit null does not select an omitted default.

Root, column and closed-class field order is retained with normalized container,
key and value occurrence indices. Missing defaults have no key/value occurrence.
These facts do not claim that normalized fields were authored in YAML, or assign
written file origins unavailable from a core Document. Accepted owned values and
facts remain after the original document drops. Native inherited capture and
written-origin pairing remain later #1758 work.

One cumulative non-owning pass admits selected fields, nodes, declarations,
column/list counts, scalar/key text, conservative escaping, depth and work before
ownership grows. Every root/column key scan is charged before lookups; unselected
execution domains are left to their own admission. Malformed selected trees
also receive complete resource admission, with a fixed maximum depth of 64.
Mapping-order buffers and boxed alternatives are covered by field/list and node
counts. Text escaping reserves ten bytes per UTF-8 byte plus quotes. The separate
`locate` helper admits root/column fields, columns, declaration count and work
before owning only location facts; null facts grant no acceptance. Full decoding
does not reset its budget through that helper.

The structural pass admits the complete finding count and all fixed closed path
segments before the first finding allocation. Limits return the exact borrowed
input and typed refusal; no fitting finding prefix is called complete. On invalid
metadata no model strings, lists or boxes are allocated. Otherwise one final
owned decode uses the same immutable input. Findings retain original unknown and
non-text key facts. Only invalid identifiers and the currently supported closed
scalar vocabularies expose REQ-1046 and REQ-1141 through REQ-1147. Missing fields,
shapes, other choices and transport defects acquire no invented normative cause.

There is no code resolution, activation, study/artifact read, rendering, publisher
or Save capability. Type/length resolution, derivation combinations, family
composition, document binding, bind-only codelists, derived identities, native
recursive capture, row-output metadata overlays and installed define output remain
#1758. Old Define-XML and
independent expected XML/JSON bytes remain unchanged. #1887's version decision,
#1866's publication lifecycle and #1857's Windows R scope remain unanswered;
this metadata component introduces no source-system-version override.
