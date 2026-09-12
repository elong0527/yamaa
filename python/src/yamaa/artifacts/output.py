"""Turn a completed table into the ordered artifact a profile can write.

R005 owns which columns the artifact has, which rows it holds, and the
order they leave in; this module applies those decisions to a completed
table and refuses a declaration or a stored value that no profile can
carry, so a rendered artifact is writable by construction.
"""

from __future__ import annotations

import datetime as dt
import math
from collections.abc import Callable, Sequence
from pathlib import PurePosixPath
from typing import Annotated, Any, Literal, TypeAlias

import polars as pl
from pydantic import BaseModel, ConfigDict, InstanceOf, JsonValue

from yamaa.artifacts.diagnostics import (
    REPORTED_KEYS,
    ArtifactDiagnostic,
    ArtifactError,
)
from yamaa.io.polars import column_dtype
from yamaa.models import TypedColumn, TypedTable
from yamaa.specification.models import Output

ArtifactProfile: TypeAlias = Literal["csv", "parquet"]

# R020-2 fixes a closed extension mapping, matched without regard to case.
# An extension outside it names no profile and never falls back to one.
_PROFILES: dict[str, ArtifactProfile] = {".csv": "csv", ".parquet": "parquet"}

# R020-23 stores a date as days and a datetime as microseconds from the
# epoch. R016's calendar bounds the values either may carry.
_EPOCH = dt.date(1970, 1, 1).toordinal()
_MIN_DAY = dt.date(dt.MINYEAR, 1, 1).toordinal() - _EPOCH
_MAX_DAY = dt.date(dt.MAXYEAR, 12, 31).toordinal() - _EPOCH
_MIN_MICROSECOND = _MIN_DAY * 86_400 * 1_000_000
_MAX_MICROSECOND = (_MAX_DAY * 86_400 + 86_399) * 1_000_000


class Artifact(BaseModel):
    """One completed, ordered, output-column table and how it is written."""

    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )

    profile: ArtifactProfile
    decimals: int | None
    keys: tuple[str, ...]
    columns: tuple[TypedColumn, ...]
    frame: Annotated[pl.DataFrame, InstanceOf[pl.DataFrame]]


def _diagnostic(
    phase: Literal["validation", "output"],
    condition: str,
    spec_path: str,
    requirement: str,
    context: dict[str, JsonValue],
) -> ArtifactDiagnostic:
    return ArtifactDiagnostic(
        phase=phase,
        condition=condition,
        spec_paths=(spec_path,),
        requirement=requirement,
        context=context,
    )


def profile_of(path: str) -> ArtifactProfile | None:
    """Return the profile a written artifact path selects, if any."""
    return _PROFILES.get(PurePosixPath(path).suffix.lower())


def artifact_profile(path: str) -> ArtifactProfile:
    """Select the R020 profile the artifact path names, or fail."""
    profile = profile_of(path)
    if profile is None:
        raise ArtifactError(
            [
                _diagnostic(
                    "validation",
                    "unknown_artifact_profile",
                    "output.path",
                    "R020-43",
                    {"path": path, "permitted": sorted(_PROFILES)},
                )
            ]
        )
    return profile


def _declaration_diagnostics(
    table: TypedTable, output: Output, keys: Sequence[str]
) -> list[ArtifactDiagnostic]:
    diagnostics: list[ArtifactDiagnostic] = []
    declared = {column.name for column in table.columns}
    profile = profile_of(output.path)
    if profile is None:
        diagnostics.append(
            _diagnostic(
                "validation",
                "unknown_artifact_profile",
                "output.path",
                "R020-43",
                {"path": output.path, "permitted": sorted(_PROFILES)},
            )
        )
    if output.decimals is not None:
        if output.decimals < 0:
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "invalid_field_type",
                    "output.decimals",
                    "R020-44",
                    {"expected": "a non-negative integer", "actual": output.decimals},
                )
            )
        elif profile == "parquet":
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "decimals_not_applicable",
                    "output.decimals",
                    "R020-45",
                    {"path": output.path, "profile": profile},
                )
            )

    seen: set[str] = set()
    for position, name in enumerate(output.columns):
        path = f"output.columns[{position}]"
        if name in seen:
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "duplicate_identifier",
                    path,
                    "R005-46",
                    {"column": name},
                )
            )
        elif name not in declared:
            diagnostics.append(
                _diagnostic(
                    "validation", "undeclared_column", path, "R005-46", {"column": name}
                )
            )
        seen.add(name)

    for position, name in enumerate(keys):
        if name in declared and name not in seen:
            # R005-16: a key identifies rows in the artifact, so an internal
            # column cannot be one even though it is derived like any other.
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "internal_column_in_keys",
                    f"keys[{position}]",
                    "R005-45",
                    {"column": name},
                )
            )

    ordered: set[str] = set()
    for position, term in enumerate(output.order_by or ()):
        path = f"output.order_by[{position}]"
        if term.variable in ordered:
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "duplicate_order_term",
                    path,
                    "R005-49",
                    {"column": term.variable},
                )
            )
        elif term.variable not in declared:
            # R005-35 admits an internal column here, so membership is the
            # declared set rather than the artifact's own columns.
            diagnostics.append(
                _diagnostic(
                    "validation",
                    "undeclared_column",
                    path,
                    "R005-48",
                    {"column": term.variable},
                )
            )
        ordered.add(term.variable)
    return diagnostics


def _unwritable(
    table: TypedTable, names: Sequence[str], keys: Sequence[str]
) -> list[ArtifactDiagnostic]:
    """Report stored values no profile may carry (R020-46).

    A `str` column holds well-formed text and an `int` column is stored
    `Int64`, so each carries the signed 64-bit bound its host column owns.
    The other three host columns are wider than the values R011 and R016
    admit: a `float` column holds a non-finite value, and a temporal column
    a date outside the calendar or a `datetime` finer than the whole second
    R020-25 writes. Each is read here rather than repaired on the way out.

    A temporal column is read as the count its profile stores rather than
    as a host date, because the values under test are exactly the ones no
    host date represents.
    """
    diagnostics: list[ArtifactDiagnostic] = []
    schema = table.frame.schema
    key_names = [name for name in keys if name in schema]
    declared = {column.name: column.type for column in table.columns}
    for name in names:
        if name not in declared:
            continue
        expected = column_dtype(declared[name])
        if schema[name] != expected:
            diagnostics.append(
                _diagnostic(
                    "output",
                    "unwritable_value",
                    f"columns.{name}.type",
                    "R020-46",
                    {
                        "column": name,
                        "type": declared[name],
                        "stored": str(schema[name]),
                    },
                )
            )
            continue
        if declared[name] == "float":
            offending = _offending_rows(
                table, name, key_names, lambda value: not math.isfinite(value)
            )
        elif declared[name] == "date":
            offending = _offending_rows(
                table,
                name,
                key_names,
                lambda day: not _MIN_DAY <= day <= _MAX_DAY,
                stored=pl.Int32,
            )
        elif declared[name] == "datetime":
            offending = _offending_rows(
                table,
                name,
                key_names,
                lambda micros: (
                    micros % 1_000_000 != 0
                    or not _MIN_MICROSECOND <= micros <= _MAX_MICROSECOND
                ),
                stored=pl.Int64,
            )
        else:
            continue
        if offending:
            diagnostics.append(
                _diagnostic(
                    "output",
                    "unwritable_value",
                    f"columns.{name}",
                    "R020-46",
                    {
                        "column": name,
                        "type": declared[name],
                        "failure_count": len(offending),
                        "keys": [row for row, _ in offending[:REPORTED_KEYS]],
                        "value": offending[0][1],
                    },
                )
            )
    return diagnostics


def _offending_rows(
    table: TypedTable,
    name: str,
    keys: Sequence[str],
    unwritable: Callable[[Any], bool],
    *,
    stored: pl.DataType | None = None,
) -> list[tuple[dict[str, JsonValue], JsonValue]]:
    column = table.frame.get_column(name)
    values = (column if stored is None else column.cast(stored)).to_list()
    positions = [
        index
        for index, value in enumerate(values)
        if value is not None and unwritable(value)
    ]
    if not positions:
        return []
    key_values = {key: _key_values(table, key) for key in keys}
    return [
        ({key: key_values[key][index] for key in keys}, _json(values[index]))
        for index in positions
    ]


def _key_values(table: TypedTable, name: str) -> list[JsonValue]:
    column = table.frame.get_column(name)
    try:
        return [_json(value) for value in column.to_list()]
    except ValueError:
        # A stored temporal value outside the calendar has no host scalar,
        # and a row this failure names is still identified by its text.
        return list(column.cast(pl.String).to_list())


def _json(value: object) -> JsonValue:
    if isinstance(value, float) and not math.isfinite(value):
        # A diagnostic is read by another runtime, and no portable document
        # spells a non-finite number, so the value is reported as its text.
        return str(value)
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def _ordered_frame(frame: pl.DataFrame, output: Output) -> pl.DataFrame:
    terms = output.order_by or ()
    if not terms:
        # R005-33: an artifact whose specification declares no order keeps
        # the construction order R001 produced.
        return frame
    return frame.sort(
        by=[term.variable for term in terms],
        descending=[term.direction == "desc" for term in terms],
        # R007-15: `nulls` states where missing values sit outright and does
        # not flip with `direction`.
        nulls_last=[term.nulls == "last" for term in terms],
        maintain_order=True,
    )


def build_artifact(table: TypedTable, output: Output, keys: Sequence[str]) -> Artifact:
    """Order a completed table and select the artifact's declared columns."""
    diagnostics = _declaration_diagnostics(table, output, keys)
    if diagnostics:
        raise ArtifactError(diagnostics)
    diagnostics = _unwritable(table, output.columns, keys)
    if diagnostics:
        raise ArtifactError(diagnostics)

    frame = _ordered_frame(table.frame, output).select(output.columns)
    declared = {column.name: column for column in table.columns}
    profile = profile_of(output.path)
    assert profile is not None
    return Artifact(
        profile=profile,
        decimals=output.decimals,
        keys=tuple(keys),
        columns=tuple(declared[name] for name in output.columns),
        frame=frame,
    )
