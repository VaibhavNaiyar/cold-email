"use client";

import { useMemo, useState } from "react";
import { Choice, Count, DATE_FILTERS, type Option, SearchBox, Toggle, Toolbar } from "@/components/controls";
import { ago, formatDate, isWithin, matchesQuery, stageLabel } from "@/lib/format";
import type { Job } from "@/lib/types";

type Sort = "fit" | "newest";

// Startup roles from the last 10 days are what this list is for; both can be widened.
const DEFAULT_DAYS = 10;

const SORTS: readonly Option<Sort>[] = [
  { value: "fit", label: "Best fit first" },
  { value: "newest", label: "Newest first" },
];

const FITS: readonly Option<number>[] = [
  { value: 0, label: "Any fit" },
  { value: 40, label: "Fit 40+" },
  { value: 60, label: "Fit 60+" },
  { value: 75, label: "Fit 75+" },
];

const BOARD_NAMES: Record<string, string> = {
  yc: "YC",
  greenhouse: "Greenhouse",
  lever: "Lever",
  ashby: "Ashby",
  remoteok: "RemoteOK",
  himalayas: "Himalayas",
  hn: "Hacker News: Who is hiring?",
  wwr: "We Work Remotely",
};

// What each board's "posted" date really means.
const DATE_SOURCE: Record<string, string> = {
  ashby: "Ashby's publish date. Ashby resets it when a company re-posts a role.",
  greenhouse: "Greenhouse's first-published date.",
  lever: "Lever's creation date.",
  yc: "YC shows only an age such as \"4 days\", so this date is approximate.",
  remoteok: "RemoteOK's listing date.",
  himalayas: "Himalayas' publish date.",
  hn: "When the post went up in the monthly Hacker News thread.",
  wwr: "We Work Remotely's listing date.",
};

// Says remote or on-site once, even when the board's location already says "Remote".
function placeLabel(job: Job): string {
  const kind = job.remote ? "Remote" : "On-site";
  if (!job.location) return kind;
  return /remote/i.test(job.location) ? job.location : `${kind}, ${job.location}`;
}

// The board's own date when it gives one, otherwise when Scout first saw the role.
const postedDate = (job: Job) => job.posted_at ?? job.first_seen_at;

export function JobsBoard({ jobs, now }: { jobs: Job[]; now: number }) {
  const [query, setQuery] = useState("");
  const [within, setWithin] = useState(DEFAULT_DAYS);
  const [minFit, setMinFit] = useState(0);
  const [remoteOnly, setRemoteOnly] = useState(false);
  const [startupsOnly, setStartupsOnly] = useState(true);
  const [sort, setSort] = useState<Sort>("fit");

  const visible = useMemo(() => {
    const rows = jobs.filter(
      (job) =>
        (!remoteOnly || job.remote) &&
        // Unknown companies stay in; only those judged not to be startups drop out.
        (!startupsOnly || job.is_startup !== false) &&
        (job.fit ?? 0) >= minFit &&
        isWithin(postedDate(job), within, now) &&
        matchesQuery(query, job.title, job.company, job.location, job.company_about),
    );
    const by: Record<Sort, (a: Job, b: Job) => number> = {
      fit: (a, b) => (b.fit ?? -1) - (a.fit ?? -1),
      newest: (a, b) => postedDate(b).localeCompare(postedDate(a)),
    };
    return rows.sort(by[sort]);
  }, [jobs, query, within, minFit, remoteOnly, startupsOnly, sort, now]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Toolbar>
        <SearchBox
          label="Search roles"
          placeholder="Search by title, company or place"
          value={query}
          onChange={setQuery}
        />
        <Choice label="Posted" value={within} options={DATE_FILTERS} onChange={setWithin} />
        <Choice label="Minimum fit" value={minFit} options={FITS} onChange={setMinFit} />
        <Choice label="Sort" value={sort} options={SORTS} onChange={setSort} />
        <Toggle label="Startups only" checked={startupsOnly} onChange={setStartupsOnly} />
        <Toggle label="Remote only" checked={remoteOnly} onChange={setRemoteOnly} />
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <Count shown={visible.length} total={jobs.length} />
        {jobs.length === 0 ? (
          <p className="px-4 py-10 text-sm leading-6 text-muted lg:px-6">
            No roles yet. Run <code>cargo scout jobs</code> in the backend folder to read the careers boards.
          </p>
        ) : visible.length === 0 ? (
          <p className="px-4 py-10 text-sm leading-6 text-muted lg:px-6">
            No role matches. Clear the search, widen the date range, lower the minimum fit or turn off “Startups only”.
          </p>
        ) : (
          <ul className="mx-auto max-w-5xl">
            {visible.map((job) => (
              <li key={job.id}>
                <JobRow job={job} now={now} />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function JobRow({ job, now }: { job: Job; now: number }) {
  // Only a role that appeared after Scout's first read of its board is known to be new.
  const isNew = !job.baseline && isWithin(job.first_seen_at, 1, now);
  return (
    <article className="grid gap-x-6 gap-y-2 border-b border-line px-4 py-4 sm:grid-cols-[1fr_auto] lg:px-6">
      <div className="min-w-0">
        <h3 className="font-semibold leading-snug">
          <a href={job.url} target="_blank" rel="noreferrer" className="underline-offset-4 hover:text-navy hover:underline">
            {job.title}
          </a>
          {isNew && <span className="ml-2 rounded-sm border border-red px-1 text-xs font-normal text-red">New</span>}
        </h3>
        <p className="mt-0.5 text-sm">
          {job.company}
          {job.company_stage && job.company_stage !== "unknown" && (
            <span className="text-muted"> ({stageLabel(job.company_stage)})</span>
          )}
          {job.company_about && <span className="text-muted">: {job.company_about}</span>}
        </p>
        <p className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
          <span className={job.remote ? "font-semibold text-navy" : ""}>{placeLabel(job)}</span>
          <span title={DATE_SOURCE[job.board] ?? "Date from the company's careers board."}>
            Posted {formatDate(postedDate(job))}, {ago(postedDate(job), now)}
            {job.board === "yc" && " (approximate)"}
          </span>
          <span>
            {job.baseline
              ? "Already listed when Scout first checked"
              : `Scout found it ${ago(job.first_seen_at, now)}`}
          </span>
          <span>via {BOARD_NAMES[job.board] ?? job.board}</span>
        </p>
        {job.notes && <p className="mt-1 text-xs text-muted">{job.notes}</p>}
        {job.fit_reason && <p className="mt-2 max-w-[70ch] text-sm leading-6">{job.fit_reason}</p>}
      </div>
      <div className="flex items-start gap-4 sm:flex-col sm:items-end">
        <span className="text-sm tabular-nums text-muted" title="Fit with your profile, out of 100">
          <span className="text-2xl font-semibold text-ink">{job.fit ?? "–"}</span>/100
        </span>
        <a
          href={job.url}
          target="_blank"
          rel="noreferrer"
          className="rounded-md bg-navy px-3 py-2 text-sm font-semibold text-surface"
        >
          Open careers page
        </a>
      </div>
    </article>
  );
}
