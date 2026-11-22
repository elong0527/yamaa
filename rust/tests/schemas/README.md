# Submission schemas

The Define-XML 2.0, Define-XML 2.1 and ODM 1.3.2 XSD declarations are retained from
[CDISC's data-definition-engine](https://github.com/cdisc-org/data-definition-engine/tree/924e5a9ec2a4e77bd8f6280f83066bf9f7d37e50/schemas).
The schemas are distributed by CDISC. The upstream MIT license is included
verbatim, including its original copyright placeholder. The ODM import in
`define/2.1/define-enumerations.xsd` points to `../../odm/1.3.2/` so both the
complete Define schema and the enumeration schema load without network requests.
The DefineVersion pattern admits every 2.1 maintenance release; the namespace
remains v2.1. CDISC recommends replacing earlier 2.1 enumeration schemas with
the maintained schema when controlled terminology is updated. The version
selector preserves the requested published release in `def:DefineVersion`;
the default remains 2.1.11.

No schema of Analysis Results Metadata 1.0 is retained here. CDISC publishes
its three files only inside its download packages, which carry no open
license. A Define-XML schema alone has no place for
`arm:AnalysisResultDisplays`, so the Define tests validate a document that
carries it after setting that one element aside, and compare the element's
bytes with an authored expected document
([DEFINE-25](../../../rules/profiles/define-xml.md#define-25)).

The Dataset-JSON schema is copied unchanged from
[CDISC's DataExchange-DatasetJson](https://github.com/cdisc-org/DataExchange-DatasetJson/blob/a379f49a3f43c2aaed63bdeca761bdb7140df2c3/schema/dataset.schema.json).
Its license is included as `DATASET-JSON-LICENSE`.
