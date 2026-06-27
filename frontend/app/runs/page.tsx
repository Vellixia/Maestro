"use client";
import { useEffect, useState } from "react";
import Link from "next/link";
import { api, RunSummary } from "@/lib/api";

export default function RunsPage() {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.listRuns(100).then(setRuns).finally(() => setLoading(false));
  }, []);

  return (
    <div className="max-w-6xl mx-auto flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">All Runs</h1>
        <Link
          href="/orchestrate"
          className="px-4 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-light)] hover:text-black transition-colors"
        >
          + New Run
        </Link>
      </div>

      <div className="card overflow-x-auto">
        {loading && <p className="text-[var(--muted)] text-sm">Loading…</p>}
        {!loading && runs.length === 0 && (
          <p className="text-[var(--muted)] text-sm">No runs yet.</p>
        )}
        {runs.length > 0 && (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-[var(--muted)] text-xs uppercase tracking-wider border-b border-[var(--card-border)]">
                <th className="text-left py-2 pr-4">Status</th>
                <th className="text-left py-2 pr-4">Goal</th>
                <th className="text-right py-2 pr-4">Cost</th>
                <th className="text-right py-2 pr-4">Tokens</th>
                <th className="text-right py-2 pr-4">Wall</th>
                <th className="text-right py-2">Time</th>
              </tr>
            </thead>
            <tbody>
              {runs.map(run => (
                <tr key={run.run_id} className="border-t border-[var(--card-border)] hover:bg-white/5">
                  <td className="py-2 pr-4">
                    <span className={`badge badge-${run.status}`}>{run.status}</span>
                  </td>
                  <td className="py-2 pr-4 max-w-xs">
                    <Link href={`/runs/${run.run_id}`} className="hover:text-[var(--accent-light)] truncate block">
                      {run.goal}
                    </Link>
                  </td>
                  <td className="py-2 pr-4 text-right font-mono">${run.total_cost_usd.toFixed(5)}</td>
                  <td className="py-2 pr-4 text-right font-mono">{run.total_tokens.toLocaleString()}</td>
                  <td className="py-2 pr-4 text-right font-mono">
                    {run.wall_ms != null ? `${run.wall_ms}ms` : "—"}
                  </td>
                  <td className="py-2 text-right text-[var(--muted)]">
                    {new Date(run.created_at).toLocaleString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
