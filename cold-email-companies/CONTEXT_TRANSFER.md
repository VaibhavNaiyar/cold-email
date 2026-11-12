# Context Transfer — Cold Email Companies (sub-project)

Handoff for moving `D:\cold-email-companies` into a larger parent project.
Written 10 Nov 2026. Owner: Vaibhav Naiyar (vaibhavnaiyar04@gmail.com).

---

## 1. What this sub-project does

Finds tech companies that are hiring, then writes researched, personalized cold emails to them for jobs and internships. It has three parts:

1. **Company list.** Nine "Backdoor" (textbackdoor.com) lead spreadsheets are merged into one tech-only workbook. It uses the same layout as a reference workbook made for someone else (Rachit).
2. **Email method.** A 10-email cold-email course ("The Cold Email Hall of Fame" by Siddharth Haldar) was converted to Word docs. Its 9 linked PDFs were read and boiled down into a playbook.
3. **Skill.** A Claude Code skill (`cold-email`) picks companies from the workbook, researches each one on the web, writes the email plus 3 follow-ups, saves the draft and marks the workbook.

---

## 2. Folder layout

```
cold-email-companies/
├── CONTEXT_TRANSFER.md                 ← this file
├── .claude/skills/cold-email           ← JUNCTION → cold-email/email-creation (see §5)
├── company-list/
│   ├── refrence-total/                 ← raw inputs (do not edit)
│   │   ├── Rachit_Job_Leads_Compiled.xlsx   format reference
│   │   ├── remote-1.xlsx   300 HN "Who's hiring" remote posts, with hiring emails
│   │   ├── remote 2.xlsx   US visa-sponsorship roles (40)
│   │   ├── remote 3.xlsx   MBA/business roles; used ONLY for contact enrichment
│   │   ├── remote 4.xlsx   Canada sponsorship roles
│   │   ├── remote 5.xlsx   UK sponsorship roles
│   │   ├── remote 6.xlsx   400 H-1B employers (remote 7 = exact duplicate)
│   │   ├── remote 8.xlsx   3,000 startup leads (YC + GreenRobot); superset of remote 9
│   │   └── remote 9.xlsx   1,000 YC leads (= first 1,000 rows of remote 8)
│   └── only-tech/
│       ├── Tech_Company_Leads_Compiled.xlsx   ★ main output
│       └── build_tech_list.py                 rebuilds it from refrence-total
└── cold-email/
    ├── refrence/
    │   ├── outlook/      10 original .eml newsletters
    │   └── word-docs/    10 .docx versions of those emails (made in MS Word, images saved inside)
    └── email-creation/   ★ the skill
        ├── SKILL.md
        ├── profile.md                 sender profile; MOSTLY TODO (see §6)
        ├── references/playbook.md     the course boiled down, with example emails word for word
        ├── references/templates.md    templates by lead source + follow-ups
        ├── references/research.md     how to research a company, find the right person, honesty rules
        ├── scripts/pick_companies.py  filters leads → JSON
        ├── scripts/save_email.py      saves draft + CSV log + marks "Email status"
        └── drafts/                    output (2 sample drafts + drafts_log.csv)
```

---

## 3. The workbook (`Tech_Company_Leads_Compiled.xlsx`)

**Tabs:**

| Tab | Rows | What's in it |
|---|---|---|
| Read Me | – | Explains the workbook |
| Best for you | 55 | India-based, plus worldwide-remote roles |
| Remote | 289 | All remote tech roles |
| Hybrid-Flex | 157 | A city plus a remote option |
| On-site | 479 | Office-based roles, mostly US |
| H-1B Sponsors | 228 | Large US employers with H-1B filings for tech roles |

That is 925 unique tech leads plus 228 H-1B employers; 310 rows have a contact email.

- **Header row:** row 4. Data starts at row 5. The title is in A1 and the description in A2.
- **Columns A–N** match the Rachit reference: Company, Role (example), Industry, Location, Work type, Experience, Entry-level?, Region, Posted / age, Pay, Open roles, Source, Find link, Notes.
- **Columns O–T** were added for cold emailing: Website, Job link, Contact email, Contact name, Visa sponsorship, Email status.
- **Link cells:** Website, Job link and Find link hold display text, with the real URL in the cell's hyperlink. Read `cell.hyperlink.target`, not the cell value.
- **Styling:** navy header `1F3A5F` with white bold text, link colour `2E6FB0`, alternate rows banded `F2F6FB`, panes frozen at A5, autofilter on.

**How rows are classified (`build_tech_list.py`):**
- **Tech test:** regexes on the role title. The order is a strong-tech match, then a non-tech exclusion (sales, marketing, PM, design, ops, finance, hardware/mechanical/electrical/RF, bio…), then a general tech match.
- **Region:** whole words are matched in any case. Two-letter codes (IN, US, CA, DE, OR…) only count when written as codes, so "anywhere in the world" doesn't match India.
- **Work type:** Remote, Hybrid / Flexible, or On-site, based on location segments.
- **Excluded:**
  - 1,697 GreenRobot rows that have only a role count and no title.
  - Job-seeker posts in the HN data ("SEEKING WORK", LinkedIn profiles).
  - Duplicates, matched on normalized company name plus role.
- **Contact enrichment:** emails and founder names come from the HN posts (remote-1) and the MBA list (remote 3), matched on normalized company name.
- **Known weakness:** HN rows whose role reads "Engineering roles - see hiring post (…)" were tagged from keywords and can be wrong. Example: JigsawStack's post was a GTM/Growth role. The skill's research step is meant to catch these.
- **Rebuild command:** `python build_tech_list.py "<refrence-total dir>" "<output.xlsx>"`
- **Warning:** a rebuild **overwrites** the Email status column. Back up or merge the statuses first.

---

## 4. The skill (`cold-email`)

**Workflow (full detail in SKILL.md):**
0. Check `profile.md`. If it still has TODOs, ask the user. Never invent anything about the sender.
1. Pick leads: `python scripts/pick_companies.py --tab "Best for you" --limit 5 --with-email`
   - Other flags: `--region`, `--entry-only`, `--company X` (repeatable), `--include-drafted`, `--workbook`.
   - Rows whose status starts with "Drafted" are skipped by default.
2. Research each company with WebSearch/WebFetch:
   - The row's job link and website, the YC company page, founder footprint, news, GitHub.
   - Goal: one specific hook, one value-first idea, and the right person, each with a source URL.
3. Write using the Golden Formula: Attention → Personal touch → Credibility → Offer value → Simple ask.
   - Subject: a curiosity gap. Body: 75–150 words, grade 5–8 reading level.
   - The ask must be answerable with "Yes", "No" or "Forward". Always include a P.S.
4. Run the self-check list (it is in SKILL.md).
5. Save: `python scripts/save_email.py --company --role --to --to-name --subject --body-file [--notes-file] [--followups-file] [--status]`
   - Writes `drafts/<date>_<company>.md` and appends `drafts/drafts_log.csv`.
   - Sets "<status> <date>" in every workbook tab where the company appears.
   - If the workbook is open in Excel, it warns instead of failing.
6. Write 3 follow-ups: day 3–4 (progress/value), day 7–8 (objection), day 14 (easy out).
7. Report to the user. **Never send.** Create Gmail *drafts* only if the user explicitly asks.

**Course method in one line each:**
- Curiosity-gap subject.
- Open like you'd text a friend; never "Dear Sir".
- Use your unique story as credibility.
- Give value before you ask (the course calls this the "reciprocity trigger").
- Make the ask one-word answerable ("3-Test Rule": Yes / No / Forward).
- Keep it skimmable, with short lines.
- Always add a P.S.
- Follow up persistently, always giving them an easy way out.
- Warm intros: write the forwardable intro email for the person introducing you.

---

## 5. Things that break when you move the folder (fix these)

1. **Junction.** `.claude/skills/cold-email` is a Windows directory junction pointing to `D:\cold-email-companies\cold-email\email-creation`. Moving the folder breaks it.
   - Recreate it in the new project. In PowerShell (no admin needed):
     `New-Item -ItemType Junction -Path "<new>\.claude\skills\cold-email" -Target "<new>\...\email-creation"`
   - Or copy `email-creation/` directly into `<new>/.claude/skills/cold-email/`.
2. **Absolute paths in `SKILL.md`.** The "Workbook:" and "Original course docs:" lines use `D:\cold-email-companies\…`. Update them.
3. **Relative paths in the scripts.**
   - `pick_companies.py` finds the workbook at `Path(__file__).parents[3] / company-list / only-tech / Tech_Company_Leads_Compiled.xlsx`.
   - `save_email.py` uses `SKILL.parents[1] / company-list / …`.
   - Keep the folders in the same positions relative to each other, or pass `--workbook <path>`, or edit `DEFAULT_WB`.
4. **Python dependencies:** `openpyxl` (both scripts and the build script). Python 3.12 was used. `pymupdf`, `pandas` and `python-docx` were installed but are only needed for one-off conversion work.

---

## 6. Open items / next steps

- [ ] **Fill in `email-creation/profile.md`.** It needs: location/timezone, work authorization, current status, target roles, the 1–2 sentence story only Vaibhav can tell, 2–3 proof points with numbers, skills, links (GitHub, portfolio, LinkedIn, resume), personal details for P.S. lines, and things never to say. Only name and email are filled in now.
- [ ] **Finish the two sample drafts** in `drafts/`: Aktoria Robotics, and Logen.io. Both have `[TODO]` slots where the sender's proof goes.
  - Their workbook status is "Sample (needs profile) 2026-11-10", so they still show up as un-drafted.
  - The Logen recipient's name "Shvilesh" is guessed from the email address. Verify it.
  - Re-check the Logen site observations on the live site before sending.
- [ ] Optional: a mail-merge or Gmail-drafts step built on `drafts_log.csv`. The Gmail MCP tools are available in Claude Code; create drafts only, never send.
- [ ] Optional: manually review the "see hiring post" HN rows, or re-research them, to drop non-engineering roles.
- [ ] Optional: research the 1,697 excluded GreenRobot companies (company names only) to recover tech ones.
- [ ] Housekeeping: a hidden WINWORD.EXE (PID 166520) was left running by the first failed conversion run. End it in Task Manager if it's still there.

---

## 7. Gotchas learned

- **PowerShell variables are case-insensitive.** `$s` and `$S` are the same variable; a loop variable once clobbered a path because of this.
- **Word COM `SaveAs2`** needs `[string]` typed arguments.
- **Opening HTML in Word:** Word ignores CSS `width:100%` on images. Set explicit `width`/`height` attributes, then call `LinkFormat.BreakLink()` so the images are saved inside the file.
- **Opening a new Word instance per document** fixed a "file not found" error. The real cause turned out to be the `$s` bug above.
- **Course PDFs** sit behind beehiiv tracking redirects that lead to Google Drive viewer pages. Direct download format: `https://drive.usercontent.google.com/download?id=<id>&export=download`.
- **Most course PDFs are single tall images.** Pull the embedded image out with pymupdf and slice it to read it; rendering the A4 page is too low-resolution.
- **openpyxl `read_only=True`** cuts rows short in some of these sheets. Use normal mode.
- **Source links** are stored either as `=HYPERLINK("url","label")` formulas or as cell hyperlinks. `link_of()` in the build script handles both.
- **Running Python with `-I`** ignores `PYTHONIOENCODING`. Call `sys.stdout.reconfigure(encoding="utf-8")` in scripts, because emoji in subject lines crash cp1252 output.

---

## 8. Sources used in the sample research

- https://www.ycombinator.com/companies/aktoria-robotics/jobs/SaL7wK7-forward-deployed-engineer
- https://aktoria.com/
- https://www.ycombinator.com/companies/aktoria-robotics
- https://www.ibtimes.com/aktoria-robotics-human-operators-teleoperation-3809341
- https://news.ycombinator.com/item?id=48760654 (Logen.io)
- https://logen.io
- https://news.ycombinator.com/item?id=46996070 (JigsawStack; turned out to be a GTM role)
