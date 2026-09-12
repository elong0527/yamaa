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
from yamaa.runtime.lifecycle import HandlerCount

__all__ = [
    "ExecutionFailure",
    "ExecutionHooks",
    "ExecutionResult",
    "ExecutionSuccess",
    "ExecutionUnsupported",
    "HandlerCount",
    "SourceProvider",
    "execute_specification",
    "execute_with_source_provider",
]
