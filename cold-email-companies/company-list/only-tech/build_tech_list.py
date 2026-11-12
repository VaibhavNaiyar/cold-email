"""Build the tech-only company workbook in the same format as Rachit_Job_Leads_Compiled.xlsx."""
import re, sys, os, urllib.parse, datetime
import openpyxl
from openpyxl.styles import Font, PatternFill, Alignment, Border, Side
from openpyxl.utils import get_column_letter
from collections import Counter

sys.stdout.reconfigure(encoding="utf-8")
SRC = sys.argv[1]
OUT = sys.argv[2]

def P(name): return os.path.join(SRC, name)

def link_of(cell):
    """Return (label, url) from a HYPERLINK formula or a cell hyperlink."""
    v = cell.value
    if isinstance(v, str) and v.startswith("=HYPERLINK("):
        m = re.match(r'=HYPERLINK\("([^"]*)"\s*,\s*"([^"]*)"\)', v)
        if m: return m.group(2), m.group(1)
    if cell.hyperlink and cell.hyperlink.target:
        return (v or ""), cell.hyperlink.target
    if isinstance(v, str) and v.startswith("http"):
        return v, v
    return (v or ""), ""

def clean(s):
    if s is None: return ""
    s = str(s).strip()
    # fix common mojibake
    try:
        if "â\x80" in s: s = s.encode("latin-1").decode("utf-8")
    except Exception: pass
    s = re.sub(r"\(\s*\)", "", s)
    return re.sub(r"\s+", " ", s).strip()

def norm(name):
    n = clean(name).lower()
    n = re.sub(r"\b(inc|llc|ltd|corp|corporation|co|gmbh|pvt|private|limited|technologies|technology|labs|ai|hq)\b\.?", "", n)
    return re.sub(r"[^a-z0-9]", "", n)

# ------------------------------------------------------------------ tech classifier
TECH = re.compile(r"""(engineer|engineering|developer|\bdev\b|software|\bswe\b|\bsde\b|programmer|coder|
    machine.?learning|\bml\b|\bai\b|\bllm|artificial.intelligence|data.scien|data.engineer|data.analy|analytics.engineer|
    devops|\bsre\b|site.reliability|infrastructure|\binfra\b|platform|back.?end|front.?end|full.?stack|
    mobile|\bios\b|android|\bqa\b|sdet|automation|security|cyber|cloud|architect|\bcto\b|technical.staff|\bmts\b|
    research.scien|researcher|applied.scien|kernel|compiler|firmware|embedded|web|game|blockchain|solidity|
    rust|python|golang|react|node|typescript|java\b|computer.vision|\bnlp\b|robotics.software|technical.lead|tech.lead)""", re.I | re.X)
NONTECH = re.compile(r"""(mechanical|electrical|hardware|manufacturing|civil|structural|chemical|process.engineer|
    quality.engineer|propulsion|avionics|\brf\b|optical|mechatronics|construction|field.service|
    sales.engineer|account.executive|\bsales\b|marketing|recruit|talent|counsel|legal|accountant|accounting|
    finance|financial|designer|customer.success|support.specialist|operations.(manager|lead|associate)|
    chief.of.staff|executive.assistant|bizops|business.develop|\bsdr\b|\bbdr\b|\bgtm\b|
    process.excellence|product.manager|program.manager|project.manager|hr\b|people.ops|protein|biolog|chemist|clinical|nurse|
    therapist|physician|lab.technician|scientist.\(ph\.?d|structur|thermal|\bmba\b|investment.bank|medical|
    change.control|cellular|power.electronics|turbomachinery|content.creator|social.media|videographer|copywriter)""", re.I | re.X)
TECH_STRONG = re.compile(r"(software|full.?stack|back.?end|front.?end|machine.?learning|\bml\b|devops|\bsre\b|data.engineer|"
                         r"platform.engineer|ai.engineer|infrastructure.engineer|security.engineer|engineering.manager)", re.I)

def is_tech_role(role):
    r = clean(role)
    if not r: return False
    if TECH_STRONG.search(r): return True
    if NONTECH.search(r): return False
    return bool(TECH.search(r))

# ------------------------------------------------------------------ location helpers
# word matches are case-insensitive via (?i:...); 2-letter codes stay case-sensitive so "in"/"or"/"de" don't match
INDIA = re.compile(r"(?i:india|bengaluru|bangalore|mumbai|delhi|noida|gurgaon|gurugram|pune|hyderabad|chennai|kolkata|ahmedabad|jaipur|kochi|indore|chandigarh)|(?:^|[,/(]\s*)IN(?=\s*(?:$|[,/)]))|\bIST\b")
GLOBAL = re.compile(r"(?i:worldwide|global|anywhere|everywhere|international)")
US = re.compile(r"\bUS\b|\bUSA\b|(?i:united states|san francisco|new york|nyc|seattle|austin|boston|chicago|los angeles|palo alto|bay area|mountain view|sunnyvale|san jose|santa clara|redmond|denver|atlanta|miami)|\b(CA|NY|TX|WA|MA|IL|CO|FL|GA|NC|PA|OR|VA|NJ|UT|AZ|MN|OH|MI|MD|DC)\b|\bSF\b|\bPST\b|\bEST\b|\bET\b|\bPT\b")
CANADA = re.compile(r"(?i:canada|toronto|vancouver|montreal|quebec|ontario)")
UK = re.compile(r"\bUK\b|\bGB\b|(?i:united kingdom|london|england|scotland)")
EUROPE = re.compile(r"(?i:europe|germany|berlin|munich|hamburg|france|paris|spain|poland|portugal|netherlands|amsterdam|sweden|denmark|ireland|dublin|italy|austria|switzerland|zurich|romania|croatia|hungary|ukraine|czech|estonia|lithuania|finland|norway|belgium)|\bEU\b|\bDE\b|\bFR\b|\bNL\b|\bCET\b|\bEMEA\b")
LATAM = re.compile(r"(?i:latam|latin america|brazil|mexico|argentina|colombia|chile)")
APAC = re.compile(r"(?i:singapore|australia|new zealand|japan|tokyo|apac|asia|dubai|uae|indonesia|philippines|vietnam|taipei|taiwan|korea)|\bAU\b")

def region_of(loc):
    l = clean(loc)
    if INDIA.search(l): return "India"
    if GLOBAL.search(l): return "Global"
    us, ca = bool(US.search(l)), bool(CANADA.search(l))
    if us and ca: return "US / Canada"
    if UK.search(l): return "UK"
    if ca: return "Canada"
    if EUROPE.search(l): return "Europe"
    if us: return "US"
    if LATAM.search(l): return "LATAM"
    if APAC.search(l): return "APAC"
    return ""

def work_type_of(loc, hint=""):
    l = clean(loc); h = clean(hint).lower()
    if "hybrid" in h: return "Hybrid / Flexible"
    if h == "remote": return "Remote"
    if not l or l.lower() in ("see company jobs", "not stated"): return ""
    if "remote" not in l.lower(): return "On-site"
    segs = [s.strip() for s in re.split(r"/|;| or |\|", l) if s.strip()]
    city_segs = [s for s in segs if "remote" not in s.lower() and len(s) > 3]
    return "Hybrid / Flexible" if city_segs else "Remote"

def entry_level(exp, role=""):
    e = clean(exp).lower(); r = clean(role).lower()
    if any(k in r for k in ("intern", "new grad", "junior", "entry", "graduate", "associate engineer")): return "Yes"
    if any(k in e for k in ("any", "new grad", "junior", "entry", "1+ year", "0", "intern", "1–3", "1-3", "1–5")): return "Yes"
    return ""

def find_link(company, role):
    q = f'{company} "{role}" careers apply' if role else f"{company} careers jobs"
    return "https://www.google.com/search?q=" + urllib.parse.quote(q)

# ------------------------------------------------------------------ contacts (enrichment)
contacts = {}   # norm(company) -> dict(email, name, src)
def add_contact(company, email="", name="", src="", website=""):
    k = norm(company)
    if not k: return
    c = contacts.setdefault(k, {"email": "", "name": "", "src": "", "website": ""})
    if email and not c["email"]: c["email"], c["src"] = email, src
    if name and not c["name"]: c["name"] = name
    if website and not c["website"]: c["website"] = website

# remote 3: MBA list (non-tech roles) — used only for founder names + public emails
ws = openpyxl.load_workbook(P("remote 3.xlsx"))["Leads"]
for row in ws.iter_rows(min_row=9):
    comp = clean(row[1].value)
    if not comp: continue
    em = clean(row[10].value)
    if em.lower().startswith("not publicly"): em = ""
    add_contact(comp, em, clean(row[14].value), "Backdoor MBA list (public business email)", link_of(row[16])[1])

# ------------------------------------------------------------------ rows
COLS = ["Company", "Role (example)", "Industry", "Location", "Work type", "Experience", "Entry-level?", "Region",
        "Posted / age", "Pay", "Open roles", "Source", "Find link", "Notes",
        # extra columns for cold emailing
        "Website", "Job link", "Contact email", "Contact name", "Visa sponsorship", "Email status"]
rows = []
stats = Counter()

def add(**kw):
    r = {c: "" for c in COLS}
    r.update(kw)
    r["Region"] = r["Region"] or region_of(r["Location"])
    rows.append(r)

# remote 8 (superset of remote 9): 3,000 startup leads
ws = openpyxl.load_workbook(P("remote 8.xlsx")).active
for row in ws.iter_rows(min_row=9):
    comp = clean(row[1].value)
    if not comp or not isinstance(row[0].value, (int, float)): continue
    role = clean(row[3].value)
    stats["r8_total"] += 1
    if role.endswith("listed on company job board"):
        stats["r8_no_role"] += 1; continue
    if not is_tech_role(role):
        stats["r8_nontech"] += 1; continue
    job = link_of(row[8])[1]; alljobs = link_of(row[9])[1]; site = link_of(row[10])[1]
    src = "textbackdoor.com - YC startups" if "ycombinator" in (job + alljobs) else "textbackdoor.com - GreenRobot directory"
    loc = clean(row[4].value)
    exp = clean(row[5].value)
    add(Company=comp, **{"Role (example)": role}, Industry=clean(row[2].value).replace("Not stated", ""),
        Location=loc, **{"Work type": work_type_of(loc)}, Experience=exp, **{"Entry-level?": entry_level(exp, role)},
        **{"Posted / age": clean(row[6].value).replace(" (3 Nov snapshot)", "")}, **{"Open roles": int(row[7].value) if row[7].value else ""},
        Source=src, Notes="One company per row; shown role is an example - company may list other openings.",
        Website=site, **{"Job link": job or alljobs})
    stats["r8_tech"] += 1

# remote-1: 300 remote hiring contacts (HN who's hiring)
ws = openpyxl.load_workbook(P("remote-1.xlsx"))["Leads"]
for row in ws.iter_rows(min_row=6):
    comp = clean(row[0].value)
    if not comp: continue
    stats["r1_total"] += 1
    kw = clean(row[5].value); head = clean(row[2].value)
    # skip job-seeker posts and rows where the "company" is really a location/profile
    if (re.search(r"seeking|willing to relocate|^location:", head, re.I) or comp.startswith("(")
            or re.search(r"linkedin\.com|framer\.website", comp, re.I) or re.search(r", [A-Z]{2}$", comp)):
        stats["r1_seeker"] += 1; continue
    segs = [s.strip() for s in re.split(r"\||\s[—–-]\s", head)]
    role = next((s for s in segs[1:] if is_tech_role(s) and not re.search(r"https?://|www\.|^\S+\.(ai|io|com|dev|co|xyz|tech|app)", s, re.I)), "")
    if not role and not re.search(r"software|ai/ml|data", kw, re.I):
        stats["r1_nontech"] += 1; continue
    if not role:
        role = "Engineering roles - see hiring post (" + kw + ")"
    site = link_of(row[6])[1]; post = link_of(row[10])[1]
    email = clean(row[1].value)
    add(Company=comp, **{"Role (example)": role[:120]}, Industry=kw, Location=clean(row[3].value),
        **{"Work type": "Hybrid / Flexible" if "hybrid" in clean(row[4].value).lower() else "Remote"},
        Experience="", **{"Entry-level?": entry_level("", role)}, **{"Posted / age": clean(row[9].value)},
        Pay=clean(row[8].value), Source="textbackdoor.com - HN Who's Hiring (remote contacts)",
        Notes="Published hiring post with a public hiring email. Check location restrictions before applying.",
        Website=site, **{"Job link": post}, **{"Contact email": email}, **{"Contact name": ""})
    add_contact(comp, email, "", "HN hiring post", site)
    stats["r1_tech"] += 1

# remote 2 / 4 / 5: visa-sponsorship lists (US / Canada / UK)
for fname, country in (("remote 2.xlsx", "US"), ("remote 4.xlsx", "Canada"), ("remote 5.xlsx", "UK")):
    ws = openpyxl.load_workbook(P(fname)).active
    for row in ws.iter_rows(min_row=10):
        comp = clean(row[0].value)
        if not comp or len(row) < 9 or not row[1].value: continue
        if comp.lower().startswith(("looking", "to get", "text")): continue
        stats["visa_total"] += 1
        role = clean(row[1].value)
        if not is_tech_role(role):
            stats["visa_nontech"] += 1; continue
        loc = clean(row[2].value)
        add(Company=comp, **{"Role (example)": role}, Industry="", Location=loc,
            **{"Work type": work_type_of(loc) or "On-site"}, Experience=clean(row[3].value),
            **{"Entry-level?": entry_level(row[3].value, role)}, Region=country, **{"Posted / age": "Checked " + clean(row[7].value)},
            Source=f"textbackdoor.com - {country} visa sponsorship list", Notes=clean(row[5].value),
            **{"Job link": link_of(row[6])[1]}, **{"Visa sponsorship": clean(row[4].value)})
        stats["visa_tech"] += 1

# remote 6 (== remote 7): 400 H-1B employers
h1b = []
ws = openpyxl.load_workbook(P("remote 6.xlsx")).active
for row in ws.iter_rows(min_row=10):
    if not isinstance(row[0].value, (int, float)): continue
    comp = clean(row[1].value); role = clean(row[3].value); sector = clean(row[2].value)
    stats["h1b_total"] += 1
    if not (is_tech_role(role) or (sector == "Information & software" and not NONTECH.search(role))):
        stats["h1b_nontech"] += 1; continue
    loc = clean(row[4].value)
    r = {c: "" for c in COLS}
    r.update({"Company": comp, "Role (example)": role, "Industry": sector, "Location": loc, "Work type": "On-site",
              "Experience": f"{int(row[7].value or 0):,} Level I/II LCAs", "Entry-level?": "Yes" if (row[7].value or 0) > 0 else "",
              "Region": "US", "Posted / age": row[9].value.strftime("%d %b %Y") if isinstance(row[9].value, datetime.datetime) else clean(row[9].value),
              "Pay": f"Wage level {clean(row[8].value)}", "Open roles": int(row[6].value or 0),
              "Source": "textbackdoor.com - H-1B LCA filings (DOL)",
              "Notes": f"{int(row[5].value or 0):,} new-employment LCAs filed. Large employer - apply via careers site / referrals.",
              "Job link": link_of(row[12])[1], "Visa sponsorship": "Verified H-1B filer"})
    h1b.append(r); stats["h1b_tech"] += 1

# ------------------------------------------------------------------ enrich + dedupe
for r in rows + h1b:
    c = contacts.get(norm(r["Company"]))
    if c:
        r["Contact email"] = r["Contact email"] or c["email"]
        r["Contact name"] = r["Contact name"] or c["name"]
        r["Website"] = r["Website"] or c["website"]
    r["Find link"] = find_link(r["Company"], r["Role (example)"])

seen = set(); uniq = []
for r in rows:
    k = (norm(r["Company"]), r["Role (example)"].lower())
    if k in seen: stats["dupes"] += 1; continue
    seen.add(k); uniq.append(r)
rows = uniq

EXP_ORDER = {"Yes": 0}
def sort_key(r):
    return (0 if r["Entry-level?"] == "Yes" else 1, 0 if r["Contact email"] else 1, r["Company"].lower())

best = [r for r in rows if r["Region"] == "India" or (r["Work type"] == "Remote" and r["Region"] == "Global")]
remote = [r for r in rows if r["Work type"] == "Remote"]
hybrid = [r for r in rows if r["Work type"] == "Hybrid / Flexible"]
onsite = [r for r in rows if r["Work type"] in ("On-site", "")]
for lst in (best, remote, hybrid, onsite, h1b): lst.sort(key=sort_key)
h1b.sort(key=lambda r: -(r["Open roles"] or 0))

# ------------------------------------------------------------------ write (same styling as reference)
NAVY = "1F3A5F"; LINK = "2E6FB0"; BAND = "F2F6FB"
thin = Side(style="thin", color="D0D7E2")
wb = openpyxl.Workbook()
rm = wb.active; rm.title = "Read Me"
today = datetime.date.today().strftime("%d %b %Y")
lines = [
    ("Tech Company Leads - compiled for cold emailing", "title"),
    (f"Compiled {today} from the Backdoor (textbackdoor.com) lead lists in company-list/refrence-total.", "sub"),
    ("", ""),
    ("WHAT THIS IS", "h"),
    ("All 9 source workbooks merged into one tech-only workbook, in the same layout as Rachit_Job_Leads_Compiled.xlsx.", ""),
    ("Kept only software / tech roles: software, full-stack, backend, frontend, mobile, AI/ML, data, platform, infra,", ""),
    ("DevOps/SRE, security, QA, research engineering, founding/forward-deployed engineers. Removed sales, marketing, PM,", ""),
    ("design, ops, finance, legal, support, biotech and hardware/mechanical/electrical/manufacturing roles.", ""),
    ("", ""),
    ("TABS", "h"),
    (f"Best for you    - India-based + worldwide-remote tech roles ({len(best)} rows). Start here.", ""),
    (f"Remote          - all remote tech roles ({len(remote)} rows).", ""),
    (f"Hybrid-Flex     - city + remote option ({len(hybrid)} rows).", ""),
    (f"On-site         - office-based tech roles, mostly US ({len(onsite)} rows).", ""),
    (f"H-1B Sponsors   - large US employers with verified H-1B filings for tech roles ({len(h1b)} rows).", ""),
    ("", ""),
    ("COLUMNS", "h"),
    ("Columns A-N match the reference workbook exactly. Columns O-T are added for cold emailing:", ""),
    ("Website / Job link - real URLs recovered from the source sheets (the reference only had a Google search link).", ""),
    ("Contact email / Contact name - public hiring or founder emails found in the sources (HN hiring posts, MBA list).", ""),
    ("Visa sponsorship - signal from the US/UK/Canada sponsorship lists and H-1B data.", ""),
    ("Email status - leave blank; the email-creation skill writes 'Drafted' here when it writes an email.", ""),
    ("", ""),
    ("WHAT WAS LEFT OUT", "h"),
    (f"{stats['r8_no_role']:,} GreenRobot directory companies had only a role COUNT and no role title, so they could not be", ""),
    ("confirmed as tech and were excluded. remote 3.xlsx (MBA business roles) was used only to enrich contacts.", ""),
    ("remote 7.xlsx is an exact duplicate of remote 6.xlsx; remote 9.xlsx is the first 1,000 rows of remote 8.xlsx.", ""),
    ("", ""),
    ("HONEST NOTE", "h"),
    ("Most startup rows are US-based and need US work authorization. 'Entry-level?' = Yes flags new-grad / 0-2 yr roles.", ""),
    ("Backdoor does not confirm availability - check the Job link before emailing.", ""),
    ("", ""),
    ("SOURCES", "h"),
    ("remote 8/9 - Backdoor 3,000 startup leads (YC + GreenRobot)  |  remote-1 - 300 remote hiring contacts (HN)", ""),
    ("remote 2/4/5 - US / Canada / UK visa-sponsorship roles  |  remote 6/7 - 400 H-1B employers  |  remote 3 - MBA roles", ""),
]
for i, (t, kind) in enumerate(lines, 1):
    c = rm.cell(row=i, column=1, value=t)
    if kind == "title": c.font = Font(size=16, bold=True, color=NAVY)
    elif kind == "sub": c.font = Font(italic=True, color="666666")
    elif kind == "h": c.font = Font(bold=True, color=LINK)
rm.column_dimensions["A"].width = 120

WIDTHS = [22, 34, 12, 26, 15, 16, 12, 13, 13, 12, 10, 24, 12, 40, 28, 12, 28, 18, 18, 13]
def write_tab(title, desc, data):
    ws = wb.create_sheet(title)
    ws["A1"] = title; ws["A1"].font = Font(size=16, bold=True, color=NAVY)
    ws["A2"] = desc; ws["A2"].font = Font(italic=True, color="666666")
    for j, h in enumerate(COLS, 1):
        c = ws.cell(row=4, column=j, value=h)
        c.font = Font(bold=True, color="FFFFFF"); c.fill = PatternFill("solid", fgColor=NAVY)
        c.alignment = Alignment(vertical="center", wrap_text=True); c.border = Border(bottom=thin)
    for i, r in enumerate(data, 5):
        for j, h in enumerate(COLS, 1):
            v = r[h]
            c = ws.cell(row=i, column=j)
            if h in ("Find link", "Website", "Job link") and v:
                c.value = {"Find link": "Search", "Website": v.replace("https://", "").replace("http://", "").rstrip("/")[:40], "Job link": "Open"}[h]
                c.hyperlink = v; c.font = Font(color=LINK, underline="single")
            elif h == "Contact email" and v:
                first = v.split(";")[0].strip()
                c.value = v; c.hyperlink = "mailto:" + first; c.font = Font(color=LINK, underline="single")
            else:
                c.value = v if v != "" else None
            c.border = Border(bottom=thin)
            if i % 2 == 0: c.fill = PatternFill("solid", fgColor=BAND)
    for j, wdt in enumerate(WIDTHS, 1): ws.column_dimensions[get_column_letter(j)].width = wdt
    ws.freeze_panes = "A5"
    ws.auto_filter.ref = f"A4:{get_column_letter(len(COLS))}{max(5, len(data) + 4)}"

write_tab("Best for you", "India-based and worldwide-remote tech roles - your realistically applicable set. Sorted entry-level first, then rows with a contact email.", best)
write_tab("Remote", "All remote-eligible tech roles from every list.", remote)
write_tab("Hybrid-Flex", "Tech roles listed as a city plus a remote option.", hybrid)
write_tab("On-site", "Office-based tech roles (mostly US - would need work authorization).", onsite)
write_tab("H-1B Sponsors", "Large US employers with verified H-1B filings for tech roles. Sorted by number of positions filed.", h1b)

wb.save(OUT)
print(dict(stats))
print("best", len(best), "remote", len(remote), "hybrid", len(hybrid), "onsite", len(onsite), "h1b", len(h1b), "total unique", len(rows))
print("with email:", sum(1 for r in rows if r["Contact email"]), "| with contact name:", sum(1 for r in rows if r["Contact name"]))
