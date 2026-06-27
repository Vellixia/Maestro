# Changelog

## [0.1.0] — 2026-06-27

### Added

- Multi-provider gateway with OpenAI, Anthropic, Gemini native support + OpenAI-compatible long tail
- SurrealDB storage layer (embedded + remote), graph/vector/relational multi-model
- Capability registry with per-connection skill profiles
- Hybrid calibration engine: benchmark priors + auto-graded probe suite + profile fusion
- Goal planner with DAG decomposition (trivial fast-path, plan cache)
- Per-subtask requirement classifier
- Cost-minimizing router: hard filter → dominance match → cost objective → escalation ladder
- `petgraph`-based DAG executor with tokio `JoinSet` parallelism
- Per-output-type verifier with escalation control
- Synthesizer for output composition
- Policy engine (budget, latency, privacy modes)
- OpenAI-compatible `/v1/chat/completions` with `auto` model
- Native `/v1/orchestrate` endpoint with SSE streaming
- Next.js dashboard: run history, orchestration UI, DAG trace viewer (React Flow)
- Docker Compose for full-stack deployment
- CI/CD with GitHub Actions
