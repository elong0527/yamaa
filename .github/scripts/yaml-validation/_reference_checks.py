"""Temporary repository-validator dependency boundary for migration #2002.

These bindings still implement reference semantics. They are deliberately kept
in one removable module while the installed shared engine gains the remaining
language families. This module must disappear before reference removal #1994;
moving imports here does not qualify semantic delegation to yamaa.check.
"""

import importlib.util
from pathlib import Path


def regex_binding():
    from yamaa.regex import (
        RegexError,
        capture_group_count,
        compile_pattern,
        full_match,
    )

    return RegexError, capture_group_count, compile_pattern, full_match


def predicate_binding():
    from yamaa.expressions.predicates import (
        COMPARISON_OPERATORS,
        RESERVED_NAMES,
        PredicateError,
        parse_predicate,
    )

    return COMPARISON_OPERATORS, RESERVED_NAMES, PredicateError, parse_predicate


def window_binding():
    from yamaa.schema.windows import expand_named_windows
    from yamaa.specification.diagnostics import SpecificationError
    from yamaa.specification.schema import SchemaBundle

    return expand_named_windows, SpecificationError, SchemaBundle


def parquet_binding():
    from yamaa.io.parquet import ParquetProfileFailure, parquet_source_fields

    return parquet_source_fields, ParquetProfileFailure


def odm_binding():
    from yamaa.odm.items import (
        ODM_SCHEMA_FIELDS,
        bind_fields,
        iter_odm_payloads,
        parse_odm_read,
    )

    return ODM_SCHEMA_FIELDS, bind_fields, iter_odm_payloads, parse_odm_read


def csv_binding():
    from yamaa.io.csv import (
        CsvProfileFailure,
        fixed_point,
        render_records,
        scan_records,
    )
    from yamaa.models import DateTimeValue, DateValue, convert_value

    return (
        CsvProfileFailure,
        fixed_point,
        render_records,
        scan_records,
        DateTimeValue,
        DateValue,
        convert_value,
    )


def source_profile_binding():
    # Keep the existing standalone loader: CSV fixture checks do not require
    # loading the host package and its runtime dependencies. Its reference
    # file authority is now explicit here rather than hidden in the validator.
    path = Path(__file__).resolve().parents[3] / 'python/src/yamaa/io/csv.py'
    spec = importlib.util.spec_from_file_location('yamaa_io_csv', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
