# Templates — tech job / internship cold emails

Skeletons, not scripts. Every `{…}` must be filled from research or `profile.md`.
If you can't fill a slot with something true and specific, cut the line — don't pad it.
Pick the skeleton by the row's **Source** column.

---

## A. YC / startup founder (Source: "YC startups", "GreenRobot directory")

Recipient: founder or CTO (founders read their own email at <50 people). Use first name.

```
Subject: {curiosity gap tied to their product — e.g. "found a bug in {feature}" /
          "{product} + {thing you built}" / "your {launch} — one idea"}

Hey {First name},

{Personal touch: one specific thing you noticed — their launch / post / product detail —
 + your genuine, specific reaction.}

{Value: what you already did or found for them. A bug + repro, a prototype, a teardown,
 a PR. Link it.}

{Credibility in one line from profile.md — the "only you" story or best proof point.}

{Ask that passes Yes/No/Forward — e.g. "Want me to walk you through it? Just reply 'yes'." /
 "If {role} is still open, who's the right person to send this to?"}

{Sign-off},
{Name}

P.S. {humour, a second personal detail, or a link to GitHub/portfolio}
```

Worked example (shape only — facts are illustrative):

```
Subject: your csv import broke on my 40mb file

Hey Priya,

Tried Acme's new bulk import this weekend for a side project. Loved how fast the preview is.

It choked on a 40MB CSV though — the parser loads the whole file in memory.
**I wrote a streaming version** that handles 1GB in ~9s: github.com/me/acme-stream-import

I'm a final-year CS student who's shipped 3 data tools used by ~2k people.

You're hiring a founding engineer. Worth 15 mins to show you the fix?
Just reply "yes".

Cheers,
Alex

P.S. My laptop fan is still recovering from the 1GB test.
```

---

## B. HN "Who's hiring" post with a public email (Source: "HN Who's Hiring")

They *asked* for email, so be direct — but still personal. Recipient: the email in the row; they wrote the post.

```
Subject: {role from their post} — {one hook, e.g. "built something on your stack" / "re: your HN post"}

Hey {First name or team},

Saw your post for {role} on HN. {One line on what caught you — their product, stack or mission,
 specific.}

{Credibility: 1–2 lines mapping your proof to the stack/requirements they listed.}

{Value: something tiny but real — a note on their product, a relevant repo, a quick idea.}

{Ask: "Happy to do a short paid/unpaid trial task — want me to?" / "Should I send my resume here
 or apply via {link}?"}

{Sign-off},
{Name}

P.S. {location/timezone fit if the post had restrictions — e.g. "I'm in IST and can overlap 4h with PST."}
```

---

## C. Visa-sponsorship list (Source: "US/UK/Canada visa sponsorship list")

Lead with fit and the specific role. Don't open with the visa — mention it once, plainly, near the end.

```
Subject: {role} — {specific hook}

Hey {First name},

{Personal touch on the company/product.}
{Value or proof that maps to the role's requirements.}
{Credibility line.}

{Ask.} (I'd need {visa type} sponsorship — your posting says you support it.)

{Sign-off}, {Name}
P.S. {…}
```

---

## D. Big company / H-1B sponsor (Source: "H-1B LCA filings")

Don't email a generic inbox. Find an **engineering manager or recruiter on the specific team** (LinkedIn,
team blog, conference talk). Ask for one thing: a referral or "who should I talk to".

```
Subject: {their talk/blog/project} → {team} question

Hi {First name},

{Specific: their blog post / talk / OSS project and what you took from it.}

{Credibility one-liner + what you built that's close to their team's work.}

I'm applying for {role, req ID}. Would you be open to pointing me to the right person on
{team}, or referring me if it looks like a fit? A "no" is totally fine too.

{Sign-off}, {Name}
P.S. {link to the closest-matching project}
```

---

## E. Compliment / value-only email (warm-up, no ask)

Use when the user wants to get on someone's radar before a later ask (Day 4: Sam Altman / Alex Cohen).

```
Subject: {Just an appreciation email! / thanks for {thing}}

Hi {First name},

{What you used/read, what it changed for you — specific.}
{One line on what you're doing now because of it.}

No ask — just wanted you to know.
{Name}
```

---

## Follow-ups (same thread, keep the subject)

**#1 — day 3–4: progress / new value**
```
Hey {First name} — quick update: {what you did since, with a link}.
Thought you'd want to see it. Still happy to walk you through it — just say the word.
```

**#2 — day 7–8: objection handling / second freebie**
```
{First name} — guessing {likely objection, e.g. "you want someone with more experience"}.
Fair. Here's {proof that answers it / small extra thing for them}.
```

**#3 — day 14: easy out, human**
```
Hey {First name}, bumping this in case it got buried.
If it isn't a priority right now, no worries at all — still a big fan of what you're building at {company}.
```

Optional later bumps: congratulate a launch/funding, share a relevant article, or ask
"curious — what would have made this a yes?"
