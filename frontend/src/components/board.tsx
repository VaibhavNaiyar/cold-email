"use client";

import { useMemo, useState } from "react";
import { Choice, Count, DATE_FILTERS, type Option, SearchBox, Toggle, Toolbar } from "@/components/controls";
import { Letter } from "@/components/letter";
import {
  OLD_AFTER_DAYS,
  REACH_LABEL,
  age,
  daysSince,
  formatDate,
  formatMoment,
  fundingSource,
  isWithin,
  keySignal,
  matchesQuery,
  money,
  reach,
  signalLabel,
  sourceLabel,
  stageLabel,
} from "@/lib/format";
import type { Company } from "@/lib/types";

type Sort = "fit" | "raised" | "newest";

const SORTS: readonly Option<Sort>[] = [
  { value: "fit", label: "Best fit first" },
  { value: "raised", label: "Most raised first" },
  { value: "newest", label: "Newest first" },
];

const keyDate = (company: Company) => keySignal(company)?.published_at ?? "";

const STAGE_FILTERS: readonly Option<string>[] = [
  { value: "all", label: "All stages" },
  { value: "pre-seed", label: "Pre-seed" },
  { value: "seed", label: "Seed" },
  { value: "series-a", label: "Series A" },
  { value: "unknown", label: "Unknown" },
];

export function Board({ companies, now }: { companies: Company[]; now: number }) {
  const [query, setQuery] = useState("");
  const [stage, setStage] = useState("all");
  const [within, setWithin] = useState(0);
  const [withEmail, setWithEmail] = useState(false);
  const [sort, setSort] = useState<Sort>("fit");
  const [selectedId, setSelectedId] = useState<number | null>(companies[0]?.id ?? null);
  const [detailOpen, setDetailOpen] = useState(false);

  const visible = useMemo(() => {
    const rows = companies.filter(
      (company) =>
        (stage === "all" || (company.stage ?? "unknown") === stage) &&
        (!withEmail || company.contact !== null) &&
        isWithin(keyDate(company), within, now) &&
        matchesQuery(query, company.name, company.domain, company.one_liner),
    );
    const by: Record<Sort, (a: Company, b: Company) => number> = {
      fit: (a, b) => b.score - a.score,
      raised: (a, b) => (b.funding.amount_usd ?? -1) - (a.funding.amount_usd ?? -1),
      newest: (a, b) => keyDate(b).localeCompare(keyDate(a)),
    };
    return rows.sort(by[sort]);
  }, [companies, query, stage, within, withEmail, sort, now]);

  const selected = companies.find((company) => company.id === selectedId) ?? null;

  const open = (id: number) => {
    setSelectedId(id);
    setDetailOpen(true);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Toolbar>
        <SearchBox
          label="Search companies"
          placeholder="Search by name or what they build"
          value={query}
          onChange={setQuery}
        />
        <Choice label="Stage" value={stage} options={STAGE_FILTERS} onChange={setStage} />
        <Choice label="News date" value={within} options={DATE_FILTERS} onChange={setWithin} />
        <Choice label="Sort" value={sort} options={SORTS} onChange={setSort} />
        <Toggle label="Only with an email" checked={withEmail} onChange={setWithEmail} />
      </Toolbar>

      <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[minmax(340px,420px)_minmax(0,1fr)]">
        <nav
          aria-label="Companies"
          className={`min-h-0 overflow-y-auto border-line lg:block lg:border-r ${detailOpen ? "hidden" : ""}`}
        >
          <Count shown={visible.length} total={companies.length} />
          {visible.length === 0 ? (
            <p className="px-4 py-10 text-sm leading-6 text-muted lg:px-6">
              No company matches. Clear the search, widen the date range or turn off “Only with an email”.
            </p>
          ) : (
            <ul>
              {visible.map((company) => (
                <li key={company.id}>
                  <Row
                    company={company}
                    now={now}
                    selected={company.id === selectedId}
                    onSelect={() => open(company.id)}
                  />
                </li>
              ))}
            </ul>
          )}
        </nav>

        <main className={`min-h-0 overflow-y-auto lg:block ${detailOpen ? "" : "hidden"}`}>
          {selected ? (
            <Detail key={selected.id} company={selected} now={now} onBack={() => setDetailOpen(false)} />
          ) : (
            <p className="p-10 text-muted">Pick a company from the list.</p>
          )}
        </main>
      </div>
    </div>
  );
}

function Row({
  company,
  now,
  selected,
  onSelect,
}: {
  company: Company;
  now: number;
  selected: boolean;
  onSelect: () => void;
}) {
  const how = reach(company);
  const amount = company.funding.amount_usd;
  const signal = keySignal(company);
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected ? "true" : undefined}
      className={`block w-full border-b border-l-[3px] border-b-line px-4 py-3 text-left lg:px-6 ${
        selected ? "border-l-navy bg-tint" : "border-l-transparent hover:bg-surface"
      }`}
    >
      <span className="flex items-baseline justify-between gap-3">
        <span className="truncate font-semibold">{company.name}</span>
        <Score value={company.score} />
      </span>
      <span className="mt-0.5 line-clamp-2 text-sm leading-5 text-muted">
        {company.one_liner ?? "No description yet."}
      </span>
      {signal && <When signal={signal.published_at} label={signalLabel(signal)} now={now} />}
      <span className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        <span className="font-semibold tabular-nums">
          {amount ? `${money(amount)} raised` : stageLabel(company.stage)}
        </span>
        <span className={how === "person" ? "text-navy" : "text-muted"}>{REACH_LABEL[how]}</span>
        {company.draft && <span className="text-red">Drafted</span>}
      </span>
    </button>
  );
}

function When({ signal, label, now }: { signal: string; label: string; now: number }) {
  const days = daysSince(signal, now);
  const old = days > OLD_AFTER_DAYS;
  return (
    <span className="mt-2 flex flex-wrap items-center gap-x-2 text-xs text-muted">
      <span>
        {label} {formatDate(signal)}, {age(days)}
      </span>
      {old && (
        <span className="rounded-sm border border-red px-1 text-red" title="The latest news is older than 3 months">
          Old news
        </span>
      )}
    </span>
  );
}

function Score({ value }: { value: number }) {
  return (
    <span className="shrink-0 text-sm tabular-nums text-muted" title="Fit with your profile, out of 100">
      <span className="font-semibold text-ink">{value}</span>/100
    </span>
  );
}

function Detail({ company, now, onBack }: { company: Company; now: number; onBack: () => void }) {
  const { funding } = company;
  const source = fundingSource(funding);
  const signal = keySignal(company);
  const days = signal ? daysSince(signal.published_at, now) : null;
  return (
    <article className="mx-auto max-w-3xl px-4 pb-16 pt-4 lg:px-10 lg:pt-10">
      <button type="button" onClick={onBack} className="mb-4 text-sm text-navy lg:hidden">
        Back to the list
      </button>

      <h2 className="text-4xl font-bold leading-tight tracking-tight lg:text-5xl">{company.name}</h2>
      <p className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
        <a href={`https://${company.domain}`} target="_blank" rel="noreferrer" className="text-navy underline-offset-4 hover:underline">
          {company.domain}
        </a>
        <span>{stageLabel(company.stage)}</span>
        {company.region && company.region !== "unknown" && <span>{company.region}</span>}
        {company.remote && <span>Remote-friendly</span>}
      </p>

      {signal && days !== null && (
        <p className={`mt-3 text-sm ${days > OLD_AFTER_DAYS ? "text-red" : "text-muted"}`}>
          {signalLabel(signal)} {formatMoment(signal)}, {age(days)}.
          {days > OLD_AFTER_DAYS && " This news is over 3 months old; they may have moved on."}
        </p>
      )}

      <p className="mt-6 max-w-[65ch] text-lg leading-8">
        {company.one_liner ?? "They haven't published a one-line description."}
      </p>

      <dl className="mt-8 grid gap-6 border-y border-line py-6 sm:grid-cols-2">
        <div>
          <dt className="text-sm text-muted">Funding</dt>
          <dd className="mt-1 text-3xl font-semibold tabular-nums">
            {funding.amount_usd ? money(funding.amount_usd) : "Amount not disclosed"}
          </dd>
          {source && <dd className="mt-1 text-sm text-muted">{source}</dd>}
          {funding.investors && funding.investors.length > 0 && (
            <dd className="mt-1 text-sm">Backed by {funding.investors.join(", ")}</dd>
          )}
        </div>
        <div>
          <dt className="text-sm text-muted">Fit with your profile</dt>
          <dd className="mt-1 text-3xl font-semibold tabular-nums">
            {company.score}
            <span className="text-base font-normal text-muted">/100</span>
          </dd>
          {company.score_reason && (
            <dd className="mt-1 text-sm leading-6 text-muted">{company.score_reason}</dd>
          )}
        </div>
      </dl>

      <section className="mt-8">
        <h3 className="font-semibold">Where we saw them</h3>
        <ul className="mt-3 space-y-2 text-sm">
          {company.signals.map((signal) => (
            <li key={signal.url} className="flex flex-wrap gap-x-3">
              <a href={signal.url} target="_blank" rel="noreferrer" className="text-navy underline-offset-4 hover:underline">
                {signal.title}
              </a>
              <span className="text-muted">
                {sourceLabel(signal.source)}, {formatMoment(signal)}
              </span>
            </li>
          ))}
        </ul>
        <p className="mt-3 text-sm text-muted">
          Scout first found them on {formatDate(company.first_seen_at)}.
        </p>
      </section>

      <section className="mt-10">
        <h3 className="font-semibold">Email</h3>
        <Letter company={company} />
      </section>
    </article>
  );
}
