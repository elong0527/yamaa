"""Diagnostic presentation and normative registry metadata."""
import _native_csv


class ValidationDiagnostic(str):
    """Rendered validation error with stable machine-readable identity."""

    def __new__(
        cls, path, condition, message, *, context=None, span=None,
        rendered=None,
    ):
        location = (
            f" at characters [{span[0]}, {span[1]})"
            if span is not None
            else ""
        )
        if rendered is None:
            rendered = f"ERROR: {path}: {message}{location} [{condition}]"
        value = super().__new__(cls, rendered)
        value.path = path
        value.condition = condition
        value.message = message
        value.context = dict(context or {})
        value.span = tuple(span) if span is not None else None
        return value


def validation_diagnostic(
    path, condition, message, *, context=None, span=None
):
    return ValidationDiagnostic(
        path,
        condition,
        message,
        context=context,
        span=span,
    )


VALIDATION_CONTEXT_FIELDS = {
    ('R001', 'dependency_cycle'): {'cycle'},
    ('R001', 'forward_reference'): {'column', 'dependency'},
    ('R001', 'key_dependency'): {'column', 'dependency'},
    ('R002', 'duplicate_identifier'): {'identifier'},
    ('R002', 'invalid_odm_context'): {'dataset'},
    ('R002', 'odm_schema_field_ambiguous'): {'dataset', 'field'},
    ('R002', 'odm_schema_field_missing'): {'dataset', 'fields'},
    ('R002', 'odm_schema_field_type'): {'dataset'},
    ('R002', 'unknown_field'): {'identifier'},
    ('R003', 'duplicate_identifier'): {'identifier'},
    ('R003', 'prohibited_construct'): {'identifier'},
    ('R003', 'unknown_field'): {'identifier'},
    ('R004', 'invalid_predicate'): {'predicate'},
    ('R004', 'incompatible_input_type'): {'left_type', 'right_type'},
    ('R004', 'unknown_field'): {'identifier'},
    ('R005', 'duplicate_derivation'): {'column', 'rows'},
    ('R005', 'duplicate_order_term'): {'column'},
    ('R005', 'internal_column_in_keys'): {'column'},
    ('R005', 'undeclared_column'): {'column'},
    ('R006', 'invalid_field_type'): {'actual', 'expected'},
    ('R019', 'ambiguous_dictionary'): {'entries', 'folded_key'},
    ('R007', 'incompatible_input_type'): {
        'actual', 'expected', 'source',
    },
    ('R007', 'invalid_cut'): {'reason'},
    ('R007', 'incomparable_sources'): {'sources', 'types'},
    ('R007', 'zero_offset'): {'offset'},
    ('R007', 'unknown_window'): {'window'},
    ('R007', 'window_order_by_required'): {'operation'},
    ('R007', 'window_order_by_forbidden'): {'operation'},
    ('R007', 'missing_value_required'): {'false_value'},
    ('R010', 'incompatible_input_type'): {
        'actual', 'expected', 'expr', 'source',
    },
    ('R010', 'invalid_numeric_expression'): {'expr'},
    ('R010', 'prohibited_construct'): {'construct', 'expr'},
    ('R010', 'prohibited_function'): {'expr', 'function'},
    ('R010', 'qualified_identifier'): {'expr', 'identifier'},
    ('R010', 'unknown_field'): {'expr', 'identifier'},
    ('R011', 'value_not_permitted'): {'permitted', 'value'},
    ('R012', 'invalid_string_template'): {'placeholder', 'reason'},
    ('R013', 'aggregate_identifier_not_grouped'): {
        'dataset', 'identifier',
    },
    ('R013', 'invalid_aggregate_context'): {'reason'},
    ('R013', 'invalid_aggregate_expression'): {'expr'},
    ('R013', 'invalid_derive_step'): {'reason'},
    ('R013', 'mixed_relations'): {'relations'},
    ('R013', 'nested_reduction'): {'expr', 'inner', 'outer'},
    ('R013', 'prohibited_construct'): {'construct', 'expr'},
    ('R013', 'prohibited_function'): {'expr', 'function'},
    ('R013', 'unknown_derive_variable'): {'identifier'},
    ('R013', 'unknown_field'): {'identifier'},
    ('R013', 'incompatible_input_type'): {
        'actual', 'expected', 'source',
    },
    ('R014', 'unknown_field'): {'dataset', 'field'},
    ('R014', 'redundant_field_type'): {'dataset', 'field', 'type'},
    ('R003', 'duplicate_identifier'): {'identifier'},
    ('R003', 'incomparable_range_types'): {
        'lower_type', 'intermediate', 'upper_type', 'value_type',
    },
    ('R003', 'no_applicable_keys'): {'dataset', 'hint', 'keys'},
    ('R003', 'unpaired_fields'): {
        'declared', 'intermediate', 'missing',
    },
    ('R003', 'rename_only_intermediate'): {
        'intermediate', 'dataset',
    },
    ('R006', 'missing_required_field'): {'class', 'field'},
    ('R006', 'value_not_permitted'): {'permitted', 'value'},
    ('R016', 'month_out_of_range'): {'month'},
    ('R016', 'month_not_permitted'): {'month'},
    ('R016', 'month_required'): {'minimum_source_precision'},
    ('R016', 'day_out_of_range'): {'day'},
    ('R016', 'incompatible_input_type'): {'actual', 'expected', 'source'},
    ('R016', 'value_not_permitted'): {'permitted', 'value'},
    ('R017', 'inheritance_cycle'): {'reason'},
    ('R017', 'invalid_clear'): {'field'},
    ('R017', 'invalid_parent_path'): {'reason'},
    ('R017', 'schema_version_mismatch'): {
        'entry_version', 'parent_version',
    },
    ('R018', 'invalid_function_argument'): {
        'argument', 'function', 'cause',
    },
    ('R020', 'unknown_artifact_profile'): {'path', 'permitted'},
    ('R021', 'resource_path_missing'): {'path'},
    ('R021', 'resource_path_not_regular_file'): {'path'},
    ('R021', 'resource_path_not_relative'): {'path'},
    ('R021', 'resource_path_outside_project'): {'path'},
    ('R021', 'resource_path_symlink'): {'path'},
    ('R021', 'resource_path_uri_scheme'): {'path'},
    # The binding's wording is additional context an implementation may
    # report; the pattern is the portable fact a fixture must state.
    ('R022', 'invalid_regex'): {'pattern'},
    ('R022', 'regex_group_out_of_range'): {
        'group', 'group_count', 'pattern',
    },
    ('R023', 'source_profile_unknown'): {'path'},
    ('R014', 'ordinal_field_collision'): {'dataset', 'field'},
}


RESOURCE_PATH_MESSAGES = {
    'resource_path_uri_scheme': 'declares a URI scheme',
    'resource_path_not_normalized': 'is not normalized',
    'resource_path_symlink': 'passes through a symbolic link',
    'resource_path_missing': 'does not exist',
    'resource_path_not_regular_file': 'is not a regular file',
    'resource_path_content_changed': 'changed after it was validated',
}


CsvProfileFailure = None


CSV_PROFILE = _native_csv
