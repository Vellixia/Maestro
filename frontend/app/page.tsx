"use client";
import { useEffect, useState } from "react";
import Link from "next/link";
import { api, RunSummary } from "@/lib/api";

function StatusBadge({ status }: { status: string }) {
  return (
    <span className={`badge badge-${status}`}>{status}</span>
  );
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="card flex flex-col gap-1">
      <div className="text-xs text-[var(--muted)] uppercase tracking-wider">{label}</div>
      <div className="text-2xl font-bold">{value}</div>
    </div>
  );
}

export default function Dashboard() {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.listRuns(10).then(setRuns).finally(() => setLoading(false));
  }, []);

  const completed = runs.filter(r => r.status === "completed");
  const totalCost = completed.reduce((s, r) => s + r.total_cost_usd, 0);
  const totalTokens = completed.reduce((s, r) => s + r.total_tokens, 0);
  const avgWall = completed.length
    ? Math.round(completed.reduce((s, r) => s + (r.wall_ms ?? 0), 0) / completed.length)
    : 0;

  return (
    <div className="max-w-6xl mx-auto flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Dashboard</h1>
        <Link
          href="/orchestrate"
          className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-light)] hover:text-black transition-colors"
        >
          + New Run
        </Link>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard label="Total Runs" value={runs.length} />
        <StatCard label="Total Cost" value={`$${totalCost.toFixed(4)}`} />
        <StatCard label="Total Tokens" value={totalTokens.toLocaleString()} />
        <StatCard label="Avg Wall Time" value={avgWall ? `${avgWall}ms` : "—"} />
      </div>

      <div className="card">
        <h2 className="text-sm font-semibold text-[var(--muted)] uppercase tracking-wider mb-4">
          Recent Runs
        </h2>
        {loading && <p className="text-[var(--muted)] text-sm">Loading…</p>}
        {!loading && runs.length === 0 && (
          <p className="text-[var(--muted)] text-sm">
            No runs yet.{" "}
            <Link href="/orchestrate" className="text-[var(--accent-light)] underline">
              Start your first orchestration →
            </Link>
          </p>
        )}
        {runs.map(run => (
          <Link
            key={run.run_id}
            href={`/runs/${run.run_id}`}
            className="flex items-center gap-4 py-3 border-t border-[var(--card-border)] hover:bg-white/5 -mx-5 px-5 transition-colors"
          >
            <StatusBadge status={run.status} />
            <span className="flex-1 text-sm truncate">{run.goal}</span>
            <span className="text-xs text-[var(--muted)]">${run.total_cost_usd.toFixed(4)}</span>
            <span className="text-xs text-[var(--muted)]">
              {new Date(run.created_at).toLocaleString()}
            </span>
          </Link>
        ))}
        {runs.length > 0 && (
          <div className="pt-3 text-center">
            <Link href="/runs" className="text-xs text-[var(--accent-light)] hover:underline">
              View all runs →
            </Link>
          </div>
        )}
      </div>
    </div>
  );
}
