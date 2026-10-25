#!/usr/bin/env python3
"""Regenerate empty_sheet.xlsx — a workbook whose only sheet has zero rows.
Exercises the "empty sheet" skip path in ingest_workbook (a whole workbook
with no usable sheet must surface a specific reason, not a bare "no readable
sheets").

(Despite the filename, this ended up testing the empty-sheet path rather
than "header, no data rows below it": header_row()'s main loop only ever
picks a header when there's at least one row *after* it to look ahead at, and
its single-row fallback requires rows.len() > 1 too so with the current
implementation a non-empty `rows` slice can never come out of header_row()
with an empty data remainder. That specific skip reason is defensive/
unreachable today, not a bug worth chasing.)

Stdlib only (zipfile + hand-written OOXML); no openpyxl dependency. Kept in
the repo so the binary fixture is reproducible (same approach as
make_messy_ledger.py).
"""
import zipfile
from pathlib import Path

sheet = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
    "<sheetData/></worksheet>"
)

workbook = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
    '<sheets><sheet name="Empty" sheetId="1" r:id="rId1"/></sheets></workbook>'
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

out = Path(__file__).with_name("empty_sheet.xlsx")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", content_types)
    z.writestr("_rels/.rels", root_rels)
    z.writestr("xl/workbook.xml", workbook)
    z.writestr("xl/_rels/workbook.xml.rels", workbook_rels)
    z.writestr("xl/worksheets/sheet1.xml", sheet)
print(f"wrote {out}")
