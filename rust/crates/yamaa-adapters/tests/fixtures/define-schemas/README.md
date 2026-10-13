# Define schema qualification fixtures

Installed Define qualification stages these independent standards schemas beside
its fixtures and validates native XML and JSON without network requests. Their
schema bytes and licenses are preserved from the retired reference package.

The Define-XML 2.1 and ODM 1.3.2 XSD files are copied unchanged from
[CDISC's data-definition-engine](https://github.com/cdisc-org/data-definition-engine/tree/924e5a9ec2a4e77bd8f6280f83066bf9f7d37e50/schemas).
The upstream MIT license is included. Relative paths are preserved so
validation needs no network requests. The DefineVersion enumeration admits
the declared 2.1 maintenance release; the namespace remains v2.1.

The Dataset-JSON schema is copied unchanged from
[CDISC's DataExchange-DatasetJson](https://github.com/cdisc-org/DataExchange-DatasetJson/blob/a379f49a3f43c2aaed63bdeca761bdb7140df2c3/schema/dataset.schema.json).
Its license is included as `DATASET-JSON-LICENSE`.
