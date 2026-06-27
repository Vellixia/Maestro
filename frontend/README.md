# Maestro Frontend

Next.js dashboard for Maestro — capability-aware LLM orchestration.

- **Dashboard** — run history, cost, and token usage overview
- **Orchestrate** — submit goals and watch live DAG execution traces
- **Runs** — per-run trace viewer with React Flow visualization

## Quick Start

```bash
npm install
npm run dev
```

Opens at [http://localhost:3000](http://localhost:3000). Requires the Maestro API running on port 3456.

Set `NEXT_PUBLIC_API_URL` for a custom API endpoint.

## Stack

Next.js 16, React 19, React Flow (DAG traces), Recharts (dashboards), Tailwind CSS 4.
