<div align="center">
  <h1>🎼 Maestro</h1>
  <p><strong>Capability-Aware LLM Orchestration Platform</strong></p>
  <p>Register many models across many providers. Measure each model's capability. Then <em>plan</em> a goal, <em>decompose</em> it into a task DAG, and <em>assign each piece to the cheapest model that is good enough</em> — weak/free models do easy work, strong models do hard work — with <strong>verify-and-escalate</strong> to protect quality.</p>
  <br>
</div>

<div align="center">

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![CI](https://github.com/Vellixia/Maestro/actions/workflows/ci.yml/badge.svg)](https://github.com/Vellixia/Maestro/actions/workflows/ci.yml)

</div>

---

## The Problem

AI model APIs are a heterogeneous market: premium models (GPT-4o, Claude Opus, Gemini 2.5 Pro) cost 10–30× more than capable free/cheap models (Gemini Flash, Claude Haiku, Llama 3, DeepSeek). Most applications pay the premium rate for **every request**, even when the task is trivial — classification, extraction, boilerplate, summarization.

**Maestro solves this** by treating every request as a multi-step job, routing each piece to the cheapest model whose capability matches the difficulty, verifying quality, and escalating on failure.

## Key Features

| Feature | Description |
|---|---|
| **Capability Profiles** | Per-provider-connection skill vectors (reasoning, coding, math, tool-use, JSON, etc.) built from benchmark priors + auto-graded probes |
| **Hybrid Calibration** | Benchmark/Elo priors for instant cold-start + lightweight probe suite + online learning from production verification outcomes |
| **Hard-Constraint Filtering** | Context window, modality (vision/audio), tool-calling, structured output — enforced before cost optimization |
| **Cost-Minimizing Router** | Dominance-margin matcher assigns each subtask to the cheapest capable model; respects free-tier rate limits |
| **Verify-and-Escalate** | Per-output-type verifiers (code tests, JSON schema, LLM-judge) with escalation to stronger models on failure |
| **Task DAG Execution** | `petgraph`-scheduled parallelism across independent subtasks, bounded by per-provider concurrency |
| **Full Observability** | Per-subtask trace persisted to SurrealDB with live SSE streaming |
| **OpenAI-Compatible API** | Drop-in `auto` model for existing tools; native `/v1/orchestrate` endpoint |
| **Policy Engine** | Budget caps, latency floors, privacy rules, provider allow/block lists |

## Architecture

```
                 ┌──────────────────────────────────────────────────────────┐
 goal ─▶ API ───▶│ ORCHESTRATION CORE                                       │─▶ answer + trace
 (model="auto"   │  Planner → Classifier → Assignment Engine → DAG Executor  │
  or /orchestrate)│     ▲          │              │            │              │
                 │  plan cache  RequirementProfile │       Verifier+Escalate  │
                 │                              ▼  ▼            │             │
                 │                          Synthesizer ◀───────┘             │
                 └──────┬───────────────────────────────────────┬─────────────┘
                        │ reads capability profiles              │ calls models
            ┌───────────▼──────────────┐          ┌─────────────▼──────────────┐
            │ Capability Registry +    │          │ Provider Gateway           │
            │ Calibration Engine       │          │ native OAI/Claude/Gemini   │
            │ priors → probes →        │          │ + OpenAI-compat tail       │
            │ profile per connection   │          │ + account fallback/RL      │
            └───────────┬──────────────┘          └─────────────┬──────────────┘
                ┌───────▼─────────────────────────────────────▼───────────┐
                │ SurrealDB (graph + vector + relational, ACID, embedded)  │
                │ + Trace UI (Next.js / React Flow)                        │
                └──────────────────────────────────────────────────────────┘
```

### Subsystems

| Crate | Role |
|---|---|
| [`crates/gateway`](crates/gateway) | Multi-provider client (OpenAI, Anthropic, Gemini + OpenAI-compat tail), account fallback, rate-limit cooldown |
| [`crates/registry`](crates/registry) | Model + connection registry, capability profile store |
| [`crates/calibration`](crates/calibration) | Benchmark priors, probe suite, auto-graders (code sandbox, JSON schema, anchor-judge), profile fusion |
| [`crates/planner`](crates/planner) | Goal decomposition → TaskGraph (DAG), trivial fast-path, plan cache |
| [`crates/classifier`](crates/classifier) | Per-subtask requirement profile estimator |
| [`crates/router`](crates/router) | Hard filter → dominance match → cost objective → escalation ladder |
| [`crates/executor`](crates/executor) | `petgraph` topological scheduler, `JoinSet` parallelism, context assembly |
| [`crates/verifier`](crates/verifier) | Per-output-type quality gates, escalation control, stakes-scaling |
| [`crates/synthesizer`](crates/synthesizer) | Output composition, SSE streaming |
| [`crates/policy`](crates/policy) | Budget, latency, privacy, provider allow/block modes |
| [`crates/storage`](crates/storage) | SurrealDB access layer (repos, migrations, vector indexes) |
| [`crates/api`](crates/api) | axum server: OpenAI-compatible drop-in, `/v1/orchestrate`, admin REST, SSE |

## Quick Start

### Prerequisites

- Rust 1.85+
- Docker (for SurrealDB + code sandbox)
- Node.js 20+ (for frontend)

### Run the API

```bash
# Start SurrealDB
docker compose up -d surrealdb

# Start the API (in-memory storage by default)
cargo run --bin maestro

# Or with persistent SurrealDB:
export SURREALDB_URL=ws://localhost:8000
export DB_USER=root
export DB_PASS=root
cargo run --bin maestro
```

### Run the Frontend

```bash
cd frontend
npm install
npm run dev
```

### Docker Compose (full stack)

```bash
docker compose up --build
```

## Configuration

Key environment variables — see [`.env.example`](.env.example) for the full list.

| Variable | Default | Description |
|---|---|---|
| `OPENAI_API_KEY` | — | OpenAI API key |
| `ANTHROPIC_API_KEY` | — | Anthropic API key |
| `GEMINI_API_KEY` | — | Gemini API key |
| `SURREALDB_URL` | (in-memory) | Remote SurrealDB connection |
| `PORT` | `3456` | API server port |
| `REQUIRE_API_KEY` | — | Enable API key auth |
| `JWT_SECRET` | `change-me-in-production` | JWT signing secret |
| `RUST_LOG` | `info` | Log level |

## Build Phases

| Phase | What it delivers |
|---|---|
| **Phase 0** ✅ | Gateway + storage spine + OpenAI-compatible passthrough |
| **Phase 1** | Capability registry + hybrid calibration (priors + probes + fusion) |
| **Phase 2** | Single-task router (`auto` model): classifier + router + verifier + escalation |
| **Phase 3** | Planner + DAG executor + synthesizer (full `/v1/orchestrate`) |
| **Phase 4** | Learning loop + Trace UI + policy/budget modes + caching |
| **Phase 5** | Agentic tools, global ILP optimizer, plan-template library |

## Development

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Run with live tracing
RUST_LOG=maestro=debug cargo run --bin maestro

# Lint
cargo clippy -- -D warnings

# Frontend
cd frontend && npm run dev
```

## License

MIT — see [LICENSE](LICENSE).

---

<p align="center">Built with 🦀 Rust + ⚡ Next.js</p>
