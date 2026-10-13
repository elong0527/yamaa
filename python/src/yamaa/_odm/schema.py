"""Temporary private reference compatibility; retired at qualified cutover."""

from importlib import import_module


def __getattr__(name):
    if name not in (
        "SCHEMA_VERSION",
        "ODM_ITEM_SCHEMA",
        "ClinicalItemRow",
        "ParquetWriteResult",
        "rows_to_frame",
    ):
        raise AttributeError(name)
    return getattr(import_module("yamaa._odm.schema"), name)
