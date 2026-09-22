# Schema fields

<!-- generated: generate_rule_reference.py -->

This table is a generated view of schema shape and defaults. Follow the
requirement link for behavior. It is not an additional semantic contract.

## schema.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `root_class.schema_version` | `"str"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.domain` | `"identifier"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.keys` | `"list[identifier]"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.input` | `"dict[identifier, dataset_source]"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.base` | `"identifier"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.parents` | `["path", "list[path]"]` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.intermediates` | `"list[intermediate_class]"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.output` | `"output_class"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.columns` | `"list[column_class]"` | `true` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.rows` | `"list[row_class]"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.verifications` | `["dataset_verification", "list[dataset_verification]"]` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.submission` | `"submission_dataset_class"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `root_class.metadata` | `"dict[str, str]"` | `false` | Absent | -- | [REQ-1042](../specification/structure.md#req-1042) |
| `output_class.path` | `"path"` | `true` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `output_class.decimals` | `"int"` | `false` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `output_class.columns` | `"list[identifier]"` | `true` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `output_class.warning_log` | `"path"` | `false` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `output_class.verification_log` | `"path"` | `false` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `output_class.order_by` | `"list[order_by_term]"` | `false` | Absent | -- | [REQ-1047](../storage/publication.md#req-1047) |
| `intermediate_class.id` | `"intermediate_id"` | `true` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.dataset` | `"identifier"` | `true` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.key` | `["identifier", "list[identifier]"]` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.key_base` | `["variable", "list[variable]"]` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.between` | `"intermediate_between_class"` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.order_by` | `"list[order_by_term]"` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.keep` | `"str"` | `false` | Absent | `{"values": ["first", "last"]}` | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.columns` | `"list[identifier]"` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.strict` | `"bool"` | `false` | `false` | -- | [REQ-1048](../operations/lookup.md#req-1048) |
| `intermediate_class.derivations` | `"dict[identifier, derivation]"` | `false` | Absent | -- | [REQ-1185](../operations/lookup.md#req-1185) |
| `intermediate_between_class.value` | `"variable"` | `true` | Absent | -- | [REQ-1049](../operations/lookup.md#req-1049) |
| `intermediate_between_class.lower` | `"identifier"` | `true` | Absent | -- | [REQ-1049](../operations/lookup.md#req-1049) |
| `intermediate_between_class.upper` | `"identifier"` | `true` | Absent | -- | [REQ-1049](../operations/lookup.md#req-1049) |
| `column_class.name` | `"identifier"` | `true` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.type` | `"column_type"` | `true` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.label` | `"str"` | `false` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.derivation` | `"derivation"` | `false` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.verifications` | `["column_verification", "list[column_verification]"]` | `false` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.submission` | `"submission_column_class"` | `false` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `column_class.metadata` | `"dict[str, str]"` | `false` | Absent | -- | [REQ-1043](../specification/structure.md#req-1043) |
| `row_class.id` | `"row_id"` | `true` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `row_class.dataset` | `"identifier"` | `false` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `row_class.group_by` | `"list[variable]"` | `false` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `row_class.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `row_class.derivations` | `"dict[identifier, derivation]"` | `true` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `row_class.submission` | `"dict[identifier, submission_column_class]"` | `false` | Absent | -- | [REQ-1056](../execution/rows.md#req-1056) |
| `variable` | `"str"` | `false` | Absent | `{"pattern": "^[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*$"}` | [REQ-1057](../specification/binding.md#req-1057) |
| `dataset_source` | `["project_path", "dataset_class"]` | `false` | Absent | -- | [REQ-1059](../storage/ingestion.md#req-1059) |
| `dataset_class.path` | `"project_path"` | `true` | Absent | -- | [REQ-1060](../storage/ingestion.md#req-1060) |
| `dataset_class.types` | `"dict[identifier, column_type]"` | `false` | Absent | -- | [REQ-1060](../storage/ingestion.md#req-1060) |
| `dataset_class.schema` | `"project_path"` | `false` | Absent | -- | [REQ-1060](../storage/ingestion.md#req-1060) |
| `dataset_class.empty_string` | `"str"` | `false` | `"missing"` | `{"values": ["missing", "present"]}` | [REQ-1158](../storage/ingestion.md#req-1158) |
| `intermediate_id` | `"str"` | `false` | Absent | `{"pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}` | [REQ-1050](../operations/lookup.md#req-1050) |
| `row_id` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1044](../specification/structure.md#req-1044) |
| `regex` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1058](../specification/binding.md#req-1058) |
| `predicate` | `"str"` | `false` | Absent | `{"min_length": 1}` | Schema constraint |

## schema_define.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `define_class.schema_version` | `"str"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.define_version` | `"define_version"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.context` | `"str"` | `true` | Absent | `{"values": ["Submission", "Other"]}` | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.language` | `"language_tag"` | `false` | `"en"` | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.file_oid` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.creation_datetime` | `"creation_datetime"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.originator` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.source_system` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.source_system_version` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.stylesheet` | `"relative_href"` | `false` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.study` | `"study_class"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.metadata_version` | `"metadata_version_class"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.standards` | `"list[standard_class]"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.default_standard` | `"identifier"` | `false` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.documents` | `"list[document_class]"` | `false` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.codelists` | `"list[codelist_class]"` | `false` | Absent | -- | [REQ-1076](../submission/terminology.md#req-1076) |
| `define_class.datasets` | `"list[define_dataset_class]"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `define_class.output` | `"define_output_class"` | `true` | Absent | -- | [REQ-1061](../submission/define-xml.md#req-1061) |
| `study_class.id` | `"define_id"` | `true` | Absent | -- | [REQ-1062](../submission/define-xml.md#req-1062) |
| `study_class.name` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1062](../submission/define-xml.md#req-1062) |
| `study_class.description` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1062](../submission/define-xml.md#req-1062) |
| `study_class.protocol_name` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1062](../submission/define-xml.md#req-1062) |
| `metadata_version_class.id` | `"define_id"` | `true` | Absent | -- | [REQ-1063](../submission/define-xml.md#req-1063) |
| `metadata_version_class.name` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1063](../submission/define-xml.md#req-1063) |
| `metadata_version_class.description` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1063](../submission/define-xml.md#req-1063) |
| `standard_class.id` | `"identifier"` | `true` | Absent | -- | [REQ-1064](../submission/define-xml.md#req-1064) |
| `standard_class.name` | `"standard_name"` | `true` | Absent | -- | [REQ-1064](../submission/define-xml.md#req-1064) |
| `standard_class.type` | `"str"` | `true` | Absent | `{"values": ["IG", "CT"]}` | [REQ-1064](../submission/define-xml.md#req-1064) |
| `standard_class.publishing_set` | `"str"` | `false` | Absent | `{"values": ["ADaM", "CDASH", "DEFINE-XML", "SDTM", "SEND"]}` | [REQ-1064](../submission/define-xml.md#req-1064) |
| `standard_class.version` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1064](../submission/define-xml.md#req-1064) |
| `standard_class.status` | `"str"` | `true` | Absent | `{"values": ["Draft", "Final", "Provisional"]}` | [REQ-1064](../submission/define-xml.md#req-1064) |
| `document_class.id` | `"identifier"` | `true` | Absent | -- | [REQ-1065](../submission/define-xml.md#req-1065) |
| `document_class.kind` | `"str"` | `true` | Absent | `{"values": ["annotated_crf", "supplemental", "other"]}` | [REQ-1065](../submission/define-xml.md#req-1065) |
| `document_class.href` | `"relative_href"` | `true` | Absent | -- | [REQ-1065](../submission/define-xml.md#req-1065) |
| `document_class.title` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1065](../submission/define-xml.md#req-1065) |
| `codelist_class.id` | `"identifier"` | `true` | Absent | -- | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.name` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.standard` | `"identifier"` | `false` | Absent | -- | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.data_type` | `"str"` | `false` | `"text"` | `{"values": ["text", "integer", "float"]}` | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.extensible` | `"bool"` | `false` | `false` | -- | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.alias` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.format_name` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.items` | `"list[codelist_item_class]"` | `false` | Absent | -- | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_class.external` | `"external_codelist_class"` | `false` | Absent | -- | [REQ-1077](../submission/terminology.md#req-1077) |
| `codelist_item_class.value` | `"literal_value"` | `true` | Absent | -- | [REQ-1078](../submission/terminology.md#req-1078) |
| `codelist_item_class.decode` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1078](../submission/terminology.md#req-1078) |
| `codelist_item_class.rank` | `"int"` | `false` | Absent | -- | [REQ-1078](../submission/terminology.md#req-1078) |
| `codelist_item_class.alias` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1078](../submission/terminology.md#req-1078) |
| `codelist_item_class.extended` | `"bool"` | `false` | `false` | -- | [REQ-1078](../submission/terminology.md#req-1078) |
| `external_codelist_class.dictionary` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1079](../submission/terminology.md#req-1079) |
| `external_codelist_class.version` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1079](../submission/terminology.md#req-1079) |
| `external_codelist_class.href` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1079](../submission/terminology.md#req-1079) |
| `define_dataset_class.id` | `"identifier"` | `true` | Absent | -- | [REQ-1066](../submission/define-xml.md#req-1066) |
| `define_dataset_class.spec` | `"project_path"` | `true` | Absent | -- | [REQ-1066](../submission/define-xml.md#req-1066) |
| `define_dataset_class.standard` | `"identifier"` | `false` | Absent | -- | [REQ-1066](../submission/define-xml.md#req-1066) |
| `define_dataset_class.has_no_data` | `"bool"` | `false` | `false` | -- | [REQ-1066](../submission/define-xml.md#req-1066) |
| `define_dataset_class.dataset_json` | `"dataset_json_path"` | `false` | Absent | -- | [REQ-1066](../submission/define-xml.md#req-1066) |
| `define_output_class.path` | `"define_path"` | `true` | Absent | -- | [REQ-1067](../submission/define-xml.md#req-1067) |
| `define_id` | `"str"` | `false` | Absent | `{"pattern": "^[A-Za-z_][A-Za-z0-9_.-]*$"}` | [REQ-1068](../submission/define-xml.md#req-1068) |
| `define_version` | `"str"` | `false` | Absent | `{"pattern": "^2\\.1\\.(0\|[1-9][0-9]*)$"}` | [REQ-1069](../submission/define-xml.md#req-1069) |
| `relative_href` | `"str"` | `false` | Absent | `{"pattern": "^(?!/)(?!.*(^\|/)\\.\\.(/\|$))[^\\\\]+$"}` | [REQ-1070](../submission/define-xml.md#req-1070) |
| `define_path` | `"str"` | `false` | Absent | `{"pattern": "^[^/\\\\][^\\\\]*\\.[Xx][Mm][Ll]$"}` | [REQ-1071](../submission/define-xml.md#req-1071) |
| `dataset_json_path` | `"str"` | `false` | Absent | `{"pattern": "^[^/\\\\][^\\\\]*\\.[Jj][Ss][Oo][Nn]$"}` | [REQ-1225](../submission/dataset-json.md#req-1225) |
| `creation_datetime` | `"str"` | `false` | Absent | `{"pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$"}` | [REQ-1072](../submission/define-xml.md#req-1072) |
| `language_tag` | `"str"` | `false` | Absent | `{"pattern": "^[A-Za-z]{2,3}(-[A-Za-z0-9]{1,8})*$"}` | [REQ-1073](../submission/define-xml.md#req-1073) |
| `standard_name` | `"str"` | `false` | Absent | `{"values": ["ADaMIG", "ADaMIG-MD", "BIMO", "CDISC/NCI", "SDTMIG", "SDTMIG-AP", "SDTMIG-MD", "SENDIG", "SENDIG-AR", "SENDIG-DART", "SENDIG-GENETOX"]}` | [REQ-1074](../submission/define-xml.md#req-1074) |

## schema_derivation.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `derivation` | `["str", "expression", "handled_expression_class"]` | `false` | Absent | -- | Schema constraint |
| `handled_expression_class.value` | `"expression"` | `true` | Absent | -- | Schema constraint |
| `handled_expression_class.missing` | `"literal_value"` | `false` | Absent | -- | Schema constraint |
| `handled_expression_class.strict` | `"bool"` | `false` | Absent | -- | Schema constraint |

## schema_environment.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `environment_class.schema_version` | `"str"` | `true` | Absent | -- | [REQ-1080](../operations/functions.md#req-1080) |
| `environment_class.version` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1080](../operations/functions.md#req-1080) |
| `environment_class.runtime` | `"project_runtime_class"` | `true` | Absent | -- | [REQ-1080](../operations/functions.md#req-1080) |
| `environment_class.functions` | `"dict[identifier, function_contract_class]"` | `true` | Absent | -- | [REQ-1080](../operations/functions.md#req-1080) |
| `project_runtime_class.language` | `"str"` | `true` | Absent | `{"values": ["r", "python"]}` | [REQ-1081](../operations/functions.md#req-1081) |
| `project_runtime_class.artifact` | `"runtime_artifact_class"` | `true` | Absent | -- | [REQ-1081](../operations/functions.md#req-1081) |
| `runtime_artifact_class.reference` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1082](../operations/functions.md#req-1082) |
| `function_contract_class.contract` | `"path"` | `false` | Absent | -- | Schema constraint |
| `function_contract_class.contract_version` | `"function_contract_version"` | `false` | Absent | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.implementation_version` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.description` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.comparison_decimals` | `"int"` | `false` | `4` | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.may_return_missing` | `"bool"` | `false` | `false` | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.params` | `"list[function_parameter_class]"` | `false` | Absent | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.returns` | `"column_type"` | `false` | Absent | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.binding` | `"function_binding_class"` | `true` | Absent | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `function_contract_class.conformance` | `"path"` | `true` | Absent | -- | [REQ-1083](../operations/functions.md#req-1083) |
| `shared_function_contract_class.contract_version` | `"function_contract_version"` | `true` | Absent | -- | Schema constraint |
| `shared_function_contract_class.description` | `"str"` | `true` | Absent | `{"min_length": 1}` | Schema constraint |
| `shared_function_contract_class.comparison_decimals` | `"int"` | `false` | `4` | -- | Schema constraint |
| `shared_function_contract_class.may_return_missing` | `"bool"` | `false` | `false` | -- | Schema constraint |
| `shared_function_contract_class.params` | `"list[function_parameter_class]"` | `true` | Absent | -- | Schema constraint |
| `shared_function_contract_class.returns` | `"column_type"` | `true` | Absent | -- | Schema constraint |
| `function_parameter_class.name` | `"identifier"` | `true` | Absent | -- | Schema constraint |
| `function_parameter_class.type` | `"function_param_type"` | `true` | Absent | -- | Schema constraint |
| `function_parameter_class.required` | `"bool"` | `false` | `true` | -- | Schema constraint |
| `function_parameter_class.default` | `"function_value"` | `false` | Absent | -- | Schema constraint |
| `function_parameter_class.accepts_missing` | `"bool"` | `false` | `false` | -- | Schema constraint |
| `function_binding_class.call` | `"qualified_callable"` | `true` | Absent | -- | [REQ-1084](../operations/functions.md#req-1084) |
| `function_binding_class.args` | `"dict[identifier, host_argument_name]"` | `false` | Absent | -- | [REQ-1084](../operations/functions.md#req-1084) |
| `function_conformance_class.schema_version` | `"str"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_class.function` | `"identifier"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_class.contract_version` | `"function_contract_version"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_class.cases` | `"list[function_conformance_case_class]"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_case_class.id` | `"function_case_id"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_case_class.covers` | `"list[function_coverage]"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_case_class.args` | `"dict[identifier, function_value]"` | `true` | Absent | -- | Schema constraint |
| `function_conformance_case_class.result` | `"function_value"` | `true` | Absent | -- | Schema constraint |
| `qualified_callable` | `"str"` | `false` | Absent | `{"min_length": 1}` | Schema constraint |
| `host_argument_name` | `"str"` | `false` | Absent | `{"min_length": 1}` | Schema constraint |
| `function_case_id` | `"str"` | `false` | Absent | `{"pattern": "^[a-z][a-z0-9-]*$"}` | Schema constraint |
| `function_coverage` | `"str"` | `false` | Absent | `{"pattern": "^(normal\|boundary\|nullable-output\|numeric-comparison\|(default\|accepted-missing\|short-circuit-missing\|boolean-true\|boolean-false):[A-Za-z_][A-Za-z0-9_]*)$"}` | Schema constraint |

## schema_expression_aggregate.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.aggregate` | `["aggregate_expression", "aggregate_class"]` | `false` | Absent | -- | Schema constraint |
| `aggregate_class.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `aggregate_class.between` | `"aggregate_between_class"` | `false` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `aggregate_class.group_by` | `"list[variable]"` | `false` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `aggregate_class.key` | `["identifier", "list[identifier]"]` | `false` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `aggregate_class.key_base` | `["variable", "list[variable]"]` | `false` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `aggregate_class.derive` | `"list[derive_binding_class]"` | `false` | Absent | -- | [REQ-1189](../operations/aggregation.md#req-1189) |
| `aggregate_class.expr` | `"aggregate_expression"` | `true` | Absent | -- | [REQ-1088](../operations/aggregation.md#req-1088) |
| `derive_binding_class.name` | `"identifier"` | `true` | Absent | -- | [REQ-1192](../operations/aggregation.md#req-1192) |
| `derive_binding_class.type` | `"column_type"` | `true` | Absent | -- | [REQ-1192](../operations/aggregation.md#req-1192) |
| `derive_binding_class.derivation` | `"derivation"` | `true` | Absent | -- | [REQ-1192](../operations/aggregation.md#req-1192) |
| `aggregate_expression` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1089](../operations/aggregation.md#req-1089) |
| `aggregate_between_class.value` | `"variable"` | `true` | Absent | -- | [REQ-1090](../operations/aggregation.md#req-1090) |
| `aggregate_between_class.lower` | `"variable"` | `false` | Absent | -- | [REQ-1090](../operations/aggregation.md#req-1090) |
| `aggregate_between_class.upper` | `"variable"` | `false` | Absent | -- | [REQ-1090](../operations/aggregation.md#req-1090) |

## schema_expression_core.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.source` | `["variable", "source_binding_class"]` | `false` | Absent | -- | [REQ-1093](../operations/expressions.md#req-1093) |
| `expressions.literal` | `"literal_value"` | `false` | Absent | -- | [REQ-1094](../operations/expressions.md#req-1094) |
| `expressions.first_available.sources` | `"list[filtered_source]"` | `true` | Absent | -- | [REQ-1095](../operations/expressions.md#req-1095) |
| `expressions.first_available.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1095](../operations/expressions.md#req-1095) |
| `expressions.greatest.sources` | `"list[variable]"` | `true` | Absent | -- | [REQ-1096](../operations/expressions.md#req-1096) |
| `expressions.least.sources` | `"list[variable]"` | `true` | Absent | -- | [REQ-1097](../operations/expressions.md#req-1097) |
| `expressions.case` | `"list[case_item_class]"` | `false` | Absent | -- | [REQ-1098](../operations/expressions.md#req-1098) |
| `source_binding_class.variable` | `"variable"` | `true` | Absent | -- | [REQ-1051](../operations/lookup.md#req-1051) |
| `source_binding_class.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1051](../operations/lookup.md#req-1051) |
| `source_binding_class.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1051](../operations/lookup.md#req-1051) |
| `source_binding_class.multiple_matches` | `"multiple_matches_class"` | `false` | Absent | -- | [REQ-1051](../operations/lookup.md#req-1051) |
| `filtered_source` | `["variable", "filtered_source_class"]` | `false` | Absent | -- | [REQ-1052](../operations/lookup.md#req-1052) |
| `filtered_source_class.variable` | `"variable"` | `true` | Absent | -- | [REQ-1053](../operations/lookup.md#req-1053) |
| `filtered_source_class.filter` | `"predicate"` | `true` | Absent | -- | [REQ-1053](../operations/lookup.md#req-1053) |
| `multiple_matches_class.order_by` | `"list[order_by_term]"` | `true` | Absent | -- | [REQ-1054](../operations/lookup.md#req-1054) |
| `multiple_matches_class.keep` | `"str"` | `true` | Absent | `{"values": ["first", "last"]}` | [REQ-1054](../operations/lookup.md#req-1054) |
| `order_by_term` | `["variable", "order_term_class"]` | `false` | Absent | -- | [REQ-1102](../execution/ordering.md#req-1102) |
| `order_term_class.variable` | `"variable"` | `true` | Absent | -- | [REQ-1103](../execution/ordering.md#req-1103) |
| `order_term_class.direction` | `"str"` | `false` | `"asc"` | `{"values": ["asc", "desc"]}` | [REQ-1103](../execution/ordering.md#req-1103) |
| `order_term_class.nulls` | `"str"` | `false` | `"last"` | `{"values": ["first", "last"]}` | [REQ-1103](../execution/ordering.md#req-1103) |
| `case_item_class` | `["case_branch_class", "case_otherwise_class"]` | `false` | Absent | -- | [REQ-1099](../operations/expressions.md#req-1099) |
| `case_branch_class.when` | `"predicate"` | `true` | Absent | -- | [REQ-1100](../operations/expressions.md#req-1100) |
| `case_branch_class.then` | `"case_result"` | `true` | Absent | -- | [REQ-1100](../operations/expressions.md#req-1100) |
| `case_otherwise_class.otherwise` | `"case_result"` | `true` | Absent | -- | [REQ-1101](../operations/expressions.md#req-1101) |
| `case_result` | `["str", "expression"]` | `false` | Absent | -- | Schema constraint |

## schema_expression_date.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.date_diff.start` | `"variable"` | `true` | Absent | -- | [REQ-1104](../operations/temporal.md#req-1104) |
| `expressions.date_diff.end` | `"variable"` | `true` | Absent | -- | [REQ-1104](../operations/temporal.md#req-1104) |
| `expressions.date_diff.unit` | `"str"` | `true` | Absent | `{"values": ["day", "week", "month", "year"]}` | [REQ-1104](../operations/temporal.md#req-1104) |
| `expressions.date_diff.bounds` | `"str"` | `false` | `"exclusive"` | `{"values": ["exclusive", "inclusive", "between"]}` | [REQ-1104](../operations/temporal.md#req-1104) |
| `expressions.date_impute.source` | `"variable"` | `true` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.month` | `"int"` | `true` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.day` | `["int", "day_rule"]` | `true` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.minimum_source_precision` | `"str"` | `false` | `"year"` | `{"values": ["year", "month"]}` | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.not_before` | `"variable"` | `false` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_impute.invalid` | `"literal_value"` | `false` | Absent | -- | [REQ-1105](../operations/temporal.md#req-1105) |
| `expressions.date_precision.source` | `"variable"` | `true` | Absent | -- | [REQ-1106](../operations/temporal.md#req-1106) |
| `expressions.date_precision.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1106](../operations/temporal.md#req-1106) |
| `expressions.date_precision.invalid` | `"literal_value"` | `false` | Absent | -- | [REQ-1106](../operations/temporal.md#req-1106) |
| `expressions.datetime_impute.source` | `"variable"` | `true` | Absent | -- | [REQ-1182](../operations/temporal.md#req-1182) |
| `expressions.datetime_impute.time` | `"time_rule"` | `true` | Absent | -- | [REQ-1182](../operations/temporal.md#req-1182) |
| `expressions.datetime_impute.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1182](../operations/temporal.md#req-1182) |
| `expressions.datetime_impute.invalid` | `"literal_value"` | `false` | Absent | -- | [REQ-1182](../operations/temporal.md#req-1182) |
| `expressions.datetime_precision.source` | `"variable"` | `true` | Absent | -- | [REQ-1183](../operations/temporal.md#req-1183) |
| `expressions.datetime_precision.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1183](../operations/temporal.md#req-1183) |
| `expressions.datetime_precision.invalid` | `"literal_value"` | `false` | Absent | -- | [REQ-1183](../operations/temporal.md#req-1183) |
| `expressions.to_date.source` | `"variable"` | `true` | Absent | -- | [REQ-1107](../operations/temporal.md#req-1107) |
| `expressions.study_day.date` | `"variable"` | `true` | Absent | -- | [REQ-1108](../operations/temporal.md#req-1108) |
| `expressions.study_day.reference` | `"variable"` | `true` | Absent | -- | [REQ-1108](../operations/temporal.md#req-1108) |
| `expressions.to_epoch_day.source` | `"variable"` | `true` | Absent | -- | [REQ-1188](../operations/temporal.md#req-1188) |
| `day_rule` | `"str"` | `false` | Absent | `{"values": ["first", "last"]}` | [REQ-1109](../operations/temporal.md#req-1109) |
| `time_rule` | `"str"` | `false` | Absent | `{"values": ["first", "last"]}` | [REQ-1184](../operations/temporal.md#req-1184) |

## schema_expression_mapping.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.mapping.source` | `"filtered_source"` | `true` | Absent | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.mapping.dict` | `"dict[str, literal_value]"` | `false` | Absent | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.mapping.dict_yaml` | `"project_path"` | `false` | Absent | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.mapping.case_sensitive` | `"bool"` | `false` | `true` | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.mapping.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.mapping.strict` | `"bool"` | `false` | Absent | -- | [REQ-1110](../operations/text.md#req-1110) |
| `expressions.lookup.value` | `"identifier"` | `true` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.dataset` | `"identifier"` | `true` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.key_base` | `["variable", "list[variable]"]` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.key` | `["identifier", "list[identifier]"]` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.between` | `"intermediate_between_class"` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.order_by` | `"list[order_by_term]"` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.keep` | `"str"` | `false` | Absent | `{"values": ["first", "last"]}` | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.lookup.strict` | `"bool"` | `false` | `false` | -- | [REQ-1055](../operations/lookup.md#req-1055) |
| `expressions.cut.source` | `"variable"` | `true` | Absent | -- | [REQ-1118](../operations/computation.md#req-1118) |
| `expressions.cut.breaks` | `"list[float]"` | `true` | Absent | -- | [REQ-1118](../operations/computation.md#req-1118) |
| `expressions.cut.labels` | `"list[str]"` | `true` | Absent | -- | [REQ-1118](../operations/computation.md#req-1118) |
| `expressions.cut.right` | `"bool"` | `false` | `false` | -- | [REQ-1118](../operations/computation.md#req-1118) |
| `expressions.cut.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1118](../operations/computation.md#req-1118) |

## schema_expression_numeric.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.compute.expr` | `"numeric_expression"` | `true` | Absent | -- | [REQ-1119](../operations/computation.md#req-1119) |
| `expressions.round_half_away_from_zero.source` | `"variable"` | `true` | Absent | -- | [REQ-1172](../operations/computation.md#req-1172) |
| `expressions.round_half_away_from_zero.digits` | `"int"` | `true` | Absent | -- | [REQ-1172](../operations/computation.md#req-1172) |
| `numeric_expression` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1120](../operations/computation.md#req-1120) |

## schema_expression_str.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.str_extract.source` | `"variable"` | `true` | Absent | -- | [REQ-1111](../operations/text.md#req-1111) |
| `expressions.str_extract.pattern` | `"regex"` | `true` | Absent | -- | [REQ-1111](../operations/text.md#req-1111) |
| `expressions.str_extract.group` | `"int"` | `false` | `0` | -- | [REQ-1111](../operations/text.md#req-1111) |
| `expressions.str_extract.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1111](../operations/text.md#req-1111) |
| `expressions.str_extract.no_match` | `"literal_value"` | `false` | Absent | -- | [REQ-1111](../operations/text.md#req-1111) |
| `expressions.str_contains.source` | `"variable"` | `true` | Absent | -- | [REQ-1243](../operations/text.md#req-1243) |
| `expressions.str_contains.pattern` | `"regex"` | `true` | Absent | -- | [REQ-1243](../operations/text.md#req-1243) |
| `expressions.str_contains.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1243](../operations/text.md#req-1243) |
| `expressions.str_concat.sources` | `"list[expression]"` | `true` | Absent | -- | [REQ-1112](../operations/text.md#req-1112) |
| `expressions.str_concat.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1112](../operations/text.md#req-1112) |
| `expressions.str_template` | `["string_template", "str_template_class"]` | `false` | Absent | -- | [REQ-1113](../operations/text.md#req-1113) |
| `expressions.str_upper.source` | `"variable"` | `true` | Absent | -- | [REQ-1114](../operations/text.md#req-1114) |
| `expressions.str_upper.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1114](../operations/text.md#req-1114) |
| `expressions.str_lower.source` | `"variable"` | `true` | Absent | -- | [REQ-1115](../operations/text.md#req-1115) |
| `expressions.str_lower.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1115](../operations/text.md#req-1115) |
| `expressions.str_sentence.source` | `"variable"` | `true` | Absent | -- | [REQ-1240](../operations/text.md#req-1240) |
| `expressions.str_sentence.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1240](../operations/text.md#req-1240) |
| `expressions.str_title.source` | `"variable"` | `true` | Absent | -- | [REQ-1241](../operations/text.md#req-1241) |
| `expressions.str_title.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1241](../operations/text.md#req-1241) |
| `string_template` | `"str"` | `false` | Absent | -- | [REQ-1116](../operations/text.md#req-1116) |
| `str_template_class.template` | `"string_template"` | `true` | Absent | -- | [REQ-1117](../operations/text.md#req-1117) |
| `str_template_class.missing` | `"literal_value"` | `false` | Absent | -- | [REQ-1117](../operations/text.md#req-1117) |

## schema_expression_window.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `window_spec.group_by` | `"list[variable]"` | `false` | Absent | -- | [REQ-1122](../operations/windows.md#req-1122) |
| `window_spec.order_by` | `"list[order_by_term]"` | `false` | Absent | -- | [REQ-1122](../operations/windows.md#req-1122) |
| `window_spec.filter` | `"predicate"` | `false` | Absent | -- | [REQ-1122](../operations/windows.md#req-1122) |
| `expressions.row_number.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1123](../operations/windows.md#req-1123) |
| `expressions.rank.method` | `"str"` | `false` | `"competition"` | `{"values": ["competition", "dense"]}` | [REQ-1124](../operations/windows.md#req-1124) |
| `expressions.rank.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1124](../operations/windows.md#req-1124) |
| `expressions.row_value.source` | `"variable"` | `true` | Absent | -- | [REQ-1125](../operations/windows.md#req-1125) |
| `expressions.row_value.offset` | `"int"` | `true` | Absent | -- | [REQ-1125](../operations/windows.md#req-1125) |
| `expressions.row_value.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1125](../operations/windows.md#req-1125) |
| `expressions.previous_non_missing.source` | `"variable"` | `true` | Absent | -- | [REQ-1126](../operations/windows.md#req-1126) |
| `expressions.previous_non_missing.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1126](../operations/windows.md#req-1126) |
| `expressions.locf.source` | `"variable"` | `true` | Absent | -- | [REQ-1239](../operations/windows.md#req-1239) |
| `expressions.locf.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1239](../operations/windows.md#req-1239) |
| `expressions.baseline_flag.date` | `"variable"` | `true` | Absent | -- | [REQ-1127](../operations/windows.md#req-1127) |
| `expressions.baseline_flag.reference_date` | `"variable"` | `true` | Absent | -- | [REQ-1127](../operations/windows.md#req-1127) |
| `expressions.baseline_flag.window` | `"window_spec"` | `false` | Absent | -- | [REQ-1127](../operations/windows.md#req-1127) |

## schema_function.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `expressions.function.name` | `"identifier"` | `true` | Absent | -- | [REQ-1085](../operations/functions.md#req-1085) |
| `expressions.function.contract_version` | `"function_contract_version"` | `true` | Absent | -- | [REQ-1085](../operations/functions.md#req-1085) |
| `expressions.function.args` | `"dict[identifier, function_arg]"` | `false` | `{}` | -- | [REQ-1085](../operations/functions.md#req-1085) |
| `function_arg` | `["variable", "int", "float", "bool", "null", "function_string_literal_class", "function_date_literal_class", "function_datetime_literal_class"]` | `false` | Absent | -- | [REQ-1086](../operations/functions.md#req-1086) |

## schema_metadata.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `submission_dataset_class.label` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.class` | `"dataset_class_name"` | `true` | Absent | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.subclass` | `"dataset_subclass_name"` | `false` | Absent | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.structure` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.repeating` | `"bool"` | `true` | Absent | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.reference_data` | `"bool"` | `false` | `false` | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.domain` | `"identifier"` | `false` | Absent | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_dataset_class.comment` | `"submission_comment"` | `false` | Absent | -- | [REQ-1130](../submission/metadata.md#req-1130) |
| `submission_column_class.core` | `"core_designation"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.mandatory` | `"bool"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.role` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.data_type` | `"define_data_type"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.length` | `"int"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.significant_digits` | `"int"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.display_format` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.codelist` | `"identifier"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.inventory_vocabulary` | `"bool"` | `false` | Absent | -- | [REQ-1156](../submission/terminology.md#req-1156) |
| `submission_column_class.origin` | `"submission_origin_class"` | `true` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.method` | `"submission_method"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_column_class.comment` | `"submission_comment"` | `false` | Absent | -- | [REQ-1131](../submission/metadata.md#req-1131) |
| `submission_origin_class.type` | `"origin_type"` | `true` | Absent | -- | [REQ-1132](../submission/metadata.md#req-1132) |
| `submission_origin_class.source` | `"origin_source"` | `false` | Absent | -- | [REQ-1132](../submission/metadata.md#req-1132) |
| `submission_origin_class.description` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1132](../submission/metadata.md#req-1132) |
| `submission_origin_class.documents` | `"list[document_reference]"` | `false` | Absent | -- | [REQ-1132](../submission/metadata.md#req-1132) |
| `submission_method_class.name` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1133](../submission/metadata.md#req-1133) |
| `submission_method_class.type` | `"method_type"` | `false` | `"Computation"` | -- | [REQ-1133](../submission/metadata.md#req-1133) |
| `submission_method_class.description` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1133](../submission/metadata.md#req-1133) |
| `submission_method_class.expression` | `"formal_expression_class"` | `false` | Absent | -- | [REQ-1133](../submission/metadata.md#req-1133) |
| `submission_method_class.documents` | `"list[document_reference]"` | `false` | Absent | -- | [REQ-1133](../submission/metadata.md#req-1133) |
| `submission_comment_class.text` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1134](../submission/metadata.md#req-1134) |
| `submission_comment_class.documents` | `"list[document_reference]"` | `false` | Absent | -- | [REQ-1134](../submission/metadata.md#req-1134) |
| `formal_expression_class.context` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1135](../submission/metadata.md#req-1135) |
| `formal_expression_class.code` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1135](../submission/metadata.md#req-1135) |
| `document_reference_class.document` | `"identifier"` | `true` | Absent | -- | [REQ-1136](../submission/metadata.md#req-1136) |
| `document_reference_class.pages` | `"page_reference_class"` | `false` | Absent | -- | [REQ-1136](../submission/metadata.md#req-1136) |
| `page_reference_class.type` | `"str"` | `true` | Absent | `{"values": ["PhysicalRef", "NamedDestination"]}` | [REQ-1137](../submission/metadata.md#req-1137) |
| `page_reference_class.refs` | `"str"` | `true` | Absent | `{"min_length": 1}` | [REQ-1137](../submission/metadata.md#req-1137) |
| `page_reference_class.title` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1137](../submission/metadata.md#req-1137) |
| `submission_comment` | `["str", "submission_comment_class"]` | `false` | Absent | -- | [REQ-1138](../submission/metadata.md#req-1138) |
| `submission_method` | `["str", "submission_method_class"]` | `false` | Absent | -- | [REQ-1139](../submission/metadata.md#req-1139) |
| `document_reference` | `["identifier", "document_reference_class"]` | `false` | Absent | -- | [REQ-1140](../submission/metadata.md#req-1140) |
| `core_designation` | `"str"` | `false` | Absent | `{"values": ["Req", "Exp", "Perm"]}` | [REQ-1141](../submission/metadata.md#req-1141) |
| `origin_type` | `"str"` | `false` | Absent | `{"values": ["Assigned", "Collected", "Derived", "Not Available", "Other", "Predecessor", "Protocol"]}` | [REQ-1142](../submission/metadata.md#req-1142) |
| `origin_source` | `"str"` | `false` | Absent | `{"values": ["Investigator", "Sponsor", "Subject", "Vendor"]}` | [REQ-1143](../submission/metadata.md#req-1143) |
| `method_type` | `"str"` | `false` | Absent | `{"values": ["Computation", "Imputation"]}` | [REQ-1144](../submission/metadata.md#req-1144) |
| `define_data_type` | `"str"` | `false` | Absent | `{"values": ["text", "integer", "float", "date", "datetime", "time", "partialDate", "partialTime", "partialDatetime", "incompleteDate", "incompleteTime", "incompleteDatetime", "durationDatetime", "intervalDatetime", "URI"]}` | [REQ-1145](../submission/metadata.md#req-1145) |
| `dataset_class_name` | `"str"` | `false` | Absent | `{"values": ["ADAM OTHER", "BASIC DATA STRUCTURE", "DEVICE LEVEL ANALYSIS DATASET", "EVENTS", "FINDINGS", "FINDINGS ABOUT", "INTERVENTIONS", "MEDICAL DEVICE BASIC DATA STRUCTURE", "MEDICAL DEVICE OCCURRENCE DATA STRUCTURE", "OCCURRENCE DATA STRUCTURE", "REFERENCE DATA STRUCTURE", "RELATIONSHIP", "SPECIAL PURPOSE", "STUDY REFERENCE", "SUBJECT LEVEL ANALYSIS DATASET", "TRIAL DESIGN"]}` | [REQ-1146](../submission/metadata.md#req-1146) |
| `dataset_subclass_name` | `"str"` | `false` | Absent | `{"values": ["ADVERSE EVENT", "MEDICAL DEVICE TIME-TO-EVENT", "NON-COMPARTMENTAL ANALYSIS", "POPULATION PHARMACOKINETIC ANALYSIS", "TIME-TO-EVENT"]}` | [REQ-1147](../submission/metadata.md#req-1147) |

## schema_shared.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `column_type` | `"str"` | `false` | Absent | `{"values": ["str", "int", "float", "date", "datetime"]}` | Schema constraint |
| `literal_value` | `["str", "int", "float", "bool", "null"]` | `false` | Absent | -- | [REQ-1149](../values/types.md#req-1149) |
| `path` | `"str"` | `false` | Absent | `{"min_length": 1}` | Schema constraint |
| `project_path` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1151](../storage/resources.md#req-1151) |
| `identifier` | `"str"` | `false` | Absent | `{"pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}` | [REQ-1046](../specification/structure.md#req-1046) |
| `function_contract_version` | `"str"` | `false` | Absent | `{"min_length": 1}` | [REQ-1087](../operations/functions.md#req-1087) |
| `function_param_type` | `"str"` | `false` | Absent | `{"values": ["str", "int", "float", "bool", "date", "datetime"]}` | Schema constraint |
| `function_string_literal_class.literal` | `"str"` | `true` | Absent | -- | Schema constraint |
| `function_date_literal_class.date` | `"str"` | `true` | Absent | -- | Schema constraint |
| `function_datetime_literal_class.datetime` | `"str"` | `true` | Absent | -- | Schema constraint |
| `function_value` | `["literal_value", "function_date_literal_class", "function_datetime_literal_class"]` | `false` | Absent | -- | Schema constraint |

## schema_verification.yaml

| Field or value type | Type | Required | Default | Constraints | Contract |
| --- | --- | --- | --- | --- | --- |
| `column_verifications.not_missing.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `column_verifications.allowed_values.values` | `"list[literal_value]"` | `true` | Absent | -- | Schema constraint |
| `column_verifications.allowed_values.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `column_verifications.range.min` | `"float"` | `false` | Absent | -- | Schema constraint |
| `column_verifications.range.max` | `"float"` | `false` | Absent | -- | Schema constraint |
| `column_verifications.range.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `column_verifications.max_length.max` | `"int"` | `true` | Absent | -- | Schema constraint |
| `column_verifications.max_length.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `column_verifications.matches.pattern` | `"regex"` | `true` | Absent | -- | Schema constraint |
| `column_verifications.matches.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `dataset_verifications.unique.columns` | `"list[variable]"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.unique.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `dataset_verifications.all_or_none.id` | `"verification_id"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.all_or_none.columns` | `"list[variable]"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.all_or_none.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `dataset_verifications.implies.id` | `"verification_id"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.implies.when` | `"predicate"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.implies.then` | `"predicate"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.implies.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `dataset_verifications.assert.id` | `"verification_id"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.assert.expr` | `"predicate"` | `true` | Absent | -- | Schema constraint |
| `dataset_verifications.assert.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `dataset_verifications.row_count.id` | `"verification_id"` | `false` | Absent | -- | Schema constraint |
| `dataset_verifications.row_count.group_by` | `"list[variable]"` | `false` | Absent | -- | Schema constraint |
| `dataset_verifications.row_count.filter` | `"predicate"` | `false` | Absent | -- | Schema constraint |
| `dataset_verifications.row_count.when` | `"predicate"` | `false` | Absent | -- | [REQ-1154](../execution/verification.md#req-1154) |
| `dataset_verifications.row_count.min` | `"int"` | `false` | Absent | -- | Schema constraint |
| `dataset_verifications.row_count.max` | `"int"` | `false` | Absent | -- | Schema constraint |
| `dataset_verifications.row_count.severity` | `"verification_severity"` | `false` | `"error"` | -- | Schema constraint |
| `verification_id` | `"str"` | `false` | Absent | `{"min_length": 1}` | Schema constraint |
| `verification_severity` | `"str"` | `false` | Absent | `{"values": ["error", "warning"]}` | Schema constraint |
