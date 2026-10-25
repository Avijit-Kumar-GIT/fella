#!/usr/bin/env python3
"""Regenerate messy_ledger.xlsx — a deliberately human-messy spreadsheet:

  row 1   a title banner in A1 only
  row 2   a blank spacer
  row 3   the real header, with a space and a "$" in one name
  rows 4-8  data; amounts written as text ("$1,200.00", "1,150") mixed with a
            bare number and one "N/A" placeholder
  row 9   a trailing "Total" summary row that must not be counted

Stdlib only (zipfile + hand-written OOXML); no openpyxl dependency. Kept in the
repo so the binary fixture is reproducible.
"""
import zipfile
from pathlib import Path

ROWS = [
    ["Rent Ledger 2024"],
    [],
    ["Date", "Amount Paid ($)", "Method"],
    ["2024-01-01", "$1,200.00", "ACH"],
    ["2024-02-01", "1,150", "ACH"],
    ["2024-03-01", 1200, "check"],
    ["2024-04-01", "N/A", "ACH"],
    ["2024-05-01", "$1,250.00", "check"],
    ["Total", "$4,800.00", ""],
]

def col_ref(i: int) -> str:
    return chr(ord("A") + i)

def cell_xml(row_idx: int, col_idx: int, value) -> str:
    ref = f"{col_ref(col_idx)}{row_idx + 1}"
    if value == "" or value is None:
        return ""
    if isinstance(value, (int, float)):
        return f'<c r="{ref}"><v>{value}</v></c>'
    esc = (str(value).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))
    return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{esc}</t></is></c>'

def row_xml(row_idx: int, cells) -> str:
    inner = "".join(cell_xml(row_idx, c, v) for c, v in enumerate(cells))
    return f'<row r="{row_idx + 1}">{inner}</row>'

sheet = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
    '<dimension ref="A1:C9"/>'
    "<sheetData>"
    + "".join(row_xml(i, r) for i, r in enumerate(ROWS))
    + "</sheetData></worksheet>"
)

workbook = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
    '<sheets><sheet name="Ledger" sheetId="1" r:id="rId1"/></sheets></workbook>'
)

workbook_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" '
    'Target="worksheets/sheet1.xml"/></Relationships>'
)

root_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="xl/workbook.xml"/></Relationships>'
)

content_types = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/xl/workbook.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
    '<Override PartName="/xl/worksheets/sheet1.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
    "</Types>"
)

out = Path(__file__).with_name("messy_ledger.xlsx")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", content_types)
    z.writestr("_rels/.rels", root_rels)
    z.writestr("xl/workbook.xml", workbook)
    z.writestr("xl/_rels/workbook.xml.rels", workbook_rels)
    z.writestr("xl/worksheets/sheet1.xml", sheet)
print(f"wrote {out}")
