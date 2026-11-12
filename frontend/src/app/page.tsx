import { Suspense } from "react";
import { Scout, type Tab } from "@/components/scout";
import type { Company, Job } from "@/lib/types";

const BACKEND_URL = process.env.BACKEND_URL ?? "http://127.0.0.1:8787";

export default function Page({ searchParams }: PageProps<"/">) {
  return (
    <Suspense fallback={<Loading />}>
      <Data searchParams={searchParams} />
    </Suspense>
  );
}

type Fetched<T> = { data: T; date: number };

async function getJson<T>(path: string): Promise<Fetched<T>> {
  const response = await fetch(`${BACKEND_URL}${path}`, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path} answered ${response.status}`);
  // The API's own clock: one "now" for every age on the page, on server and browser alike.
  const date = Date.parse(response.headers.get("date") ?? "");
  if (Number.isNaN(date)) throw new Error(`${path} sent no Date header`);
  return { data: (await response.json()) as T, date };
}

async function Data({ searchParams }: { searchParams: PageProps<"/">["searchParams"] }) {
  const { tab } = await searchParams;
  const initialTab: Tab = tab === "jobs" ? "jobs" : "startups";
  const [companies, jobs] = await Promise.allSettled([
    getJson<Company[]>("/api/companies"),
    getJson<Job[]>("/api/jobs"),
  ]);
  if (companies.status === "rejected") return <Offline />;
  // Jobs failing alone (for example an older backend) leaves the startups usable.
  return (
    <Scout
      companies={companies.value.data}
      jobs={jobs.status === "fulfilled" ? jobs.value.data : null}
      now={companies.value.date}
      initialTab={initialTab}
    />
  );
}

function Loading() {
  return (
    <main className="mx-auto max-w-6xl px-4 py-10" aria-busy="true">
      <p className="text-muted">Loading companies and jobs…</p>
      <div className="mt-6 grid gap-3">
        {Array.from({ length: 6 }, (_, index) => (
          <div key={index} className="h-20 rounded-lg bg-line/50" />
        ))}
      </div>
    </main>
  );
}

function Offline() {
  return (
    <main className="mx-auto max-w-xl px-4 py-24">
      <h1 className="text-3xl font-semibold tracking-tight">The backend isn&apos;t running</h1>
      <p className="mt-4 leading-7 text-muted">
        This page reads from the Rust API on port 8787. Start it from the backend folder, then
        reload:
      </p>
      <pre className="mt-4 overflow-x-auto rounded-lg border border-line bg-surface p-4 text-sm">
        cd backend{"\n"}cargo scout serve
      </pre>
    </main>
  );
}
