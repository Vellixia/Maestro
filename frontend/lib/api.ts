const BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3456";

export interface RunSummary {
  run_id: string;
  goal: string;
  status: "running" | "completed" | "failed";
  total_cost_usd: number;
  total_tokens: number;
  wall_ms: number | null;
  created_at: string;
  completed_at: string | null;
}

export interface TraceEntry {
  event_type: string;
  data: Record<string, unknown>;
  ts: string;
}

export interface ConnectionProfile {
  connection_id: string;
  model_id: string;
  skills: Record<string, { score: number; confidence: number }>;
  hard: Record<string, unknown>;
  ops: Record<string, unknown>;
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

export const api = {
  listRuns: (limit = 50) =>
    get<RunSummary[]>(`/admin/runs?limit=${limit}`),

  getRun: (id: string) =>
    get<RunSummary>(`/admin/runs/${id}`),

  getRunTrace: (id: string) =>
    get<TraceEntry[]>(`/admin/runs/${id}/trace`),

  listProfiles: (connectionId: string) =>
    get<ConnectionProfile[]>(`/admin/connections/${connectionId}/profiles`),

  orchestrate: async (goal: string, stream = false) => {
    const res = await fetch(`${BASE}/v1/orchestrate`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ goal, stream }),
    });
    if (!res.ok) throw new Error(`${res.status}`);
    return res.json();
  },

  orchestrateStream: (goal: string): EventSource => {
    // POST via EventSource isn't supported natively; use fetch + ReadableStream instead.
    throw new Error("Use orchestrateStreamFetch");
  },

  orchestrateStreamFetch: async (
    goal: string,
    onEvent: (ev: TraceEntry) => void,
    onDone: (result: string) => void,
  ) => {
    const res = await fetch(`${BASE}/v1/orchestrate`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ goal, stream: true }),
    });
    if (!res.ok || !res.body) throw new Error(`${res.status}`);

    const reader = res.body.getReader();
    const dec = new TextDecoder();
    let buf = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += dec.decode(value, { stream: true });
      const lines = buf.split("\n");
      buf = lines.pop() ?? "";
      for (const line of lines) {
        if (line.startsWith("data: ")) {
          const payload = line.slice(6).trim();
          if (payload === "[DONE]") {
            onDone("");
            return;
          }
          try {
            const ev = JSON.parse(payload);
            onEvent(ev as TraceEntry);
            if (ev.type === "run_completed") {
              onDone(ev.result ?? "");
            }
          } catch {}
        }
      }
    }
  },
};
