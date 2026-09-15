import pytest
from pathlib import Path
from cdiscbuilderv2.pipeline import SDTMPipeline


def test_pipeline_cath_integration(tmp_path):
    cath_xml = Path("/home/ming/Documents/yamaa/cath/odm/odm.xml")
    cath_specs = Path("/home/ming/Documents/yamaa/cath/sdtm/specs")

    if not (cath_xml.exists() and cath_specs.exists()):
        pytest.skip("CATH ODM XML or specs not found")

    out_dir = tmp_path / "sdtm_out"
    pipeline = SDTMPipeline(
        xml_path=cath_xml,
        specs_dir=cath_specs,
        output_dir=out_dir
    )

    pipeline.ingest_odm()
    assert pipeline.df_long.height > 5000

    order = pipeline.compute_build_order()
    # Check that _DM_REF is built before domains that depend on it
    if "_DM_REF" in order and "DM" in order:
        assert order.index("_DM_REF") < order.index("DM")

    results = pipeline.run(export_formats=["csv", "parquet", "xpt"])
    assert len(results) > 5

    # Check DM
    assert "DM" in results
    dm_df = results["DM"]
    assert dm_df.height == 82
    assert "USUBJID" in dm_df.columns
    assert "ARM" in dm_df.columns
    assert "SEX" in dm_df.columns

    # Check Findings domain: LB
    assert "LB" in results
    lb_df = results["LB"]
    assert lb_df.height > 0
    assert "LBTESTCD" in lb_df.columns
    assert "LBORRES" in lb_df.columns
    assert "LBSTRESN" in lb_df.columns
    assert "LBDY" in lb_df.columns

    # Check Findings domain: VS
    assert "VS" in results
    vs_df = results["VS"]
    assert vs_df.height > 0
    assert "VSTESTCD" in vs_df.columns

    # Check saved files
    assert (out_dir / "dm.csv").exists()
    assert (out_dir / "dm.parquet").exists()
    assert (out_dir / "dm.xpt").exists()
    assert (out_dir / "lb.csv").exists()
    assert (out_dir / "vs.csv").exists()

    # Check zip packaging
    zip_p = pipeline.create_zip_package(tmp_path / "submission.zip")
    assert zip_p.exists()
    assert zip_p.stat().st_size > 0
