# Submission schemas

The Define-XML 2.1 and ODM 1.3.2 XSD declarations are retained from
[CDISC's data-definition-engine](https://github.com/cdisc-org/data-definition-engine/tree/924e5a9ec2a4e77bd8f6280f83066bf9f7d37e50/schemas).
The schemas are distributed by CDISC. The upstream MIT license is included
verbatim, including its original copyright placeholder. The ODM import in
`define/2.1/define-enumerations.xsd` points to `../../odm/1.3.2/` so both the
complete Define schema and the enumeration schema load without network requests.
The DefineVersion enumeration admits
the declared 2.1 maintenance release; the namespace remains v2.1.

The Dataset-JSON schema is copied unchanged from
[CDISC's DataExchange-DatasetJson](https://github.com/cdisc-org/DataExchange-DatasetJson/blob/a379f49a3f43c2aaed63bdeca761bdb7140df2c3/schema/dataset.schema.json).
Its license is included as `DATASET-JSON-LICENSE`.
