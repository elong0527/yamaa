"""Temporary private reference compatibility; retired at qualified cutover."""

from importlib import import_module


def __getattr__(name):
    if name not in ("iter_odm_records", "read_odm"):
        raise AttributeError(name)
    return getattr(import_module("yamaa._odm.readers"), name)
