"""Writes each lead's links from the workbook as `Company<TAB>URL` lines.

Startup Scout reads the file with `cargo scout import-boards <file>` and follows every careers
board it recognises (YC company pages, Ashby, Greenhouse, Lever).

Usage: python export_job_links.py [workbook.xlsx] [out.tsv]
"""

import sys
from pathlib import Path

import openpyxl

HERE = Path(__file__).resolve().parent
DEFAULT_WB = HERE / "Tech_Company_Leads_Compiled.xlsx"
DEFAULT_OUT = HERE / "job_links.tsv"
HEADER_ROW = 4
LINK_COLUMNS = ("Website", "Job link")
SKIPPED_TABS = {"Read Me"}


def link_of(cell):
    """The URL behind a link cell: its hyperlink, or the value when it is a bare URL."""
    if cell.hyperlink and cell.hyperlink.target:
        return cell.hyperlink.target
    if isinstance(cell.value, str) and cell.value.startswith("http"):
        return cell.value
    return None


def main():
    workbook = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_WB
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else DEFAULT_OUT
    # Normal mode: read-only mode cuts some of these sheets short.
    wb = openpyxl.load_workbook(workbook)
    lines = set()
    for ws in wb.worksheets:
        if ws.title in SKIPPED_TABS:
            continue
        header = [cell.value for cell in ws[HEADER_ROW]]
        columns = [header.index(name) for name in LINK_COLUMNS if name in header]
        for row in ws.iter_rows(min_row=HEADER_ROW + 1):
            company = row[0].value
            if not company:
                continue
            for index in columns:
                url = link_of(row[index])
                if url:
                    lines.add(f"{str(company).strip()}\t{url.strip()}")
    out.write_text("\n".join(sorted(lines)) + "\n", encoding="utf-8")
    print(f"{len(lines)} links written to {out}")


if __name__ == "__main__":
    main()
