"""Secondary input/output adapters behind one common contract.

Each format lives in its own module: ``csv`` today, ``xpt`` and
``datajson`` next. Host-table treatment lives in ``polars``. Every
adapter delivers plain text-or-missing values; quoting is a transport
detail no adapter preserves.
"""

from yamaa.io.source import (
    LoadedDataset,
    SourceDiagnostic,
    SourceError,
    load_source_table,
    load_source_tables,
)

__all__ = [
    "LoadedDataset",
    "SourceDiagnostic",
    "SourceError",
    "load_source_table",
    "load_source_tables",
]
