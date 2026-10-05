"""Explicit optional normalized-specification bridge; Python remains the default.

Rust owns all admitted derivation, conversion, grouping and table checks. The
existing Python loader/planner/source and artifact adapters are temporary ports;
Project execution explicitly activates verified bindings before source IO. This
interface does not publish files or run workflows.
"""

from __future__ import annotations

import io
import json
from dataclasses import dataclass
from pathlib import Path

import polars as pl
import pyarrow as pa
import pyarrow.compute as pc

from yamaa.adapters._native_dataset_plan import (
    NUMBERING,
    OPERATIONS,
    WINDOW_VALUES,
    WINDOWS,
    admit,
    lower,
    primary_source,
)
from yamaa.adapters._native_dataset_report import condition, observations
from yamaa.adapters._native_dependencies import (
    bind_column_dependency_analyzer,
    bind_dependency_analyzer,
)
from yamaa.adapters._native_project_functions import (
    NATIVE_ACTIVATION_CACHE,
    NativeActivationCache,
    activate_project,
)
from yamaa.adapters._native_references import bind_reference_compiler
from yamaa.functions.artifact import ArtifactResolver
from yamaa.functions.errors import FunctionActivationError
from yamaa.io import (
    ArtifactError,
    LoadedDataset,
    ProducerSchemaUnresolved,
    SourceError,
    build_artifact,
)
from yamaa.io.polars import column_dtype
from yamaa.models import TypedColumn, TypedTable
from yamaa.planning import (
    ExecutionDiagnostic,
    ExecutionPlanningError,
    UnsupportedFeature,
    UnsupportedPlanningError,
    plan_execution,
)
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionResult,
    ExecutionSuccess,
    ExecutionUnsupported,
    HandlerCount,
    SourceProvider,
)
from yamaa.specification.models import Specification
from yamaa.verification import (
    VerificationRecord,
    build_verification_log,
    build_warning_log,
)


@dataclass(frozen=True)
class NativeDatasetRun:
    """A native result plus every completed dataset check, including held checks."""

    result: ExecutionResult
    verifications: tuple[VerificationRecord, ...] = ()


class NativeDatasetLimitError(RuntimeError):
    """A native resource policy stopped the run; no accepted output is available."""

    def __init__(self, outcome, handler_counts=()):
        """Retain exact resource counters without claiming a language condition."""
        self.handler_counts = handler_counts
        self.resource = outcome["resource"]
        self.limit = outcome["limit"]
        self.required = outcome["required"]
        super().__init__(f"native dataset resource limit: {self.resource}")


def _source_ipc(table):
    """Encode the host storage boundary once; ingestion already fixed temporal precision."""
    source = table.frame.to_arrow()
    fields = []
    arrays = []
    for column, array in zip(table.columns, source.columns, strict=True):
        if column.type in {"date", "datetime"}:
            date = column.type == "date"
            dtype = pa.date32() if date else pa.timestamp("s")
            compatible = (
                pa.types.is_date32(array.type)
                if date
                else pa.types.is_timestamp(array.type) and array.type.tz is None
            )
            if not compatible:
                raise ValueError("incompatible temporal source storage")
            children = [
                pa.field("value", dtype, nullable=False),
                pa.field("precision", pa.uint8(), nullable=False),
            ]
            chunks = []
            for chunk in array.chunks:
                value = pc.cast(
                    pc.if_else(chunk.is_null(), pa.scalar(0, type=chunk.type), chunk),
                    dtype,
                    safe=True,
                )
                precision = pa.array([2 if date else 1] * len(chunk), type=pa.uint8())
                chunks.append(
                    pa.StructArray.from_arrays(
                        [value, precision], fields=children, mask=chunk.is_null()
                    )
                )
            dtype = pa.struct(children)
            converted = pa.chunked_array(chunks, type=dtype)
        else:
            dtype = {"str": pa.string(), "int": pa.int64(), "float": pa.float64()}[
                column.type
            ]
            compatible = (
                (pa.types.is_string(array.type) or pa.types.is_large_string(array.type))
                if column.type == "str"
                else array.type == dtype
            )
            if not compatible:
                raise ValueError("incompatible source storage")
            converted = pc.cast(array, dtype, safe=True)
        fields.append(pa.field(column.name, dtype, nullable=True))
        arrays.append(converted)
    schema = pa.schema(fields)
    output = io.BytesIO()
    with pa.ipc.new_stream(output, schema) as writer:
        writer.write_table(pa.Table.from_arrays(arrays, schema=schema))
    return output.getvalue()


def _output_table(specification, data):
    """Materialize accepted native values, dropping precision only at host storage."""
    native = pa.ipc.open_stream(data).read_all()
    columns = tuple(
        TypedColumn(name=column.name, type=column.type)
        for column in specification.columns
    )
    fields = []
    for column in columns:
        dtype = {"str": pa.string(), "int": pa.int64(), "float": pa.float64()}.get(
            column.type
        )
        if dtype is None:
            dtype = pa.struct(
                [
                    pa.field(
                        "value",
                        pa.date32() if column.type == "date" else pa.timestamp("s"),
                        nullable=False,
                    ),
                    pa.field("precision", pa.uint8(), nullable=False),
                ]
            )
        fields.append(pa.field(column.name, dtype, nullable=True))
    if native.schema != pa.schema(fields):
        raise ValueError("native output schema does not match the admitted plan")
    series = []
    for column, array in zip(columns, native.columns, strict=True):
        if column.type in {"date", "datetime"}:
            # Flattening a nullable struct must propagate its parent nulls.
            array = pc.struct_field(array, "value")
        series.append(
            pl.from_arrow(array)
            .rename(column.name)
            .cast(column_dtype(column.type), strict=True)
        )
    return TypedTable(columns=columns, frame=pl.DataFrame(series))


def _failure(diagnostics):
    """Retain portable validation/ingestion/output errors without manufacturing handlers."""
    return NativeDatasetRun(
        ExecutionFailure(
            diagnostics=tuple(
                ExecutionDiagnostic.model_validate(item.model_dump())
                for item in diagnostics
            ),
            handler_counts=(),
        )
    )


def execute_with_source_provider(
    specification: Specification, source_provider: SourceProvider
) -> NativeDatasetRun:
    """Admit one normalized specification before reading sources and run Rust once.

    Only schema-normalized specifications are accepted. The provider and artifact
    codecs retain their existing IO contracts. There are no user execution hooks,
    callbacks, implicit backend fallback, file publication or expected-file reads.
    Native transport/resource errors propagate separately from semantic failures.
    """
    return _execute(specification, source_provider)


def execute_with_project_functions(
    specification: Specification,
    source_provider: SourceProvider,
    project_root: str | Path,
    schema_root: str | Path,
    *,
    resolver: ArtifactResolver | None = None,
    cache: NativeActivationCache | None = NATIVE_ACTIVATION_CACHE,
) -> NativeDatasetRun:
    """Activate a pinned project through native invocation before reading sources.

    The host loads environments, verifies artifacts and compares activation
    vectors. Rust owns each invocation and all admitted dataset derivations.
    Reference activation cache entries cannot qualify this optional backend.
    """
    return _execute(
        specification,
        source_provider,
        lambda spec: activate_project(spec, project_root, schema_root, resolver, cache),
    )


def _execute(specification, source_provider, prepare_functions=None):
    """Capture and admit a whole run before activating callbacks or reading data."""
    # Frozen Pydantic models still contain mutable lists/dictionaries. Retain
    # the admitted run independently of caller/provider mutations during IO.
    specification = specification.model_copy(deep=True)
    try:
        admit(specification, allow_functions=prepare_functions is not None)
    except ExecutionPlanningError as error:
        return _failure(error.diagnostics)
    except UnsupportedPlanningError as error:
        return NativeDatasetRun(
            ExecutionUnsupported(features=error.features, handler_counts=())
        )

    import yamaa_native

    # Fail on a missing native API before the source provider runs.
    execute = yamaa_native.execute_dataset
    if not callable(execute):
        raise TypeError("native execute_dataset must be callable")
    required = [
        (
            "row_filter",
            UnsupportedFeature(
                operation="native_row_filter", spec_path=f"rows[{i}].filter"
            ),
        )
        for i, row in enumerate(specification.rows or ())
        if row.filter is not None
    ]
    required.extend(
        (
            "predicate_checks",
            UnsupportedFeature(
                operation="native_predicate_checks",
                spec_path=f"verifications[{i}].{check.operation}",
            ),
        )
        for i, check in enumerate(specification.verifications or ())
        if check.operation in {"assert", "implies"}
    )
    if len(specification.input) > 1 and specification.rows:
        required.append(
            (
                "row_source_lookup",
                UnsupportedFeature(
                    operation="native_row_source_lookup", spec_path="rows"
                ),
            )
        )
    handler_sites = [
        f"columns.{column.name}.derivation.unconvertible"
        for column in specification.columns
        if column.derivation is not None
        and "unconvertible" in column.derivation.model_fields_set
    ]
    handler_sites.extend(
        f"rows[{index}].derivations.{name}.unconvertible"
        for index, row in enumerate(specification.rows or ())
        for name, declaration in row.derivations.items()
        if "unconvertible" in declaration.model_fields_set
    )
    required.extend(
        (
            "unconvertible",
            UnsupportedFeature(operation="native_unconvertible", spec_path=path),
        )
        for path in handler_sites
    )
    compute_sites = [
        f"columns.{column.name}.derivation.compute"
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation == "compute"
    ]
    compute_sites.extend(
        f"rows[{index}].derivations.{name}.compute"
        for index, row in enumerate(specification.rows or ())
        for name, declaration in row.derivations.items()
        if declaration.value.operation == "compute"
    )
    required.extend(
        (
            "numeric_compute",
            UnsupportedFeature(operation="native_numeric_compute", spec_path=path),
        )
        for path in compute_sites
    )
    if specification.filter is not None:
        required.append(
            (
                "root_filter",
                UnsupportedFeature(operation="native_root_filter", spec_path="filter"),
            )
        )
    if not specification.rows:
        required.append(
            (
                "key_grain",
                UnsupportedFeature(operation="native_key_grain", spec_path="rows"),
            )
        )
    required.extend(
        (
            "window_numbering",
            UnsupportedFeature(
                operation="native_window_numbering",
                spec_path=f"columns.{column.name}.derivation",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation in NUMBERING
    )
    required.extend(
        (
            "window_filter",
            UnsupportedFeature(
                operation="native_window_filter",
                spec_path=f"columns.{column.name}.derivation.{column.derivation.value.operation}.window.filter",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation in WINDOWS
        and column.derivation.value.root[column.derivation.value.operation]
        .get("window", {})
        .get("filter")
        is not None
    )
    required.extend(
        (
            "window_values",
            UnsupportedFeature(
                operation="native_window_values",
                spec_path=f"columns.{column.name}.derivation.{column.derivation.value.operation}",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation in WINDOW_VALUES
    )
    required.extend(
        (
            "window_baseline",
            UnsupportedFeature(
                operation="native_window_baseline",
                spec_path=f"columns.{column.name}.derivation.baseline_flag",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation == "baseline_flag"
    )
    required.extend(
        (
            "source_filter",
            UnsupportedFeature(
                operation="native_source_filter",
                spec_path=f"columns.{column.name}.derivation.source.filter",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation == "source"
        and isinstance(column.derivation.value.root["source"], dict)
        and column.derivation.value.root["source"].get("filter") is not None
    )
    required.extend(
        (
            "source_selection",
            UnsupportedFeature(
                operation="native_source_selection",
                spec_path=f"columns.{column.name}.derivation.source",
            ),
        )
        for column in specification.columns
        if column.derivation is not None
        and column.derivation.value.operation == "source"
        and isinstance(column.derivation.value.root["source"], dict)
        and column.derivation.value.root["source"].get("order_by") is not None
    )
    if specification.intermediates:
        required.append(
            (
                "named_intermediate",
                UnsupportedFeature(
                    operation="native_named_intermediate", spec_path="intermediates"
                ),
            )
        )
    if len(specification.input) > 1:
        required.append(
            (
                "multi_source",
                UnsupportedFeature(operation="native_multi_source", spec_path="input"),
            )
        )
    if prepare_functions is not None:
        required.append(
            (
                "host_functions",
                UnsupportedFeature(operation="native_host_functions", spec_path="$"),
            )
        )
        if not specification.rows:
            required.append(
                (
                    "function_source_collection",
                    UnsupportedFeature(
                        operation="native_function_source_collection", spec_path="$"
                    ),
                )
            )
    if required:
        discover = getattr(yamaa_native, "dataset_capabilities", None)
        capabilities = json.loads(discover()) if callable(discover) else {}
        features = (
            capabilities.get("features", [])
            if capabilities.get("protocol") == "dataset/1"
            else []
        )
        refused = tuple(feature for name, feature in required if name not in features)
        if refused:
            return NativeDatasetRun(
                ExecutionUnsupported(features=refused, handler_counts=())
            )
    if len(specification.input) > 1:
        execute = getattr(yamaa_native, "execute_dataset_sources", None)
        if not callable(execute):
            raise TypeError("native execute_dataset_sources must be callable")
    for service, operation in (
        ("analyze_dependencies", "native_dependency_analysis"),
        ("analyze_column_dependencies", "native_column_dependency_analysis"),
        ("_compile_reference_catalog", "native_reference_binding"),
    ):
        if not callable(getattr(yamaa_native, service, None)):
            return NativeDatasetRun(
                ExecutionUnsupported(
                    features=(UnsupportedFeature(operation=operation, spec_path="$"),),
                    handler_counts=(),
                )
            )
    discover_references = getattr(yamaa_native, "reference_capabilities", None)
    reference_features = (
        json.loads(discover_references()) if callable(discover_references) else {}
    )
    for feature, operation in (
        ("qualified_validation", "native_qualified_reference_validation"),
        ("intermediate_validation", "native_intermediate_reference_validation"),
    ):
        if reference_features.get(
            "protocol"
        ) != "reference-analysis/1" or feature not in reference_features.get(
            "features", []
        ):
            return NativeDatasetRun(
                ExecutionUnsupported(
                    features=(UnsupportedFeature(operation=operation, spec_path="$"),),
                    handler_counts=(),
                )
            )
    dependency_analyzer = bind_dependency_analyzer(yamaa_native)
    column_dependency_analyzer = bind_column_dependency_analyzer(yamaa_native)
    reference_compiler_factory = bind_reference_compiler(yamaa_native)
    functions = None
    if prepare_functions is not None:
        execute = getattr(yamaa_native, "execute_dataset_functions", None)
        if not callable(execute):
            raise TypeError("native execute_dataset_functions must be callable")
        try:
            functions = prepare_functions(specification)
        except FunctionActivationError as error:
            return _failure(error.diagnostics)
    try:
        sources = source_provider(
            {
                name: declaration.model_copy(deep=True)
                for name, declaration in specification.input.items()
            }
        )
    except SourceError as error:
        return _failure(error.diagnostics)
    except ProducerSchemaUnresolved as error:
        return NativeDatasetRun(
            ExecutionUnsupported(
                features=tuple(
                    UnsupportedFeature(
                        operation="workflow_schema_resolution",
                        spec_path=f"input.{name}.schema",
                    )
                    for name in error.datasets
                ),
                handler_counts=(),
            )
        )
    try:
        plan = plan_execution(
            specification,
            sources,
            supported_operations=OPERATIONS | {"function"}
            if functions is not None
            else OPERATIONS,
            dependency_analyzer=dependency_analyzer,
            column_dependency_analyzer=column_dependency_analyzer,
            reference_compiler_factory=reference_compiler_factory,
        )
    except ExecutionPlanningError as error:
        return _failure(error.diagnostics)
    except UnsupportedPlanningError as error:
        return NativeDatasetRun(
            ExecutionUnsupported(features=error.features, handler_counts=())
        )
    dataset = primary_source(specification)
    source = sources[dataset]
    source = source.table if isinstance(source, LoadedDataset) else source
    secondary = {
        name: table.table if isinstance(table, LoadedDataset) else table
        for name, table in sources.items()
        if name != dataset
    }
    try:
        lowered, declaration_error = lower(plan, source, secondary, functions=functions)
    except UnsupportedPlanningError as error:
        return NativeDatasetRun(
            ExecutionUnsupported(features=error.features, handler_counts=())
        )
    request = json.dumps(lowered, ensure_ascii=True, separators=(",", ":"))
    data, encoded = (
        execute(
            request,
            _source_ipc(source),
            [_source_ipc(table) for table in secondary.values()],
            list(functions.callbacks),
        )
        if functions is not None
        else (
            execute(
                request,
                _source_ipc(source),
                [_source_ipc(table) for table in secondary.values()],
            )
            if secondary
            else execute(request, _source_ipc(source))
        )
    )
    envelope = json.loads(encoded)
    if envelope["protocol"] != "dataset/1":
        raise ValueError("unsupported native dataset response protocol")
    handler_counts = tuple(
        HandlerCount(
            spec_path=entry["spec_path"],
            handler=entry["handler"],
            count=int(entry["count"]),
        )
        for entry in envelope.get("handler_counts", ())
    )
    outcome = envelope["outcome"]
    status = outcome["status"]
    if (data is not None) != (status == "success"):
        raise ValueError("native dataset response has inconsistent output ownership")
    if status == "limit":
        raise NativeDatasetLimitError(outcome, handler_counts)
    records = ()
    if status == "condition":
        if "verifications" in outcome:
            records, _ = observations(specification, outcome)
        diagnostics = (condition(outcome, specification.keys),)
    elif status in {"success", "failure"}:
        records, diagnostics = observations(specification, outcome)
        if bool(diagnostics) != (status == "failure"):
            raise ValueError("native dataset response has inconsistent check outcomes")
    else:
        raise ValueError("unknown native dataset response status")
    if declaration_error is not None and (
        status == "success"
        or (status == "failure" and outcome["phase"] == "verification")
    ):
        diagnostics = (declaration_error,)
        status = "failure"
    verification_log = build_verification_log(
        records if specification.output.verification_log is not None else (),
        specification.output,
    )
    if status != "success":
        return NativeDatasetRun(
            ExecutionFailure(
                diagnostics=diagnostics,
                handler_counts=handler_counts,
                verification_log=verification_log,
            ),
            records,
        )
    table = _output_table(specification, data)
    try:
        artifact = build_artifact(table, specification.output, specification.keys)
    except ArtifactError as error:
        return NativeDatasetRun(
            _failure(error.diagnostics).result.model_copy(
                update={
                    "verification_log": verification_log,
                    "handler_counts": handler_counts,
                }
            ),
            records,
        )
    return NativeDatasetRun(
        ExecutionSuccess(
            table=table,
            artifact=artifact,
            handler_counts=handler_counts,
            verification_log=verification_log,
            warning_log=build_warning_log((), specification.output),
        ),
        records,
    )
