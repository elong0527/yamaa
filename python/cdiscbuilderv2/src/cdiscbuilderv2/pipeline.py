"""
SDTM Pipeline Orchestrator.
Orchestrates multi-domain CDISC build from ODM XML and YAML mapping specifications
with topological dependency resolution, cross-domain reference tracking, and multi-format export.
"""

import logging
import zipfile
import re
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple, Union
import polars as pl
import yaml

from .engine import CDISCEngine
from .odm_parser import ODMParser
from .verifications import VerificationReport

logger = logging.getLogger(__name__)


class SDTMPipeline:
    """
    Orchestrates the entire SDTM generation workflow:
    1. Parses raw EDC ODM XML into normalized clinical & metadata dataframes.
    2. Analyzes YAML mapping specifications and computes topological dependency build order.
    3. Transforms, stacks, and derives all SDTM domains with cross-domain context.
    4. Executes data quality & conformance verifications.
    5. Exports to Parquet, CSV, and SAS Transport (.xpt) datasets.
    """

    def __init__(
        self,
        xml_path: Optional[Union[str, Path]] = None,
        specs_dir: Optional[Union[str, Path]] = None,
        specs_dict: Optional[Dict[str, Any]] = None,
        output_dir: Optional[Union[str, Path]] = None
    ):
        self.xml_path = Path(xml_path) if xml_path else None
        self.specs_dir = Path(specs_dir) if specs_dir else None
        self.output_dir = Path(output_dir) if output_dir else Path("./sdtm_output")
        
        self.odm_parser: Optional[ODMParser] = None
        self.df_long: Optional[pl.DataFrame] = None
        self.metadata_df: Optional[pl.DataFrame] = None
        
        self.specs: Dict[str, Any] = specs_dict or {}
        self.built_domains: Dict[str, pl.DataFrame] = {}
        self.verification_reports: Dict[str, VerificationReport] = {}
        self.execution_logs: List[Dict[str, Any]] = []

        if self.specs_dir and self.specs_dir.exists():
            self._load_specs_from_dir()

    def _load_specs_from_dir(self) -> None:
        """Load all .yaml specifications from specs_dir."""
        for yml_file in sorted(self.specs_dir.glob("*.yaml")):
            try:
                with open(yml_file, "r") as f:
                    content = yaml.safe_load(f)
                if not content:
                    continue

                # If domain-keyed dict (e.g. { "DM": [...] })
                if isinstance(content, dict) and "domain" not in content:
                    for d_name, d_cfg in content.items():
                        self.specs[d_name] = {
                            "domain": d_name,
                            "is_domain_format": True,
                            "raw_config": d_cfg
                        }
                else:
                    d_name = content.get("domain", yml_file.stem.upper())
                    content["domain"] = d_name
                    self.specs[d_name] = content
            except Exception as e:
                logger.error(f"Error loading specification {yml_file}: {e}")

    def ingest_odm(self, xml_source: Optional[Union[str, Path, bytes]] = None) -> ODMParser:
        """Parse EDC ODM XML file."""
        src = xml_source or self.xml_path
        if not src:
            raise ValueError("No XML source provided for ODM ingestion.")

        logger.info(f"Ingesting ODM XML from {src}")
        self.odm_parser = ODMParser(src)
        self.df_long = self.odm_parser.df_long
        self.metadata_df = self.odm_parser.get_metadata_summary()
        return self.odm_parser

    def _extract_domain_dependencies(self, domain_name: str, spec: Dict[str, Any]) -> List[str]:
        """Scan spec to identify references to other domains."""
        deps: Set[str] = set()

        def scan_val(val: Any) -> None:
            if isinstance(val, str):
                if "." in val and not (val.startswith("'") or val.startswith('"') or val.startswith("IT.") or val.startswith("FO.") or val.startswith("SE.") or val.startswith("IG.")):
                    ref = val.split(".", 1)[0]
                    if ref != domain_name and re.match(r"^_?[A-Z0-9_]+$", ref):
                        deps.add(ref)
            elif isinstance(val, list):
                for item in val:
                    scan_val(item)
            elif isinstance(val, dict):
                for k, v in val.items():
                    if k in ("source", "args", "dataset", "base"):
                        scan_val(v)
                    else:
                        scan_val(v)

        scan_val(spec)
        return list(deps)

    def compute_build_order(self) -> List[str]:
        """Topological sort of domain specifications according to dependencies."""
        graph: Dict[str, List[str]] = {}
        for domain, spec in self.specs.items():
            graph[domain] = self._extract_domain_dependencies(domain, spec)

        all_domains = list(self.specs.keys())
        visited: Set[str] = set()
        temp_mark: Set[str] = set()
        order: List[str] = []

        def visit(node: str) -> None:
            if node in temp_mark:
                logger.warning(f"Circular dependency detected involving {node}; breaking cycle.")
                return
            if node in visited:
                return
            temp_mark.add(node)
            for dep in graph.get(node, []):
                if dep in all_domains:
                    visit(dep)
            temp_mark.remove(node)
            visited.add(node)
            order.append(node)

        for d in all_domains:
            visit(d)

        return order

    def run(self, export_formats: Optional[List[str]] = None) -> Dict[str, pl.DataFrame]:
        """
        Execute the full SDTM generation pipeline.
        """
        if self.df_long is None:
            if self.xml_path and self.xml_path.exists():
                self.ingest_odm()
            else:
                logger.warning("No ODM XML loaded; attempting to build with existing datasets.")

        build_order = self.compute_build_order()
        logger.info(f"SDTM Build order: {' -> '.join(build_order)}")
        self.execution_logs.clear()

        for domain in build_order:
            spec = self.specs[domain]
            logger.info(f"Building domain: {domain}")
            
            try:
                engine = CDISCEngine(
                    spec=spec,
                    df_long=self.df_long,
                    built_domains=self.built_domains
                )
                df = engine.build()
                self.built_domains[domain] = df
                if engine.verification_report:
                    self.verification_reports[domain] = engine.verification_report

                # Save if output directory is defined and domain is a standard SDTM output (exclude temporary ref tables if starting with _)
                if self.output_dir and not domain.startswith("_") and df.height > 0:
                    saved = engine.save(self.output_dir, export_formats)
                    self.execution_logs.append({
                        "domain": domain,
                        "status": "SUCCESS",
                        "rows": df.height,
                        "cols": df.width,
                        "files": {k: str(v) for k, v in saved.items()},
                        "validations": engine.verification_report.to_dict() if engine.verification_report else {}
                    })
                else:
                    self.execution_logs.append({
                        "domain": domain,
                        "status": "SUCCESS",
                        "rows": df.height,
                        "cols": df.width,
                        "validations": engine.verification_report.to_dict() if engine.verification_report else {}
                    })

            except Exception as e:
                logger.error(f"Failed to build domain {domain}: {e}", exc_info=True)
                self.execution_logs.append({
                    "domain": domain,
                    "status": "ERROR",
                    "error": str(e)
                })

        return self.built_domains

    def create_zip_package(self, output_zip_path: Union[str, Path]) -> Path:
        """Create a zip package of all exported SDTM datasets in output_dir."""
        zip_p = Path(output_zip_path)
        zip_p.parent.mkdir(parents=True, exist_ok=True)

        with zipfile.ZipFile(zip_p, "w", zipfile.ZIP_DEFLATED) as zf:
            for file_path in self.output_dir.glob("*.*"):
                if file_path.suffix.lower() in (".csv", ".parquet", ".xpt", ".json"):
                    zf.write(file_path, arcname=file_path.name)

        return zip_p
