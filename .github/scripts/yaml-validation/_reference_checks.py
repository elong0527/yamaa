"""Temporary repository-validator dependency boundary for migration #2002.

These bindings still implement reference semantics. They are deliberately kept
in one removable module while the installed shared engine gains the remaining
language families. This module must disappear before reference removal #1994;
moving imports here does not qualify semantic delegation to yamaa.check.
"""


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
