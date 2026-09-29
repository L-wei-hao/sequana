# Sequana

Lightweight workflow automation, built for AI.

Sequana V1 is intentionally focused on the workflow patterns used by Braindge today:

```text
trigger -> transform / logic -> OpenAI / HTTP / PostgreSQL -> response
```

## V1 nodes

- Manual Trigger
- Webhook
- Schedule
- Respond to Webhook
- HTTP Request
- OpenAI
- PostgreSQL
- Set / Transform
- If
- Switch

## Architecture

```text
Caddy
  |
  +-- /api, /webhook, /health -> Sequana (Rust / Axum / Tokio)
  |
  +-- everything else -------> React editor

Sequana
  |
  +-- API
  +-- workflow engine
  +-- webhook handler
  +-- scheduler
  +-- PostgreSQL job workers
  |
PostgreSQL
```

One Rust process handles the API, webhooks, scheduler, workers, and workflow execution.

## Run with Docker

```bash
cp .env.example .env
```

Generate the two secrets:

```bash
openssl rand -hex 32
openssl rand -hex 32
```

Put them in:

```text
SEQUANA_CREDENTIAL_KEY=
SEQUANA_ADMIN_TOKEN=
```

Then:

```bash
docker compose up --build
```

Default local URL:

```text
http://localhost
```

For Caddy-managed HTTPS, set:

```text
SEQUANA_ADDRESS=sequana.example.com
```

## Local backend

Required environment variables:

```text
DATABASE_URL=postgres://sequana:password@localhost:5432/sequana
SEQUANA_CREDENTIAL_KEY=<64 hex characters>
SEQUANA_ADMIN_TOKEN=<at least 32 characters>
SEQUANA_WORKERS=2
SEQUANA_MAX_IN_FLIGHT_EXECUTIONS=32
```

`SEQUANA_MAX_IN_FLIGHT_EXECUTIONS` bounds simultaneous manual runs, webhook executions,
retries, and node tests. Saturated execution routes return HTTP 503 with `Retry-After: 1`.

Run:

```bash
cd backend
cargo run
```

## Local frontend

```bash
cd frontend
npm install
npm run dev
```

Vite proxies `/api` and `/webhook` to `localhost:8080`.

## Authentication

The current V1 admin API expects:

```http
Authorization: Bearer <SEQUANA_ADMIN_TOKEN>
x-tenant-id: <tenant UUID>
```

Webhook endpoints do not use the admin token.

## Schedule node

A schedule node config looks like:

```json
{
  "cron": "0 10 * * *",
  "timezone": "Asia/Singapore",
  "input": {
    "source": "daily_schedule"
  }
}
```

Five-field cron expressions are accepted. Six- and seven-field expressions are also supported.

Scheduled work is stored in PostgreSQL and claimed by workers with `FOR UPDATE SKIP LOCKED`.

## OpenAI node

OpenAI API keys are stored as encrypted tenant-scoped credentials. Workflow JSON stores only the credential ID.

Example config:

```json
{
  "credential_id": "00000000-0000-0000-0000-000000000000",
  "model": "gpt-5.6-luna",
  "instructions": "Return a concise candidate profile.",
  "input": {
    "$from": "/cv_text"
  },
  "reasoning_effort": "medium",
  "max_output_tokens": 2000,
  "output": {
    "type": "text"
  }
}
```

The node records the response ID, model, input tokens, cached input tokens, and output tokens in its step output.

## Early V1 migration rule

During early V1 development, the database schema is deliberately kept in one file:

```text
backend/migrations/0001_init.sql
```

Do not add incremental migration files yet.

Because `0001_init.sql` is still being edited, an existing disposable development database may have an old migration checksum. Reset only a disposable local V1 database with:

```bash
docker compose down -v
docker compose up --build
```

Do not use that reset command against a database containing data you need.

## Checks

Backend:

```bash
cd backend
cargo test --all-targets
```

Frontend:

```bash
cd frontend
npm run build
```

GitHub Actions runs both on `development` and `main`.
