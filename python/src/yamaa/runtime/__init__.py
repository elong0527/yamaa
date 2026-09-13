"""Execute normalized YAMAA specifications over typed source tables."""

from yamaa.runtime.executor import (
    ExecutionFailure,
    ExecutionHooks,
    ExecutionResult,
    ExecutionSuccess,
    ExecutionUnsupported,
    SourceProvider,
    execute_specification,
    execute_with_source_provider,
)
from yamaa.runtime.joins import (
    IndexedRecord,
    OrderError,
    RelationIndex,
    applicable_keys,
    build_relation_indexes,
    compare_values,
    eligible_records,
    evaluate_mapping_from,
    join_scalar,
    order_records,
    partition_key,
    partition_records,
    select_record,
)
from yamaa.runtime.lifecycle import HandlerCount
from yamaa.runtime.lookups import (
    LookupOutcome,
    RecordLookupSelector,
    types_comparable,
)
from yamaa.runtime.rows import (
    CandidateRow,
    RelationalContext,
    RowResolver,
    driver_groups,
    group_candidates,
)

__all__ = [
    "CandidateRow",
    "ExecutionFailure",
    "ExecutionHooks",
    "ExecutionResult",
    "ExecutionSuccess",
    "ExecutionUnsupported",
    "HandlerCount",
    "IndexedRecord",
    "LookupOutcome",
    "OrderError",
    "RecordLookupSelector",
    "RelationIndex",
    "RelationalContext",
    "RowResolver",
    "SourceProvider",
    "applicable_keys",
    "build_relation_indexes",
    "compare_values",
    "driver_groups",
    "eligible_records",
    "evaluate_mapping_from",
    "execute_specification",
    "execute_with_source_provider",
    "group_candidates",
    "join_scalar",
    "order_records",
    "partition_key",
    "partition_records",
    "select_record",
    "types_comparable",
]
