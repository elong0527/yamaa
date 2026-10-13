"""Temporary private reference compatibility; retired at qualified cutover."""

from importlib import import_module


def __getattr__(name):
    if name not in ("write_odm_parquet",):
        raise AttributeError(name)
    return getattr(import_module("yamaa._odm.parquet"), name)
