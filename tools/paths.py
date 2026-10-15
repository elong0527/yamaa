"""Repository locations shared by development tools."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT / "rust"
