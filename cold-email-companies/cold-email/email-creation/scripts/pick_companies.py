"""Pick leads from the tech workbook and print them as JSON.

Examples:
  python pick_companies.py --tab "Best for you" --limit 5 --with-email
  python pick_companies.py --tab Remote --region India --entry-only
  python pick_companies.py --company "Aktoria Robotics" --company "Infinity"
"""
import argparse, json, sys
from pathlib import Path
import openpyxl

DEFAULT_WB = Path(__file__).resolve().parents[3] / "company-list" / "only-tech" / "Tech_Company_Leads_Compiled.xlsx"
LINK_COLS = {"Find link", "Website", "Job link"}
HEADER_ROW = 4


def read_tab(ws):
    headers = [c.value for c in ws[HEADER_ROW]]
    out = []
    for row in ws.iter_rows(min_row=HEADER_ROW + 1):
        if not row[0].value:
            continue
        rec = {}
        for h, c in zip(headers, row):
            if not h:
                continue
            if h in LINK_COLS:
                rec[h] = c.hyperlink.target if c.hyperlink else (c.value or "")
            else:
                rec[h] = c.value if c.value is not None else ""
        rec["Tab"] = ws.title
        out.append(rec)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--workbook", default=str(DEFAULT_WB))
    ap.add_argument("--tab", default="Best for you",
                    help="Best for you | Remote | Hybrid-Flex | On-site | H-1B Sponsors | all")
    ap.add_argument("--company", action="append", help="exact company name (case-insensitive); repeatable")
    ap.add_argument("--region", help="e.g. India, Global, US, Europe")
    ap.add_argument("--entry-only", action="store_true", help="only rows with Entry-level? = Yes")
    ap.add_argument("--with-email", action="store_true", help="only rows with a Contact email")
    ap.add_argument("--include-drafted", action="store_true", help="include rows already marked Drafted")
    ap.add_argument("--limit", type=int, default=5)
    a = ap.parse_args()

    wb = openpyxl.load_workbook(a.workbook)
    tabs = [s for s in wb.sheetnames if s != "Read Me"] if (a.company or a.tab == "all") else [a.tab]
    rows, seen = [], set()
    for t in tabs:
        if t not in wb.sheetnames:
            sys.exit(f"No tab named {t!r}. Tabs: {wb.sheetnames}")
        for r in read_tab(wb[t]):
            key = (str(r["Company"]).lower(), str(r["Role (example)"]).lower())
            if key in seen:
                continue
            seen.add(key)
            rows.append(r)

    if a.company:
        wanted = {c.lower().strip() for c in a.company}
        rows = [r for r in rows if str(r["Company"]).lower().strip() in wanted]
    if a.region:
        rows = [r for r in rows if str(r.get("Region", "")).lower() == a.region.lower()]
    if a.entry_only:
        rows = [r for r in rows if r.get("Entry-level?") == "Yes"]
    if a.with_email:
        rows = [r for r in rows if r.get("Contact email")]
    if not a.include_drafted:
        rows = [r for r in rows if not str(r.get("Email status", "")).startswith("Drafted")]

    sys.stdout.reconfigure(encoding="utf-8")
    print(json.dumps(rows[: a.limit] if not a.company else rows, indent=2, ensure_ascii=False, default=str))
    print(f"# {len(rows)} matching rows (showing {min(len(rows), a.limit) if not a.company else len(rows)})", file=sys.stderr)


if __name__ == "__main__":
    main()
