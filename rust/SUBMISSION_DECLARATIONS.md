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
file OID, metadata version, originator and stylesheet overrides. Paths remain
text declarations. The decoder performs no filesystem operation.

Existing omitted defaults are false for `dataset_json`, Submission for
`context`, en for `xml_lang`, and an empty document list. Required fields remain
required. Explicit null values never become omitted defaults. A present study
or section is decoded completely; an absent study or section stays absent.
Generator prerequisites and family compatibility remain #1758 semantic work.

Every owned field carries its normalized container, key and value occurrence
indices. Defaulted omitted fields have no key or value occurrence. These facts
describe the supplied normalized document. They do not assert a written file
origin, or claim that a present normalized default was authored in YAML. Unknown
and non-text fields also retain exact container/key facts on refusal. The owned
accepted values remain available after the original document is dropped.

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
recursive specification files. Those services and installed define APIs remain
#1758. The old Define-XML implementation and independently authored XML/JSON
expected bytes remain until that complete use case qualifies.

#1887 asks how to preserve historical source-system-version bytes beside the
installed runtime version. No source version field, schema override or expected
byte change is inferred while that decision is unanswered. #1866 publication
lifecycle and #1857 Windows R scope remain independent decisions.
