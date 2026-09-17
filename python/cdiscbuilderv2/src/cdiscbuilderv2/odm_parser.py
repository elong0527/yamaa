"""
Native CDISC ODM XML Parser for Python and Polars.
Parses OpenClinica, Medidata Rave, Castor, and standard CDISC ODM XML files (v1.2, v1.3, v2.0).
Extracts both ClinicalData and MetaDataVersion with high throughput.
"""

import logging
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple, Union
import xml.etree.ElementTree as ET
import defusedxml.ElementTree as dET
import polars as pl

logger = logging.getLogger(__name__)


class ODMParser:
    """
    Parses CDISC ODM XML files into structured Polars DataFrames and metadata dictionaries.
    """

    def __init__(self, xml_source: Union[str, Path, bytes]):
        self.xml_source = xml_source
        self.tree: Optional[ET.ElementTree] = None
        self.root: Optional[ET.Element] = None
        self.ns: Dict[str, str] = {}
        
        self.study_info: Dict[str, Any] = {}
        self.form_defs: Dict[str, Dict[str, Any]] = {}
        self.item_defs: Dict[str, Dict[str, Any]] = {}
        self.item_group_defs: Dict[str, Dict[str, Any]] = {}
        self.study_event_defs: Dict[str, Dict[str, Any]] = {}
        self.codelists: Dict[str, Dict[str, Any]] = {}
        
        self.df_long: Optional[pl.DataFrame] = None
        self.metadata_df: Optional[pl.DataFrame] = None

        self._load_and_parse()

    def _strip_tag(self, tag: str) -> str:
        """Strip XML namespace prefix from tag name."""
        if "}" in tag:
            return tag.split("}", 1)[1]
        return tag

    def _load_and_parse(self) -> None:
        """Load XML and extract metadata and clinical data."""
        if isinstance(self.xml_source, (str, Path)):
            path = Path(self.xml_source)
            if not path.exists():
                raise FileNotFoundError(f"ODM XML file not found: {path}")
            self.tree = dET.parse(str(path))
            self.root = self.tree.getroot()
        elif isinstance(self.xml_source, bytes):
            self.root = dET.fromstring(self.xml_source)
        else:
            raise ValueError("Unsupported xml_source type. Expected Path, str, or bytes.")

        if self.root.tag.startswith("{"):
            ns_uri = self.root.tag.split("}")[0][1:]
            self.ns = {"odm": ns_uri}
        else:
            self.ns = {"odm": ""}

        self._parse_metadata()
        self._parse_clinical_data()

    def _parse_metadata(self) -> None:
        """Parse Study, MetaDataVersion, FormDefs, ItemDefs, ItemGroupDefs, StudyEventDefs, and CodeLists."""
        for elem in self.root.iter():
            tag = self._strip_tag(elem.tag)
            if tag == "Study":
                self.study_info["OID"] = elem.attrib.get("OID", "")
                self.study_info["StudyName"] = elem.attrib.get("StudyName", "")
                self.study_info["ProtocolName"] = elem.attrib.get("ProtocolName", "")
                
                for child in elem:
                    if self._strip_tag(child.tag) == "Description":
                        for desc_child in child:
                            if self._strip_tag(desc_child.tag) == "TranslatedText":
                                self.study_info["Description"] = (desc_child.text or "").strip()

            elif tag == "FormDef":
                oid = elem.attrib.get("OID", "")
                name = elem.attrib.get("Name", oid)
                repeating = elem.attrib.get("Repeating", "No")
                group_refs = []
                for child in elem:
                    if self._strip_tag(child.tag) == "ItemGroupRef":
                        group_refs.append(child.attrib.get("ItemGroupOID"))
                self.form_defs[oid] = {
                    "OID": oid,
                    "Name": name,
                    "Repeating": repeating,
                    "ItemGroupRefs": group_refs
                }

            elif tag == "ItemDef":
                oid = elem.attrib.get("OID", "")
                name = elem.attrib.get("Name", oid)
                data_type = elem.attrib.get("DataType", "text")
                length = elem.attrib.get("Length", "")
                
                question = ""
                for child in elem:
                    if self._strip_tag(child.tag) == "Question":
                        for q_child in child:
                            if self._strip_tag(q_child.tag) == "TranslatedText":
                                question = (q_child.text or "").strip()

                self.item_defs[oid] = {
                    "OID": oid,
                    "Name": name,
                    "DataType": data_type,
                    "Length": length,
                    "Question": question
                }

            elif tag == "ItemGroupDef":
                oid = elem.attrib.get("OID", "")
                name = elem.attrib.get("Name", oid)
                repeating = elem.attrib.get("Repeating", "No")
                item_refs = []
                for child in elem:
                    if self._strip_tag(child.tag) == "ItemRef":
                        item_refs.append(child.attrib.get("ItemOID"))
                self.item_group_defs[oid] = {
                    "OID": oid,
                    "Name": name,
                    "Repeating": repeating,
                    "ItemRefs": item_refs
                }

            elif tag == "StudyEventDef":
                oid = elem.attrib.get("OID", "")
                name = elem.attrib.get("Name", oid)
                repeating = elem.attrib.get("Repeating", "No")
                event_type = elem.attrib.get("Type", "Scheduled")
                group_refs = []
                for child in elem:
                    if self._strip_tag(child.tag) == "ItemGroupRef":
                        group_refs.append(child.attrib.get("ItemGroupOID"))
                self.study_event_defs[oid] = {
                    "OID": oid,
                    "Name": name,
                    "Repeating": repeating,
                    "Type": event_type,
                    "ItemGroupRefs": group_refs
                }

            elif tag == "CodeList":
                oid = elem.attrib.get("OID", "")
                name = elem.attrib.get("Name", oid)
                items = {}
                for child in elem:
                    if self._strip_tag(child.tag) == "CodeListItem":
                        coded_val = child.attrib.get("CodedValue", "")
                        decode = ""
                        for dec_child in child:
                            if self._strip_tag(dec_child.tag) == "Decode":
                                for t_child in dec_child:
                                    if self._strip_tag(t_child.tag) == "TranslatedText":
                                        decode = (t_child.text or "").strip()
                        items[coded_val] = decode
                self.codelists[oid] = {
                    "OID": oid,
                    "Name": name,
                    "Items": items
                }

    def _parse_clinical_data(self) -> None:
        """Parse SubjectData, StudyEventData, and all nested forms/groups/items."""
        rows: List[Dict[str, Any]] = []

        for elem in self.root.iter():
            if self._strip_tag(elem.tag) != "ClinicalData":
                continue

            study_oid = elem.attrib.get("StudyOID", self.study_info.get("OID", ""))

            for subj in elem:
                if self._strip_tag(subj.tag) != "SubjectData":
                    continue

                subj_key = (
                    subj.attrib.get("SubjectKey")
                    or subj.attrib.get("StudySubjectID")
                    or subj.attrib.get("studysubjectid")
                    or ""
                )
                study_subj_id = (
                    subj.attrib.get("StudySubjectID")
                    or subj.attrib.get("studysubjectid")
                    or subj_key
                )

                for se in subj:
                    if self._strip_tag(se.tag) != "StudyEventData":
                        continue

                    se_oid = se.attrib.get("StudyEventOID", "")
                    se_repeat = se.attrib.get("StudyEventRepeatKey", "1")
                    start_date = se.attrib.get("StartDate", "")

                    def extract_items_recursive(node: ET.Element, current_form: str, current_group: str, current_repeat: str):
                        for child in node:
                            ctag = self._strip_tag(child.tag)
                            if ctag == "FormData":
                                form_val = child.attrib.get("FormOID", current_form)
                                extract_items_recursive(child, form_val, current_group, current_repeat)
                            elif ctag == "ItemGroupData":
                                ig_val = child.attrib.get("ItemGroupOID", current_group)
                                ig_repeat = child.attrib.get("ItemGroupRepeatKey", current_repeat)
                                # If form_val is empty or looks like FormOID (starts with FO.), assign
                                form_val = current_form
                                if not form_val or ig_val.startswith("FO."):
                                    form_val = ig_val
                                extract_items_recursive(child, form_val, ig_val, ig_repeat)
                            elif ctag == "ItemData":
                                item_oid = child.attrib.get("ItemOID", "")
                                val = child.attrib.get("Value", None)
                                if val is None:
                                    for v_child in child:
                                        if self._strip_tag(v_child.tag) == "Value":
                                            val = (v_child.text or "").strip()
                                            break

                                meta = self.item_defs.get(item_oid, {})
                                rows.append({
                                    "StudyOID": study_oid,
                                    "SubjectKey": subj_key,
                                    "StudySubjectID": study_subj_id,
                                    "StudyEventOID": se_oid,
                                    "StudyEventRepeatKey": se_repeat,
                                    "StudyEventStartDate": start_date,
                                    "FormOID": current_form,
                                    "ItemGroupOID": current_group,
                                    "ItemGroupRepeatKey": current_repeat,
                                    "ItemOID": item_oid,
                                    "Value": val,
                                    "Question": meta.get("Question", ""),
                                    "ItemName": meta.get("Name", "")
                                })

                    extract_items_recursive(se, "", "", "1")

        if rows:
            self.df_long = pl.DataFrame(rows)
        else:
            self.df_long = pl.DataFrame({
                "StudyOID": pl.Series([], dtype=pl.Utf8),
                "SubjectKey": pl.Series([], dtype=pl.Utf8),
                "StudySubjectID": pl.Series([], dtype=pl.Utf8),
                "StudyEventOID": pl.Series([], dtype=pl.Utf8),
                "StudyEventRepeatKey": pl.Series([], dtype=pl.Utf8),
                "StudyEventStartDate": pl.Series([], dtype=pl.Utf8),
                "FormOID": pl.Series([], dtype=pl.Utf8),
                "ItemGroupOID": pl.Series([], dtype=pl.Utf8),
                "ItemGroupRepeatKey": pl.Series([], dtype=pl.Utf8),
                "ItemOID": pl.Series([], dtype=pl.Utf8),
                "Value": pl.Series([], dtype=pl.Utf8),
                "Question": pl.Series([], dtype=pl.Utf8),
                "ItemName": pl.Series([], dtype=pl.Utf8),
            })

    def get_metadata_summary(self) -> pl.DataFrame:
        """
        Generate a summary table of all captured metadata, item definitions, and sample values.
        """
        if self.metadata_df is not None:
            return self.metadata_df

        if self.df_long is None or self.df_long.height == 0:
            return pl.DataFrame({
                "FormOID": pl.Series([], dtype=pl.Utf8),
                "ItemGroupOID": pl.Series([], dtype=pl.Utf8),
                "ItemOID": pl.Series([], dtype=pl.Utf8),
                "ItemName": pl.Series([], dtype=pl.Utf8),
                "Question": pl.Series([], dtype=pl.Utf8),
                "SampleValues": pl.Series([], dtype=pl.Utf8)
            })

        unique_meta = (
            self.df_long.select(["FormOID", "ItemGroupOID", "ItemOID", "ItemName", "Question"])
            .unique(subset=["FormOID", "ItemGroupOID", "ItemOID"])
            .sort(["FormOID", "ItemGroupOID", "ItemOID"])
        )

        sample_vals = []
        for row in unique_meta.iter_rows(named=True):
            f_oid = row["FormOID"]
            ig_oid = row["ItemGroupOID"]
            i_oid = row["ItemOID"]

            sub_df = self.df_long.filter(
                (pl.col("FormOID") == f_oid)
                & (pl.col("ItemGroupOID") == ig_oid)
                & (pl.col("ItemOID") == i_oid)
                & pl.col("Value").is_not_null()
                & (pl.col("Value") != "")
                & (pl.col("Value") != "NA")
            )
            vals = sub_df["Value"].unique().head(3).to_list()
            sample_vals.append(str(vals))

        self.metadata_df = unique_meta.with_columns(pl.Series("SampleValues", sample_vals))
        return self.metadata_df

    def pivot_domain(
        self,
        form_oids: Optional[List[str]] = None,
        keys: Optional[List[str]] = None
    ) -> pl.DataFrame:
        """
        Pivot long-format data into a wide table where columns are ItemOIDs.
        """
        if self.df_long is None or self.df_long.height == 0:
            return pl.DataFrame()

        filtered = self.df_long
        if form_oids:
            filtered = filtered.filter(pl.col("FormOID").is_in(form_oids))

        if filtered.height == 0:
            return pl.DataFrame()

        if not keys:
            keys = ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"]

        valid_keys = [k for k in keys if k in filtered.columns]

        pivoted = filtered.pivot(
            on="ItemOID",
            index=valid_keys,
            values="Value",
            aggregate_function="first"
        )
        return pivoted

    def get_forms_summary(self) -> List[Dict[str, Any]]:
        """
        Returns structured summary of all CRF Forms, items, counts, and questions.
        """
        if self.df_long is None or self.df_long.height == 0:
            return []

        forms_list = []
        meta_df = self.get_metadata_summary()
        unique_forms = sorted(list(set(self.df_long["FormOID"].unique().to_list())))

        for f_oid in unique_forms:
            form_meta = self.form_defs.get(f_oid, {})
            form_name = form_meta.get("Name") or f_oid.split(".")[-1]
            
            f_items_df = meta_df.filter(pl.col("FormOID") == f_oid)
            items = f_items_df.to_dicts()
            
            f_data_df = self.df_long.filter(pl.col("FormOID") == f_oid)
            total_records = f_data_df.height
            subject_count = len(f_data_df["SubjectKey"].unique())

            forms_list.append({
                "FormOID": f_oid,
                "FormName": form_name,
                "ItemCount": len(items),
                "TotalRecords": total_records,
                "SubjectCount": subject_count,
                "Items": items
            })

        return forms_list
