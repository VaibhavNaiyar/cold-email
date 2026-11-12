"""Save a cold email draft and mark the company as Drafted in the workbook.

  python save_email.py --company "Acme" --role "Founding Engineer" --to "jane@acme.com" \
      --to-name "Jane Doe" --subject "found a bug in your importer" --body-file body.md \
      [--notes-file notes.md] [--followups-file followups.md] [--status "Drafted"]

Writes drafts/<YYYY-MM-DD>_<company>.md, appends drafts/drafts_log.csv, and sets the
"Email status" cell for that company in every tab of the workbook.
"""
import argparse, csv, datetime, re, sys
from pathlib import Path
import openpyxl

HERE = Path(__file__).resolve().parent
SKILL = HERE.parent
DRAFTS = SKILL / "drafts"
DEFAULT_WB = SKILL.parents[1] / "company-list" / "only-tech" / "Tech_Company_Leads_Compiled.xlsx"
HEADER_ROW = 4


def slug(s):
    return re.sub(r"[^A-Za-z0-9]+", "-", s).strip("-")[:60] or "company"


def read(p):
    return Path(p).read_text(encoding="utf-8").strip() if p else ""


def mark_workbook(wb_path, company, status):
    try:
        wb = openpyxl.load_workbook(wb_path)
    except Exception as e:
        return f"could not open workbook: {e}"
    hits = 0
    for ws in wb.worksheets:
        if ws.title == "Read Me":
            continue
        headers = [c.value for c in ws[HEADER_ROW]]
        if "Email status" not in headers:
            continue
        col = headers.index("Email status") + 1
        for r in range(HEADER_ROW + 1, ws.max_row + 1):
            if str(ws.cell(r, 1).value or "").strip().lower() == company.strip().lower():
                ws.cell(r, col).value = status
                hits += 1
    if not hits:
        return f"company {company!r} not found in workbook (draft still saved)"
    try:
        wb.save(wb_path)
    except PermissionError:
        return "workbook is open in Excel - close it and re-run to mark the status (draft still saved)"
    return f"marked {hits} row(s) as {status!r}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--company", required=True)
    ap.add_argument("--role", default="")
    ap.add_argument("--to", default="", help="recipient email; add ' (unverified)' if guessed")
    ap.add_argument("--to-name", default="")
    ap.add_argument("--subject", required=True)
    ap.add_argument("--body-file", required=True)
    ap.add_argument("--notes-file")
    ap.add_argument("--followups-file")
    ap.add_argument("--status", default="Drafted")
    ap.add_argument("--workbook", default=str(DEFAULT_WB))
    a = ap.parse_args()

    today = datetime.date.today().isoformat()
    DRAFTS.mkdir(exist_ok=True)
    body, notes, fups = read(a.body_file), read(a.notes_file), read(a.followups_file)
    path = DRAFTS / f"{today}_{slug(a.company)}.md"

    md = [f"# {a.company} — {a.role}".rstrip(" —"), "",
          f"**To:** {a.to_name} <{a.to}>" if a.to_name else f"**To:** {a.to}",
          f"**Subject:** {a.subject}", "", "---", "", body, "", "---", ""]
    if fups:
        md += ["## Follow-ups", "", fups, ""]
    if notes:
        md += ["## Research notes", "", notes, ""]
    path.write_text("\n".join(md), encoding="utf-8")

    log = DRAFTS / "drafts_log.csv"
    new = not log.exists()
    with log.open("a", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        if new:
            w.writerow(["date", "company", "role", "to_name", "to_email", "subject", "body", "draft_file", "status"])
        w.writerow([today, a.company, a.role, a.to_name, a.to, a.subject, body, path.name, a.status])

    msg = mark_workbook(a.workbook, a.company, f"{a.status} {today}")
    sys.stdout.reconfigure(encoding="utf-8")
    print(f"saved {path}")
    print(f"logged {log}")
    print(msg)


if __name__ == "__main__":
    main()
