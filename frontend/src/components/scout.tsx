"use client";

import { useState } from "react";
import { Board } from "@/components/board";
import { JobsBoard } from "@/components/jobs-board";
import { RefreshButton } from "@/components/refresh-button";
import type { Company, Job } from "@/lib/types";

export type Tab = "startups" | "jobs";

export function Scout({
  companies,
  jobs,
  now,
  initialTab,
}: {
  companies: Company[];
  /** Null when the jobs list could not be loaded. */
  jobs: Job[] | null;
  now: number;
  initialTab: Tab;
}) {
  const [tab, setTab] = useState<Tab>(initialTab);
  // Keep the open tab in the address bar, so a reload or bookmark returns to it.
  const choose = (next: Tab) => {
    setTab(next);
    const url = new URL(window.location.href);
    url.searchParams.set("tab", next);
    window.history.replaceState(null, "", url);
  };
  const reachable = companies.filter((company) => company.contact).length;
  const jobCount = jobs?.length ?? 0;
  const goodFits = (jobs ?? []).filter((job) => (job.fit ?? 0) >= 60).length;
  const tabs: { id: Tab; label: string; summary: string }[] = [
    {
      id: "startups",
      label: `Startups (${companies.length})`,
      summary: `${companies.length} funded AI startups. ${reachable} have an address to write to.`,
    },
    {
      id: "jobs",
      label: jobs ? `Jobs (${jobCount})` : "Jobs",
      summary: jobs
        ? `${jobCount} open roles that match your titles. ${goodFits} fit 60 or better.`
        : "The jobs list could not be loaded.",
    },
  ];
  const current = tabs.find((item) => item.id === tab) ?? tabs[0];

  return (
    <div className="flex h-dvh flex-col">
      <header className="border-b border-line bg-surface px-4 pt-4 lg:px-6">
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
          <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
            <h1 className="text-xl font-bold tracking-tight">Startup Scout</h1>
            <p className="text-sm text-muted">{current.summary}</p>
          </div>
          <RefreshButton />
        </div>
        <div role="tablist" aria-label="View" className="mt-3 flex gap-6">
          {tabs.map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={item.id === tab}
              onClick={() => choose(item.id)}
              className={`-mb-px border-b-2 pb-2 text-sm font-semibold ${
                item.id === tab ? "border-navy text-ink" : "border-transparent text-muted hover:text-ink"
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      </header>
      {tab === "startups" ? (
        <Board companies={companies} now={now} />
      ) : jobs ? (
        <JobsBoard jobs={jobs} now={now} />
      ) : (
        <p className="mx-auto max-w-xl px-4 py-16 leading-7 text-muted">
          The backend didn&apos;t return the jobs list. If it was started before the job finder was
          added, stop it and run <code>cargo scout serve</code> again in the backend folder.
        </p>
      )}
    </div>
  );
}
