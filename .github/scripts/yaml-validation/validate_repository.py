"""Repository validation command and compatibility exports."""
import argparse
import sys
from editorial import *  # noqa: F403: repository-tool compatibility exports
from pathlib import Path
import yaml
import _native_specification
from _repository_yaml import UniqueKeyLoader
from grammar_checks import validate_grammar_contracts
from regex_checks import RegexBindingUnavailable, require_regex_binding, validate_regex_conformance
from repository_artifacts import validate_csv_shapes, validate_examples_csv, validate_key_column_order
from repository_manifests import load_condition_registry, load_validation_manifest, validate_condition_registry, validate_expected_error_contracts, validate_validation_manifest
from repository_schema import build_schema_env
from repository_structure import validate_examples_define_documents, validate_examples_environment_documents, validate_examples_layout, validate_examples_structure
import regex_checks
from repository_schema import build_schema_env as build_schema_env, check_descriptor as check_descriptor, iter_function_calls as iter_function_calls, native_diagnostics as native_diagnostics, normalize_single_type_value as normalize_single_type_value, prepare_spec_document as prepare_spec_document, project_environment_paths as project_environment_paths, resolve_spec_inheritance as resolve_spec_inheritance, schema_class_fields as schema_class_fields, valid_temporal_literal as valid_temporal_literal, validate_schemas as validate_schemas, validate_spec_document as validate_spec_document, validate_spec_functions as validate_spec_functions, validate_spec_functions_against as validate_spec_functions_against, validate_type as validate_type
from repository_artifacts import ARTIFACT_PROFILES as ARTIFACT_PROFILES, BOM_UTF8 as BOM_UTF8, CANONICAL_DATE as CANONICAL_DATE, CANONICAL_DATETIME as CANONICAL_DATETIME, CANONICAL_INT as CANONICAL_INT, HISTORICAL_RESOLVED_FIXTURES as HISTORICAL_RESOLVED_FIXTURES, KEY_COLUMN_ORDER as KEY_COLUMN_ORDER, SOURCE_PROFILE_CONDITIONS as SOURCE_PROFILE_CONDITIONS, SOURCE_READ_CONDITIONS as SOURCE_READ_CONDITIONS, SourceProfileError as SourceProfileError, VERIFICATION_LOG_TYPES as VERIFICATION_LOG_TYPES, WARNING_LOG_TYPES as WARNING_LOG_TYPES, _ensure_csv_binding as _ensure_csv_binding, _key_column_order_errors as _key_column_order_errors, artifact_profile as artifact_profile, canonical_float_text as canonical_float_text, canonical_temporal_text as canonical_temporal_text, check_source_file as check_source_file, declared_source_condition as declared_source_condition, example_artifact_owners as example_artifact_owners, example_entry_specs as example_entry_specs, expected_resolved_path as expected_resolved_path, parse_source_profile as parse_source_profile, render_records as render_records, scan_records as scan_records, source_coordinates as source_coordinates, validate_csv_artifact as validate_csv_artifact, validate_csv_shapes as validate_csv_shapes, validate_examples_csv as validate_examples_csv, validate_expected_resolved_fixture as validate_expected_resolved_fixture, validate_key_column_order as validate_key_column_order
from repository_manifests import EXPECTED_ERROR_PHASES as EXPECTED_ERROR_PHASES, VALIDATION_CONDITION_REGISTRY as VALIDATION_CONDITION_REGISTRY, check_declared_validation_error as check_declared_validation_error, diagnostic_matches_path as diagnostic_matches_path, load_condition_registry as load_condition_registry, load_validation_manifest as load_validation_manifest, spec_path_exists as spec_path_exists, validate_condition_registry as validate_condition_registry, validate_expected_error_contracts as validate_expected_error_contracts, validate_registered_fixture_diagnostics as validate_registered_fixture_diagnostics, validate_validation_manifest as validate_validation_manifest, validation_phase_contracts as validation_phase_contracts
from grammar_checks import AGGREGATE_REDUCERS as AGGREGATE_REDUCERS, AggregateExpressionError as AggregateExpressionError, GRAMMAR_CASE_KEYS as GRAMMAR_CASE_KEYS, GRAMMAR_CONDITIONS as GRAMMAR_CONDITIONS, GRAMMAR_CONTRACTS as GRAMMAR_CONTRACTS, GRAMMAR_COVERS as GRAMMAR_COVERS, GRAMMAR_DECISIONS as GRAMMAR_DECISIONS, GRAMMAR_DIR as GRAMMAR_DIR, GRAMMAR_DOCUMENT_KEYS as GRAMMAR_DOCUMENT_KEYS, GRAMMAR_PRODUCTION_KEYS as GRAMMAR_PRODUCTION_KEYS, GRAMMAR_REQUIRED_KEYS as GRAMMAR_REQUIRED_KEYS, NUMERIC_FUNCTION_ARITIES as NUMERIC_FUNCTION_ARITIES, NumericExpressionError as NumericExpressionError, PREDICATE_COMPARISON_OPERATORS as PREDICATE_COMPARISON_OPERATORS, PREDICATE_RESERVED_NAMES as PREDICATE_RESERVED_NAMES, PROHIBITED_NUMERIC_KEYWORDS as PROHIBITED_NUMERIC_KEYWORDS, PredicateError as PredicateError, PredicateSemanticIssue as PredicateSemanticIssue, StringTemplateError as StringTemplateError, _ensure_predicate_binding as _ensure_predicate_binding, _expression_fixture as _expression_fixture, aggregate_expression_identifier_names as aggregate_expression_identifier_names, decide_aggregate as decide_aggregate, decide_numeric as decide_numeric, decide_predicate as decide_predicate, decide_string_template as decide_string_template, expand_spec_windows as expand_spec_windows, expression_shape as expression_shape, grammar_case_errors as grammar_case_errors, grammar_defined_symbols as grammar_defined_symbols, grammar_symbol_references as grammar_symbol_references, grammar_vocabulary_errors as grammar_vocabulary_errors, numeric_expression_identifier_names as numeric_expression_identifier_names, parse_aggregate_expression as parse_aggregate_expression, parse_numeric_expression as parse_numeric_expression, parse_predicate as parse_predicate, parse_string_template as parse_string_template, predicate_identifier_names as predicate_identifier_names, predicate_operand_shape as predicate_operand_shape, predicate_shape as predicate_shape, quote_grammar_scalar as quote_grammar_scalar, render_grammar_block as render_grammar_block, rule_grammar_block as rule_grammar_block, string_template_identifier_names as string_template_identifier_names, string_template_placeholders as string_template_placeholders, string_template_shape as string_template_shape, validate_grammar_contract as validate_grammar_contract, validate_grammar_contracts as validate_grammar_contracts
from regex_checks import InvalidRegex as InvalidRegex, REGEX_CONFORMANCE_PATH as REGEX_CONFORMANCE_PATH, REGEX_CONTRACT as REGEX_CONTRACT, REGEX_CONTRACT_VERSION as REGEX_CONTRACT_VERSION, REGEX_FIXTURE_CASE_KEYS as REGEX_FIXTURE_CASE_KEYS, REGEX_FIXTURE_COVERS as REGEX_FIXTURE_COVERS, REGEX_FIXTURE_REQUIRED_COVERS as REGEX_FIXTURE_REQUIRED_COVERS, REGEX_NO_MATCH as REGEX_NO_MATCH, RegexBindingUnavailable as RegexBindingUnavailable, RegexGroupOutOfRange as RegexGroupOutOfRange, _PortableRegexError as _PortableRegexError, _portable_binding as _portable_binding, _portable_compile as _portable_compile, _portable_full_match as _portable_full_match, _portable_group_count as _portable_group_count, _regex_fixture_case_errors as _regex_fixture_case_errors, _regex_fixture_extract_errors as _regex_fixture_extract_errors, anchored_regex_source as anchored_regex_source, compile_regex as compile_regex, regex_capture_group_count as regex_capture_group_count, regex_contract_requirement as regex_contract_requirement, regex_extract as regex_extract, regex_full_match as regex_full_match, regex_group_errors as regex_group_errors, regex_pattern_errors as regex_pattern_errors, regex_search as regex_search, require_regex_binding as require_regex_binding, validate_regex_conformance as validate_regex_conformance
from validation_diagnostics import CSV_PROFILE as CSV_PROFILE, CsvProfileFailure as CsvProfileFailure, RESOURCE_PATH_MESSAGES as RESOURCE_PATH_MESSAGES, VALIDATION_CONTEXT_FIELDS as VALIDATION_CONTEXT_FIELDS, ValidationDiagnostic as ValidationDiagnostic, validation_diagnostic as validation_diagnostic
from _repository_yaml import construct_yaml_12_float as construct_yaml_12_float, construct_yaml_12_int as construct_yaml_12_int, normalize_non_finite_float as normalize_non_finite_float
from repository_structure import HISTORICAL_RESOURCE_FIXTURES as HISTORICAL_RESOURCE_FIXTURES, validate_examples_define_documents as validate_examples_define_documents, validate_examples_environment_documents as validate_examples_environment_documents, validate_examples_layout as validate_examples_layout, validate_examples_structure as validate_examples_structure
from editorial import validate_column_labels as validate_column_labels, validate_producer_output_contract as validate_producer_output_contract
import _native_csv
import _native_schema
import _native_specification
import _typed_fixtures


def check_yaml_files(root: Path):
    errors = []
    warnings = []
    errors.extend(validate_ascii_sources(root))
    for yaml_file in sorted(root.rglob('*.yaml')):
        if '.github' in yaml_file.parts:
            continue
        try:
            with open(yaml_file, 'r', encoding='utf-8') as f:
                document = yaml.load(f, Loader=UniqueKeyLoader)
            errors.extend(
                validate_unicode_scalars(
                    document,
                    str(yaml_file.relative_to(root)),
                )
            )
        except Exception as e:
            msg = str(e)
            if hasattr(e, 'problem_mark') and e.problem_mark:
                msg += f" at line {e.problem_mark.line + 1}"
            errors.append(f"ERROR: {yaml_file.relative_to(root)}: {msg}")

    # Also validate schemas
    env, schema_errors = build_schema_env(root)
    errors.extend(schema_errors)
    environment_schema = None
    if (root / 'yaml' / 'schema_environment.yaml').exists():
        environment_schema, environment_schema_errors = build_schema_env(
            root, 'schema_environment.yaml'
        )
        errors.extend(environment_schema_errors)
        if environment_schema_errors:
            environment_schema = None
    validation_manifest, manifest_load_errors = load_validation_manifest(root)
    errors.extend(manifest_load_errors)
    if validation_manifest is not None:
        errors.extend(
            validate_validation_manifest(root, validation_manifest)
        )
    condition_registry, condition_registry_load_errors = (
        load_condition_registry(root)
    )
    errors.extend(condition_registry_load_errors)
    if condition_registry is not None:
        errors.extend(
            validate_condition_registry(root, condition_registry)
        )
    errors.extend(
        validate_examples_structure(
            root, env, warnings, validation_manifest
        )
    )
    errors.extend(validate_examples_layout(root))
    if environment_schema is not None:
        errors.extend(validate_examples_environment_documents(root, environment_schema))
    errors.extend(validate_examples_define_documents(root))
    errors.extend(validate_expected_error_contracts(root))
    errors.extend(validate_csv_shapes(root))
    errors.extend(validate_key_column_order(root))
    errors.extend(validate_grammar_contracts(root))
    errors.extend(validate_regex_conformance(root))

    csv_errors, csv_warnings = validate_examples_csv(root, env)
    errors.extend(csv_errors)
    warnings.extend(csv_warnings)
    return errors, warnings


def main():
    parser = argparse.ArgumentParser(description="Validate yamaa repository structure and specs.")
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[3],
                        help="Repository root directory")
    parser.add_argument('--warnings-as-errors', action='store_true',
                        help="Treat warnings as errors")
    args = parser.parse_args()

    try:
        require_regex_binding()
    except RegexBindingUnavailable as exc:
        print(f"ERROR: {exc}")
        return 1

    errors, warnings = check_yaml_files(args.root)

    for warning in warnings:
        print(warning)

    if errors:
        for e in errors:
            print(e)
        return 1

    if warnings and args.warnings_as_errors:
        return 1

    if _native_specification.DEFERRED_ENVIRONMENTS:
        for environment in sorted(_native_specification.DEFERRED_ENVIRONMENTS):
            print(f"DEFERRED: R semantic source/binding/runtime coverage for {environment}; required installed R-host CI owns this lane.")
        print("PASS: Available-host repository checks are clean; R semantic coverage is deferred.")
    else:
        print("PASS: Repository looks clean.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
