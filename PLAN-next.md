# Startup Scout: next two features

> **Status (2026-11-02):** B is built, scoped to what was asked: one user, and funded startups at any stage that hire remotely or in India and Germany. Big-company adapters (Workday, SmartRecruiters, Amazon, Google, Microsoft) and Adzuna are not built. A (other users) is on hold.
>
> **As built:** the `scout-jobs` crate, `cargo scout jobs` (also part of `cargo scout run`), migration `0004_jobs`, `GET /api/jobs`, and a Jobs tab on the page.
> - **Boards:** each YC company's own jobs page (data embedded in the page, allowed by robots.txt), plus Greenhouse, Lever and Ashby boards that a startup links to from its own site. Slugs are never guessed, because a guess can match a namesake company. Extra boards go in `backend/job_boards.yaml`.
> - **Filters:** title words, levels and on-site places come from `profile.yaml` (`job_titles`, `onsite_regions`). Remote roles from anywhere are kept, and Claude Haiku scores fit, including visa and country limits.
> - **Limits:** the run limits are in `sources.yaml` (`max_career_checks_per_run`, `career_recheck_days`, `max_job_scores_per_run`).
>
> **2026-11-04 update:**
> - **Job feeds:** Hacker News "Who is hiring?" (Algolia API, the post's header line parsed), RemoteOK by tag and Himalayas by search query, set up as boards in `job_boards.yaml`. Feed roles carry their own company and close after 14 days unseen, because a feed only shows its latest results.
> - **Faster YC reads:** YC pages are read only for companies YC marks as hiring, boards with open roles, or every `yc_recheck_days`. A refresh went from about 9 minutes to under 3.
> - **New roles:** DevOps, SRE, platform and cloud titles were added.
> - **Funding feeds:** Crunchbase News and TechCrunch AI were added.
> - **Skipped:** Remotive (10 delayed jobs on the free API) and Arbeitnow (mostly German-language roles).

Two features, planned here before any code. Both build on what exists: Rust backend in `backend/`, Next.js page in `frontend/`, Postgres.

- **A. Profiles for other people.** Anyone can add their resume, GitHub and LinkedIn, and every company gets scored for them.
- **B. Daily job links.** Each day, a list of newly opened roles at large companies in the US, UK and India, plus remote roles, each linking to the company's own careers page.

Suggested order: **B first**, because it helps you right away and needs no accounts. A comes next, together with sign-in.

---

## A. Profiles for other people

### What a user does
1. Sign in. This is the sign-in work already planned for the page.
2. Upload a resume PDF, enter a GitHub username, and add LinkedIn (see the LinkedIn section).
3. Review the profile the app builds: summary, roles, stack, proof points, locations. Edit it and save.
4. See the same company list, scored for them, with emails written in their voice.

### How each source is read

| Source | How | Notes |
|---|---|---|
| Resume (PDF) | Sent to Claude as a document with a JSON schema for summary, roles, stack, experience, projects and locations | This is how your resume was read today. No PDF library needed. |
| GitHub | Public REST API: repo list, then README, `package.json`, `requirements.txt` and `Cargo.toml` from raw.githubusercontent.com; the LLM summarises what was built | Same as today. Unauthenticated limit is 60 calls/hour, so use a server GitHub token. Repos with a starter README get their file tree read. |
| LinkedIn | **Not scraped.** The user uploads LinkedIn's own "Save to PDF" export, or pastes the About and Experience text | LinkedIn blocked our fetch today (HTTP 999), and its terms forbid scraping. "Sign in with LinkedIn" only returns name, email and photo, not experience. |

All three become one profile record. The user edits it, and the profile they saved is what drives scoring.

### Database changes
- `users` (id, email, created_at), and `profiles` (user_id, summary, roles, stack, locations, onsite_regions, stages, proof_points, sources_json, updated_at). `profile.yaml` becomes your own row.
- Scores move from `companies.score` to `user_scores (user_id, company_id, score, reason, best_proof_point, scored_at)`. Companies, signals and contacts stay shared, since they are facts about the company.
- `drafts` gets a `user_id`. Suppression stays global, so an opt-out is honoured for everyone.

### Keeping scoring cost under control
Scoring 300+ companies for every user with Sonnet doesn't scale. Instead:
1. Embed each company once, and each profile once (pgvector in Postgres).
2. Keep the closest ~60 companies per user by vector similarity.
3. Score only those with the LLM, as today.

Rough cost is a few cents per user per day. It only rises when new companies arrive or a profile changes.

### Email for other users: the hard part
- **Gmail App Passwords** (what you use) can't be asked of strangers.
- **Gmail OAuth with the drafts scope** is a Google *restricted* scope. Public launch needs Google's verification plus a paid yearly security assessment. Until then, a project can have at most 100 test users.
- **Plan:** launch with "Copy email" and "Open in mail app", which already exist on the page. Add Gmail OAuth for up to 100 invited test users. Do the verification only if the product takes off.

### Privacy
Resumes hold phone numbers and addresses. Keep the PDF only until it has been parsed, then store the profile text and not the file. Add a "Delete my data" button. Each user's drafts are visible only to them.

---

## B. Daily job links from company careers pages

### What you get each morning
A **Jobs** tab on the page, plus an optional email digest, listing roles first seen in the last 24 hours. Each one shows title, company, location, remote or not, posted date, a fit score against your profile, and a **direct link to the company's careers page**.

### How jobs are collected
Most large companies host their careers page on an applicant-tracking system (ATS), and most of those publish job lists as public JSON. That is far more reliable and far more polite than scraping HTML.

| Source | Endpoint | Coverage |
|---|---|---|
| Greenhouse | `boards-api.greenhouse.io/v1/boards/{slug}/jobs` | Many tech companies (Stripe, Airbnb, Databricks...) |
| Lever | `api.lever.co/v0/postings/{slug}` | Many mid-size tech companies |
| Ashby | `api.ashbyhq.com/posting-api/job-board/{slug}` | Most AI labs and newer startups |
| SmartRecruiters | `api.smartrecruiters.com/v1/companies/{id}/postings` | Large enterprises (Visa, Bosch...) |
| Workday | The JSON endpoint behind `{tenant}.myworkdayjobs.com` career sites | Most Fortune 500 companies and many Indian firms |
| Custom big-tech pages | One small adapter each (Amazon, Google, Microsoft, Apple, Meta) | Done one at a time; skipped where the site forbids automated access |
| Any other careers page | Reads schema.org `JobPosting` data, which sites embed for Google Jobs | Generic fallback |
| Adzuna API (optional) | Free official job-search API covering the US, UK and India | Broad catch-all with links to the original posting |

**Company list:** a `companies.yaml` of big employers, seeded with about 300 names across the US, UK and India. Each has its ATS and slug. A discovery step finds the ATS automatically: open the careers page and recognise links such as `boards.greenhouse.io/x` or `x.myworkdayjobs.com`. You can add any company by name.

**Not used:** LinkedIn Jobs, Indeed and Glassdoor scraping. Their terms forbid it and they block bots. Jobs still get found, because nearly every company also lists its jobs on its own careers page, which is where our links go.

### Daily pipeline
1. **Fetch:** each company's job list at about 1 request per second per host, respecting robots.txt, with the same contact User-Agent as today.
2. **Normalise:** title, location(s), remote flag, posted date, department, and the apply URL on the company's own site.
3. **Store:** a `jobs` table with `first_seen_at`, `last_seen_at` and `closed_at`. A job missing from two runs in a row counts as closed.
4. **Filter:**
   - Country: US, UK, India, or remote.
   - Title: match against your roles, with a keyword prefilter (engineer, developer, AI, ML, frontend, full-stack...).
   - Level: keep intern, new-grad, junior and SDE I/II roles; drop senior, staff, principal and manager. Easy to change.
5. **Score:** Haiku rates only the jobs that pass the filters, against your profile: fit and why. That's a few hundred per day at most.
6. **Show:** the Jobs tab, newest first, with filters for country, remote, level and minimum fit, plus the digest.

### What it runs on
A new Rust crate, `scout-jobs`, with one adapter per ATS, the same pattern as `scout-sources`. A new command, `cargo scout jobs`, joins `cargo scout run`. The API gets `GET /api/jobs`, and the page gets a Jobs tab. Tests use saved JSON responses, as today.

### Build steps for B

| Step | Deliverable | Done when |
|---|---|---|
| B1 | `jobs` table, Greenhouse, Lever and Ashby adapters, a 50-company seed list | One run stores jobs with working links; a re-run adds no duplicates |
| B2 | Workday and SmartRecruiters adapters, ATS discovery from a careers URL, ~300 companies | Coverage is reported per country |
| B3 | Location, level and title filters, plus Haiku scoring | A daily list of new jobs that fit, with reasons |
| B4 | Jobs tab on the page and an optional digest | You can open a role's careers page in one click |
| B5 | Big-tech custom adapters and the Adzuna catch-all | Amazon, Google and Microsoft roles appear |

### Cost
The ATS APIs and the Adzuna free tier cost nothing. LLM scoring is a few hundred Haiku calls a day, so cents. Runs fit in the existing daily job.

---

## Decisions needed before building
1. Start with B (jobs) and do A (multi-user) after sign-in exists?
2. Jobs: which levels count? Intern, new-grad and SDE I by default.
3. Jobs: any must-have companies for the seed list?
4. Multi-user: is the 100-test-user Gmail limit acceptable at launch?
