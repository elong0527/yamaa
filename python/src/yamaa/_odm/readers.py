"""Temporary private reference compatibility; retired at qualified cutover."""

<<<<<<<< HEAD:python/src/yamaa/_odm/readers.py
from __future__ import annotations

import tarfile
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path, PurePosixPath
from typing import BinaryIO, Literal

import polars as pl
from lxml import etree
from pydantic import ValidationError

from yamaa._odm.errors import ODMError
from yamaa._odm.schema import ClinicalItemRow, rows_to_frame

ODM_13_NAMESPACE = "http://www.cdisc.org/ns/odm/v1.3"
ODM_20_NAMESPACE = "http://www.cdisc.org/ns/odm/v2.0"
SUPPORTED_NAMESPACES = frozenset({ODM_13_NAMESPACE, ODM_20_NAMESPACE})

DefinitionKind = Literal["event", "form", "group", "item"]
MetadataKey = tuple[str, str, str]
========
from importlib import import_module
>>>>>>>> fix/pr2100:python/src/yamaa/odm/readers.py


def __getattr__(name):
    if name not in ("iter_odm_records", "read_odm"):
        raise AttributeError(name)
    return getattr(import_module("yamaa._odm.readers"), name)
