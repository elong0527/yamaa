"""
CLI interface for CDISC Builder v2.
Supports headless batch SDTM creation, parsing ODM XML, and exporting
built-in Yamaa schemas and templates.
"""

import argparse
import shutil
import sys
from pathlib import Path
from .pipeline import SDTMPipeline
from .odm_parser import ODMParser

SCHEMAS_DIR = Path(__file__).parent / "schemas"


def main(args=None):
    parser = argparse.ArgumentParser(
        prog="cdiscbuilder",
        description="CDISC Builder v2: Next-generation SDTM creation engine with EDC ODM XML ingestion & AI Yamaa Schema Synthesizer"
    )
    parser.add_argument("--version", "-v", action="version", version="CDISC Builder v2.0 (Yamaa Schema Standard)")
    subparsers = parser.add_subparsers(dest="command", help="Available subcommands")

    # Command: build
    build_parser = subparsers.add_parser("build", help="Build SDTM datasets from ODM XML and YAML specs (Headless)")
    build_parser.add_argument("--xml", "-x", type=str, required=True, help="Path to EDC ODM XML file")
    build_parser.add_argument("--specs", "-s", type=str, required=True, help="Directory containing YAML domain specs")
    build_parser.add_argument("--output", "-o", type=str, default="./sdtm_output", help="Directory to save SDTM datasets")
    build_parser.add_argument("--formats", "-f", type=str, default="parquet,csv,xpt", help="Comma-separated export formats (parquet,csv,xpt)")

    # Command: parse-odm
    parse_parser = subparsers.add_parser("parse-odm", help="Parse ODM XML into long-format dataset")
    parse_parser.add_argument("--xml", "-x", type=str, required=True, help="Path to EDC ODM XML file")
    parse_parser.add_argument("--output", "-o", type=str, default="long_data.csv", help="Output file path (.csv or .parquet)")

    # Command: schemas
    schema_parser = subparsers.add_parser("schemas", help="List or export built-in Yamaa schema standards & templates")
    schema_parser.add_argument("--export-dir", "-e", type=str, help="Directory to export built-in schema templates to")
    schema_parser.add_argument("--template", "-t", type=str, help="Specific template domain to print (e.g. DM, VS, LB, AE)")

    parsed_args = parser.parse_args(args)

    if parsed_args.command == "build":
        formats = [f.strip() for f in parsed_args.formats.split(",")]
        pipeline = SDTMPipeline(
            xml_path=parsed_args.xml,
            specs_dir=parsed_args.specs,
            output_dir=parsed_args.output
        )
        print(f"Ingesting ODM XML: {parsed_args.xml}")
        pipeline.ingest_odm()
        print(f"Building SDTM datasets into {parsed_args.output}...")
        pipeline.run(export_formats=formats)
        print("\n--- SDTM Build Summary ---")
        for log in pipeline.execution_logs:
            print(f"Domain {log['domain']:<10} -> {log['status']} ({log.get('rows', 0)} rows, {log.get('cols', 0)} cols)")
        print(f"\nAll datasets generated in: {parsed_args.output}")

    elif parsed_args.command == "parse-odm":
        print(f"Parsing ODM XML: {parsed_args.xml}")
        parser_obj = ODMParser(parsed_args.xml)
        out_path = Path(parsed_args.output)
        if out_path.suffix == ".parquet":
            parser_obj.df_long.write_parquet(out_path)
        else:
            parser_obj.df_long.write_csv(out_path)
        print(f"Saved long data to: {out_path} ({parser_obj.df_long.height} rows)")

    elif parsed_args.command == "schemas":
        templates_dir = SCHEMAS_DIR / "templates"
        if parsed_args.template:
            t_file = templates_dir / f"{parsed_args.template.lower()}.yaml"
            if t_file.exists():
                print(t_file.read_text())
            else:
                print(f"Template '{parsed_args.template}' not found. Available: {[p.stem.upper() for p in templates_dir.glob('*.yaml')]}")
        elif parsed_args.export_dir:
            out_dir = Path(parsed_args.export_dir)
            out_dir.mkdir(parents=True, exist_ok=True)
            for f in templates_dir.glob("*.yaml"):
                shutil.copy(f, out_dir / f.name)
            print(f"Exported {len(list(templates_dir.glob('*.yaml')))} schema templates to: {out_dir}")
        else:
            print("Built-in Yamaa Domain Schema Templates:")
            for f in sorted(templates_dir.glob("*.yaml")):
                print(f"  - {f.stem.upper():<10} ({f.stat().st_size} bytes)")
            print("\nUse `cdiscbuilder schemas --export-dir <dir>` to export all templates.")

    else:
        parser.print_help()


if __name__ == "__main__":
    main()
