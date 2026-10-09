from __future__ import annotations

from pathlib import Path

import pytest
from _odm_documents import ODM_13, ODM_20


@pytest.fixture
def odm13_path(tmp_path: Path) -> Path:
    path = tmp_path / "odm13.xml"
    path.write_text(ODM_13, encoding="utf-8")
    return path


@pytest.fixture
def odm20_path(tmp_path: Path) -> Path:
    path = tmp_path / "odm20.xml"
    path.write_text(ODM_20, encoding="utf-8")
    return path
