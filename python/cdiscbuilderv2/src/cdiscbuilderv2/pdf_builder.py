"""
Regulatory Clinical Document PDF Builder.
Generates publication-ready, ICH GCP E6 (R2) and ICH E9 compliant PDF documents
for Clinical Trial Protocols and Statistical Analysis Plans (SAP) using ReportLab.
"""

import io
import re
import logging
from typing import Any, Dict, List, Optional
from datetime import datetime

from reportlab.lib.pagesizes import letter
from reportlab.lib import colors
from reportlab.platypus import (
    SimpleDocTemplate,
    Paragraph,
    Spacer,
    Table,
    TableStyle,
    PageBreak,
    KeepTogether,
    HRFlowable
)
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.enums import TA_CENTER, TA_LEFT, TA_RIGHT, TA_JUSTIFY
from reportlab.pdfgen import canvas

logger = logging.getLogger("cdiscbuilderv2.pdf_builder")


class NumberedCanvas(canvas.Canvas):
    """
    Two-pass canvas to dynamically compute and render 'Page X of Y' page numbers
    along with running clinical protocol headers and confidentiality notices.
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._saved_page_states = []

    def showPage(self):
        self._saved_page_states.append(dict(self.__dict__))
        self._startPage()

    def save(self):
        num_pages = len(self._saved_page_states)
        for state in self._saved_page_states:
            self.__dict__.update(state)
            self.draw_page_decorations(num_pages)
            super().showPage()
        super().save()

    def draw_page_decorations(self, page_count: int):
        self.saveState()
        self.setFont("Helvetica", 8)
        self.setFillColor(colors.HexColor("#64748b"))

        # Running Header (pages > 1)
        if self._pageNumber > 1:
            doc_title = getattr(self, "doc_title", "Clinical Study Document")
            self.drawString(54, 750, doc_title[:60])
            self.drawRightString(612 - 54, 750, "CONFIDENTIAL")
            self.setStrokeColor(colors.HexColor("#cbd5e1"))
            self.setLineWidth(0.5)
            self.line(54, 744, 612 - 54, 744)

        # Running Footer (all pages)
        self.setStrokeColor(colors.HexColor("#cbd5e1"))
        self.setLineWidth(0.5)
        self.line(54, 48, 612 - 54, 48)

        self.drawString(54, 36, "ClinForge Suite — Regulatory Document")
        page_str = f"Page {self._pageNumber} of {page_count}"
        self.drawRightString(612 - 54, 36, page_str)
        self.restoreState()


class ClinicalPDFBuilder:
    """
    Builds styled, regulatory-grade PDF documents from clinical protocol
    and statistical analysis plan markdown content and study metadata.
    """

    def __init__(self):
        self.styles = getSampleStyleSheet()
        self._setup_custom_styles()

    def _setup_custom_styles(self):
        """Initializes clinical regulatory document typography."""
        self.styles.add(ParagraphStyle(
            name="CoverTitle",
            fontName="Helvetica-Bold",
            fontSize=22,
            leading=26,
            textColor=colors.HexColor("#0f172a"),
            alignment=TA_CENTER,
            spaceAfter=12
        ))
        self.styles.add(ParagraphStyle(
            name="CoverSubtitle",
            fontName="Helvetica-Bold",
            fontSize=13,
            leading=17,
            textColor=colors.HexColor("#0284c7"),
            alignment=TA_CENTER,
            spaceAfter=20
        ))
        self.styles.add(ParagraphStyle(
            name="CoverMeta",
            fontName="Helvetica",
            fontSize=9.5,
            leading=14,
            textColor=colors.HexColor("#334155"),
            alignment=TA_CENTER
        ))
        self.styles.add(ParagraphStyle(
            name="DocH1",
            fontName="Helvetica-Bold",
            fontSize=14,
            leading=18,
            textColor=colors.HexColor("#0369a1"),
            spaceBefore=14,
            spaceAfter=6,
            keepWithNext=True
        ))
        self.styles.add(ParagraphStyle(
            name="DocH2",
            fontName="Helvetica-Bold",
            fontSize=11,
            leading=15,
            textColor=colors.HexColor("#0f172a"),
            spaceBefore=10,
            spaceAfter=4,
            keepWithNext=True
        ))
        self.styles.add(ParagraphStyle(
            name="DocH3",
            fontName="Helvetica-Bold",
            fontSize=9.5,
            leading=13,
            textColor=colors.HexColor("#334155"),
            spaceBefore=8,
            spaceAfter=3,
            keepWithNext=True
        ))
        self.styles.add(ParagraphStyle(
            name="DocBody",
            fontName="Helvetica",
            fontSize=8.5,
            leading=12,
            textColor=colors.HexColor("#1e293b"),
            spaceAfter=5
        ))
        self.styles.add(ParagraphStyle(
            name="DocBullet",
            fontName="Helvetica",
            fontSize=8.5,
            leading=12,
            textColor=colors.HexColor("#1e293b"),
            leftIndent=14,
            firstLineIndent=-10,
            spaceAfter=3
        ))
        self.styles.add(ParagraphStyle(
            name="TableHeader",
            fontName="Helvetica-Bold",
            fontSize=7.5,
            leading=9.5,
            textColor=colors.white,
            alignment=TA_CENTER
        ))
        self.styles.add(ParagraphStyle(
            name="TableCell",
            fontName="Helvetica",
            fontSize=7.5,
            leading=9.5,
            textColor=colors.HexColor("#1e293b"),
            alignment=TA_LEFT
        ))
        self.styles.add(ParagraphStyle(
            name="TableCellCenter",
            fontName="Helvetica",
            fontSize=7.5,
            leading=9.5,
            textColor=colors.HexColor("#1e293b"),
            alignment=TA_CENTER
        ))
        self.styles.add(ParagraphStyle(
            name="CodeBlock",
            fontName="Courier",
            fontSize=7.5,
            leading=9.5,
            textColor=colors.HexColor("#0f172a"),
            leftIndent=10,
            spaceBefore=4,
            spaceAfter=4
        ))
        self.styles.add(ParagraphStyle(
            name="DocMath",
            fontName="Helvetica",
            fontSize=8.5,
            leading=12,
            textColor=colors.HexColor("#0369a1"),
            alignment=TA_LEFT
        ))

    def build_pdf_from_markdown(
        self,
        title: str,
        doc_type: str,
        markdown_text: str,
        metadata: Optional[Dict[str, Any]] = None
    ) -> bytes:
        """
        Converts clinical markdown text into a formatted PDF stream.
        """
        buf = io.BytesIO()
        doc = SimpleDocTemplate(
            buf,
            pagesize=letter,
            leftMargin=54,
            rightMargin=54,
            topMargin=54,
            bottomMargin=54
        )

        story = []
        meta = metadata or {}
        today = datetime.now().strftime("%B %d, %Y")

        # 1. Executive Cover Page
        story.append(Spacer(1, 40))
        story.append(Paragraph(meta.get("title", title), self.styles["CoverTitle"]))
        story.append(Paragraph(f"<b>{doc_type}</b>", self.styles["CoverSubtitle"]))
        story.append(HRFlowable(width="100%", thickness=1.5, color=colors.HexColor("#0284c7"), spaceAfter=25))

        cover_info = [
            ["Protocol / Document ID:", meta.get("protocol_id", "PRT-CLIN-001-V1.0")],
            ["Therapeutic Area:", meta.get("therapeutic_area", "General Medicine")],
            ["Clinical Study Phase:", meta.get("phase", "Phase 3")],
            ["Target Indication:", meta.get("indication", "Target Clinical Indication")],
            ["Study Design Archetype:", meta.get("design_type", "Randomized Controlled Trial")],
            ["Document Date:", today],
            ["Regulatory Standards:", "ICH GCP E6 (R2), ICH E9, 21 CFR Part 11, CDISC SDTM v1.8"],
            ["Confidentiality Statement:", "Proprietary & Confidential — For Regulatory Review Only"]
        ]
        t_cover_data = [
            [Paragraph(f"<b>{row[0]}</b>", self.styles["TableCell"]), Paragraph(str(row[1]), self.styles["TableCell"])]
            for row in cover_info
        ]
        t_cover = Table(t_cover_data, colWidths=[150, 354])
        t_cover.setStyle(TableStyle([
            ('BACKGROUND', (0,0), (-1,-1), colors.HexColor("#f8fafc")),
            ('BOTTOMPADDING', (0,0), (-1,-1), 6),
            ('TOPPADDING', (0,0), (-1,-1), 6),
            ('LEFTPADDING', (0,0), (-1,-1), 10),
            ('RIGHTPADDING', (0,0), (-1,-1), 10),
            ('GRID', (0,0), (-1,-1), 0.5, colors.HexColor("#e2e8f0")),
            ('ROWBACKGROUNDS', (0,0), (-1,-1), [colors.HexColor("#f8fafc"), colors.white])
        ]))
        story.append(t_cover)
        story.append(Spacer(1, 40))
        story.append(PageBreak())

        # 2. Parse Markdown Sections into Flowables
        lines = markdown_text.split("\n")
        i = 0
        in_code_block = False
        code_lines = []

        while i < len(lines):
            line = lines[i]

            # Code Block handling
            if line.strip().startswith("```"):
                if in_code_block:
                    in_code_block = False
                    code_text = "<br/>".join([c.replace(" ", "&nbsp;").replace("<", "&lt;").replace(">", "&gt;") for c in code_lines])
                    t_box = Table([[Paragraph(code_text, self.styles["CodeBlock"])]], colWidths=[504])
                    t_box.setStyle(TableStyle([
                        ('BACKGROUND', (0,0), (-1,-1), colors.HexColor("#f1f5f9")),
                        ('BOX', (0,0), (-1,-1), 0.5, colors.HexColor("#cbd5e1")),
                        ('TOPPADDING', (0,0), (-1,-1), 6),
                        ('BOTTOMPADDING', (0,0), (-1,-1), 6),
                    ]))
                    story.append(t_box)
                    story.append(Spacer(1, 4))
                    code_lines = []
                else:
                    in_code_block = True
                    code_lines = []
                i += 1
                continue

            if in_code_block:
                code_lines.append(line)
                i += 1
                continue

            # Markdown Table handling
            if line.strip().startswith("|") and line.strip().endswith("|"):
                table_lines = []
                while i < len(lines) and lines[i].strip().startswith("|") and lines[i].strip().endswith("|"):
                    table_lines.append(lines[i].strip())
                    i += 1
                t_flowable = self._parse_markdown_table_to_flowable(table_lines)
                if t_flowable:
                    story.append(Spacer(1, 4))
                    story.append(t_flowable)
                    story.append(Spacer(1, 6))
                continue

            # Math display block handling ($$ ... $$)
            if line.strip().startswith("$$"):
                math_lines = []
                if line.strip() == "$$":
                    i += 1
                    while i < len(lines) and lines[i].strip() != "$$":
                        math_lines.append(lines[i].strip())
                        i += 1
                else:
                    math_content = line.strip().strip("$").strip()
                    if math_content:
                        math_lines.append(math_content)

                raw_latex = " ".join(math_lines)
                clean_math = self._format_latex_for_pdf(raw_latex)
                t_math = Table([[Paragraph(f"<b>Statistical Formula:</b>&nbsp;&nbsp;{clean_math}", self.styles["DocMath"])]], colWidths=[504])
                t_math.setStyle(TableStyle([
                    ('BACKGROUND', (0,0), (-1,-1), colors.HexColor("#f0f9ff")),
                    ('BOX', (0,0), (-1,-1), 1, colors.HexColor("#0284c7")),
                    ('TOPPADDING', (0,0), (-1,-1), 6),
                    ('BOTTOMPADDING', (0,0), (-1,-1), 6),
                    ('LEFTPADDING', (0,0), (-1,-1), 10),
                    ('RIGHTPADDING', (0,0), (-1,-1), 10),
                ]))
                story.append(Spacer(1, 4))
                story.append(t_math)
                story.append(Spacer(1, 6))
                i += 1
                continue

            # Headers
            if line.startswith("# "):
                text = line[2:].strip()
                story.append(Spacer(1, 8))
                story.append(Paragraph(self._clean_inline_markdown(text), self.styles["DocH1"]))
                story.append(HRFlowable(width="100%", thickness=1, color=colors.HexColor("#0284c7"), spaceAfter=8))
            elif line.startswith("## "):
                text = line[3:].strip()
                story.append(Spacer(1, 6))
                story.append(Paragraph(self._clean_inline_markdown(text), self.styles["DocH2"]))
            elif line.startswith("### "):
                text = line[4:].strip()
                story.append(Paragraph(self._clean_inline_markdown(text), self.styles["DocH3"]))
            elif line.startswith("---"):
                story.append(HRFlowable(width="100%", thickness=0.5, color=colors.HexColor("#cbd5e1"), spaceBefore=6, spaceAfter=6))
            elif line.startswith("- ") or line.startswith("* "):
                text = line[2:].strip()
                story.append(Paragraph(f"&bull;&nbsp;&nbsp;{self._clean_inline_markdown(text)}", self.styles["DocBullet"]))
            elif re.match(r"^\d+\.\s", line):
                m = re.match(r"^(\d+\.)\s*(.*)", line)
                num = m.group(1)
                text = m.group(2)
                story.append(Paragraph(f"<b>{num}</b>&nbsp;&nbsp;{self._clean_inline_markdown(text)}", self.styles["DocBullet"]))
            elif line.strip():
                story.append(Paragraph(self._clean_inline_markdown(line.strip()), self.styles["DocBody"]))
            else:
                story.append(Spacer(1, 2))

            i += 1

        def make_canvas(*args, **kwargs):
            c = NumberedCanvas(*args, **kwargs)
            c.doc_title = meta.get("protocol_id", title[:45])
            return c

        doc.build(story, canvasmaker=make_canvas)
        return buf.getvalue()

    def _parse_markdown_table_to_flowable(self, table_lines: List[str]) -> Optional[Table]:
        """Converts raw markdown table lines into a ReportLab Table Flowable."""
        if len(table_lines) < 2:
            return None

        # Filter out separator row (|:---|:---|)
        rows_raw = []
        for line in table_lines:
            cells = [c.strip() for c in line.strip("|").split("|")]
            if all(set(c).issubset({"-", ":", " "}) for c in cells):
                continue  # separator
            rows_raw.append(cells)

        if not rows_raw:
            return None

        num_cols = max(len(r) for r in rows_raw)
        # Normalize column counts
        for r in rows_raw:
            while len(r) < num_cols:
                r.append("")

        total_width = 504.0  # letter width 612 - 2*54 margin
        # Smart column width allocation
        if num_cols == 2:
            col_widths = [154.0, 350.0]
        elif num_cols == 4:
            col_widths = [140.0, 60.0, 154.0, 150.0]
        else:
            first_col_w = max(110.0, total_width * 0.28)
            rem_w = (total_width - first_col_w) / max(1, num_cols - 1)
            col_widths = [first_col_w] + [rem_w] * (num_cols - 1)

        table_data = []
        for row_idx, row in enumerate(rows_raw):
            row_cells = []
            is_header = (row_idx == 0)
            for c_idx, cell in enumerate(row):
                cell_clean = self._clean_inline_markdown(cell)
                if is_header:
                    p = Paragraph(f"<b>{cell_clean}</b>", self.styles["TableHeader"])
                else:
                    align_center = (c_idx > 0 and (cell.strip() == "X" or len(cell) <= 6))
                    style_to_use = self.styles["TableCellCenter"] if align_center else self.styles["TableCell"]
                    p = Paragraph(cell_clean, style_to_use)
                row_cells.append(p)
            table_data.append(row_cells)

        t = Table(table_data, colWidths=col_widths, repeatRows=1)
        t.setStyle(TableStyle([
            ('BACKGROUND', (0,0), (-1,0), colors.HexColor("#0284c7")),
            ('ALIGN', (0,0), (-1,-1), 'LEFT'),
            ('VALIGN', (0,0), (-1,-1), 'MIDDLE'),
            ('TOPPADDING', (0,0), (-1,-1), 4),
            ('BOTTOMPADDING', (0,0), (-1,-1), 4),
            ('LEFTPADDING', (0,0), (-1,-1), 4),
            ('RIGHTPADDING', (0,0), (-1,-1), 4),
            ('GRID', (0,0), (-1,-1), 0.5, colors.HexColor("#cbd5e1")),
            ('ROWBACKGROUNDS', (0,1), (-1,-1), [colors.white, colors.HexColor("#f8fafc")]),
        ]))
        return t

    def _clean_inline_markdown(self, text: str) -> str:
        """Converts basic markdown formatting (*bold*, _italic_, `code`, math) into ReportLab XML."""
        # Bold **text** -> <b>text</b>
        text = re.sub(r"\*\*(.*?)\*\*", r"<b>\1</b>", text)
        # Italic *text* -> <i>text</i>
        text = re.sub(r"\*(.*?)\*", r"<i>\1</i>", text)
        # Inline code `text` -> <font name='Courier'>text</font>
        text = re.sub(r"`(.*?)`", r"<font name='Courier'><b>\1</b></font>", text)
        # Simple math symbols cleanup ($...$)
        text = text.replace("$\\alpha$", "&alpha;")
        text = text.replace("$\\beta$", "&beta;")
        text = text.replace("$\\Delta$", "&Delta;")
        text = text.replace("$\\theta$", "&theta;")
        text = text.replace("$\\ge$", "&ge;")
        text = text.replace("$\\le$", "&le;")
        text = text.replace("$", "")
        return text

    @staticmethod
    def _format_latex_for_pdf(text: str) -> str:
        """Converts LaTeX mathematical markup into clean ReportLab HTML/Unicode."""
        while r"\frac{" in text:
            idx = text.find(r"\frac{")
            depth = 0
            num_end = -1
            for i in range(idx + 6, len(text)):
                if text[i] == '{': depth += 1
                elif text[i] == '}':
                    if depth == 0:
                        num_end = i
                        break
                    depth -= 1
            if num_end == -1 or num_end + 1 >= len(text) or text[num_end + 1] != '{':
                break
            depth = 0
            den_end = -1
            for i in range(num_end + 2, len(text)):
                if text[i] == '{': depth += 1
                elif text[i] == '}':
                    if depth == 0:
                        den_end = i
                        break
                    depth -= 1
            if den_end == -1: break
            num = text[idx+6:num_end]
            den = text[num_end+2:den_end]
            text = text[:idx] + f"({num}) / ({den})" + text[den_end+1:]

        text = re.sub(r"\\sqrt\{([^}]+)\}", r"√(\1)", text)
        text = re.sub(r"\\text\{([^}]+)\}", r"\1", text)
        text = text.replace(r"\cdot", " · ")
        text = text.replace(r"\alpha", "α")
        text = text.replace(r"\beta", "β")
        text = text.replace(r"\sigma", "σ")
        text = text.replace(r"\Delta", "Δ")
        text = text.replace(r"\delta", "δ")
        text = text.replace(r"\bar{p}", "p̄")
        text = text.replace(r"\ln", "ln")
        text = text.replace(r"\left(", "(").replace(r"\right)", ")")
        text = text.replace(r"\left[", "[").replace(r"\right]", "]")
        text = text.replace(r"\quad", "&nbsp;&nbsp;&nbsp;&nbsp;")
        text = text.replace(r"\le", "≤").replace(r"\ge", "≥")
        text = re.sub(r"_\{?([a-zA-Z0-9αβ/]+)\}?", r"<sub>\1</sub>", text)
        text = re.sub(r"\^\{?([a-zA-Z0-9]+)\}?", r"<sup>\1</sup>", text)
        return text.strip()

