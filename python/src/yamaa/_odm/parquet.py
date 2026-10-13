"""Temporary private reference compatibility; retired at qualified cutover."""

<<<<<<<< HEAD:python/src/yamaa/_odm/parquet.py
from __future__ import annotations

import os
import tempfile
from pathlib import Path

import pyarrow.parquet as pq

from yamaa._odm.errors import ODMError
from yamaa._odm.readers import iter_odm_records
from yamaa._odm.schema import (
    SCHEMA_VERSION,
    ClinicalItemRow,
    ParquetWriteResult,
    rows_to_frame,
)

_PARQUET_COMPRESSIONS = {
    "brotli",
    "gzip",
    "lz4",
    "snappy",
    "uncompressed",
    "zstd",
}
========
from importlib import import_module
>>>>>>>> fix/pr2100:python/src/yamaa/odm/parquet.py


def __getattr__(name):
    if name not in ("write_odm_parquet",):
        raise AttributeError(name)
    return getattr(import_module("yamaa._odm.parquet"), name)
