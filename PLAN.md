# Sequana V1 Development Plan

> **Lightweight workflow automation, built for AI.**

Sequana V1 has one deliberately narrow goal:

> **Replace the n8n workflows we actually run today, not replace n8n as a product.**

Sequana should be small, predictable, easy to deploy, and easy to debug.

The architecture stays intentionally boring:

- One Rust backend
- One React frontend
- One PostgreSQL database
- No microservices
- No Redis
- No RabbitMQ
- No Kafka
- No Kubernetes
- No distributed workers

The V1 definition of success is not feature count. It is the ability to reliably run real production workflows involving webhooks, branching, OpenAI, HTTP APIs, PostgreSQL, scheduled execution, credentials, execution history, retries, and restart recovery.

---

# 1. Product Goals

Sequana V1 must support the workflow patterns currently used in production automation.

Typical target workflow:

```text
Webhook
   ↓
Set / Transform
   ↓
OpenAI
   ↓
If / Switch
   ↓
HTTP / PostgreSQL
   ↓
Respond to Webhook
```

Sequana must provide:

- Visual workflow editing
- Manual workflow execution
- Webhook-triggered workflows
- Scheduled workflows
- HTTP requests
- OpenAI Responses API integration
- Structured OpenAI output
- PostgreSQL access
- JSON transformation
- Conditional branching
- Multi-route branching
- Secure credential storage
- Execution history
- Per-node execution history
- Manual retry
- Execution cancellation
- Background execution
- Restart recovery
- Duplicate webhook protection
- Docker deployment

---

# 2. V1 Architecture

```text
                         Sequana

┌─────────────────────────────────────────────┐
│ React + TypeScript                          │
│ React Flow                                  │
│                                             │
│        Visual Workflow Editor               │
└──────────────────┬──────────────────────────┘
                   │ REST
                   ▼
┌─────────────────────────────────────────────┐
│ Rust                                        │
│ Axum + Tokio + Serde                        │
│                                             │
│ API                                         │
│ Workflow Engine                             │
│ Webhook Handler                             │
│ Scheduler                                   │
│ Worker                                      │
│ Credential Manager                          │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
             PostgreSQL
       ┌──────────┼──────────┐
       ▼          ▼          ▼
   workflows  executions  credentials
```

The Rust service contains:

```text
API
Webhook Handler
Workflow Engine
Scheduler
Worker
Credential Management
Execution Management
```

No part of the backend should be split into a separate service during V1.

---

# 3. Technology Stack

## Backend

```text
Rust
Axum
Tokio
Serde
SQLx
Reqwest
Tracing
Chrono
UUID
```

## Frontend

```text
React
TypeScript
React Flow
```

## Database

```text
PostgreSQL 16+
JSONB workflow definitions
```

## Deployment

```text
Docker
Docker Compose
Caddy
```

---

# 4. V1 Nodes

Sequana V1 must implement the following nodes.

| Node | Purpose |
|---|---|
| Manual Trigger | Manually execute workflows for testing |
| Webhook | Receive HTTP requests |
| Respond to Webhook | Return an HTTP response |
| HTTP Request | Call external APIs |
| OpenAI | Call the OpenAI Responses API |
| PostgreSQL | Execute SQL queries and commands |
| Set / Transform | Map and modify JSON values |
| If | Boolean branching |
| Switch | Multi-route branching |
| Schedule | Cron-based workflow execution |

Explicit looping is not required in V1.

Arrays may pass between nodes naturally. An explicit Loop node should only be added when a real workflow requires loop semantics.

---

# 5. Workflow Model

Workflow definitions are stored as JSONB.

Example:

```json
{
  "nodes": [
    {
      "id": "webhook_1",
      "type": "webhook",
      "position": {
        "x": 100,
        "y": 100
      },
      "config": {}
    },
    {
      "id": "openai_1",
      "type": "openai",
      "position": {
        "x": 400,
        "y": 100
      },
      "config": {
        "credential_id": "cred_123"
      }
    }
  ],
  "edges": [
    {
      "source": "webhook_1",
      "target": "openai_1"
    }
  ]
}
```

Workflow definitions must remain data, not compiled code.

---

# 6. Core Database Tables

V1 uses five core tables.

```text
workflows
workflow_versions
executions
execution_steps
credentials
```

Do not expand the schema unnecessarily.

---

## 6.1 workflows

Suggested fields:

```text
id
tenant_id
name
description
definition JSONB
active
created_at
updated_at
created_by
```

---

## 6.2 workflow_versions

Workflow changes must be versioned.

Suggested fields:

```text
id
workflow_id
version
definition JSONB
created_at
created_by
```

A new workflow version should be created whenever a workflow is saved.

Executions must record the workflow version they ran.

---

## 6.3 executions

Suggested fields:

```text
id
workflow_id
workflow_version_id
tenant_id
trigger_type
status
started_at
finished_at
input JSONB
output JSONB
error JSONB
idempotency_key
created_at
updated_at
```

Execution statuses:

```text
queued
running
success
failed
cancelled
```

---

## 6.4 execution_steps

Suggested fields:

```text
id
execution_id
node_id
node_type
status
started_at
finished_at
duration_ms
input JSONB
output JSONB
error JSONB
created_at
```

This table provides node-level debugging.

---

## 6.5 credentials

Suggested fields:

```text
id
tenant_id
name
credential_type
encrypted_value
created_at
updated_at
created_by
```

Workflow definitions must reference credentials by ID.

Example:

```json
{
  "credential_id": "cred_123"
}
```

Never store credentials directly inside workflow JSON.

---

# 7. Execution Model

The execution engine follows this lifecycle:

```text
Workflow JSON
     ↓
Validate
     ↓
Resolve workflow version
     ↓
Find trigger
     ↓
Create execution
     ↓
Execute node
     ↓
Store step input/output
     ↓
Determine next node(s)
     ↓
Execute
     ↓
Complete
```

Every node execution must be persisted.

A failed node must not silently disappear.

The execution history must always show the exact node that failed.

---

# 8. Node Contract

Every executable node follows the same conceptual interface:

```rust
execute(config, input, context) -> result
```

The result must contain:

```text
output
route
metadata
```

Conceptually:

```rust
struct NodeResult {
    output: serde_json::Value,
    route: Option<String>,
    metadata: Option<serde_json::Value>,
}
```

Examples:

```text
Set
route = default
```

```text
If
route = true
```

```text
Switch
route = case_2
```

Avoid large inheritance-style abstractions.

A simple node type enum is sufficient.

```rust
enum NodeType {
    ManualTrigger,
    Webhook,
    Respond,
    Http,
    Set,
    If,
    Switch,
    Postgres,
    OpenAi,
    Schedule,
}
```

---

# 9. Phase 1 - Rust Execution Engine

**Target: approximately 20 hours**

Build the core runtime.

Required capabilities:

- Parse workflow JSON
- Validate workflow structure
- Validate nodes
- Validate edges
- Detect missing nodes
- Detect invalid node references
- Identify trigger node
- Execute nodes
- Persist node input
- Persist node output
- Persist errors
- Resolve outgoing edges
- Handle branching
- Track execution status
- Track execution duration
- Recover incomplete executions after restart

Execution state must be persisted before moving to the next node.

### Phase 1 Acceptance Criteria

A manually triggered workflow containing:

```text
Manual Trigger
   ↓
Set
   ↓
If
   ↓
Set
```

must:

- execute successfully
- persist the execution
- persist every execution step
- show node input/output
- show execution duration
- record failures correctly

---

# 10. Phase 2 - Essential Nodes

**Target: approximately 25 hours**

Implement:

```text
Webhook
Respond to Webhook
HTTP Request
Set
If
Switch
PostgreSQL
```

---

## 10.1 Webhook Node

Must support:

```text
GET
POST
PUT
PATCH
DELETE
```

Webhook configuration:

```text
path
method
authentication
response_mode
```

Webhook trigger must capture:

```text
headers
query parameters
path parameters
body
HTTP method
```

---

## 10.2 Respond to Webhook Node

Must support:

```text
status code
headers
JSON body
text body
```

The original webhook connection should remain open until the Respond node executes or the request timeout is reached.

---

## 10.3 HTTP Request Node

Must support:

```text
GET
POST
PUT
PATCH
DELETE
```

Configuration:

```text
URL
headers
query parameters
body
authentication
timeout
```

Output:

```text
status
headers
body
duration
```

---

## 10.4 Set / Transform Node

Used for JSON mapping and modification.

Must support:

```text
set field
rename field
remove field
nested values
expressions
```

Example:

```text
{{$json.candidate.name}}
```

Expression syntax should remain deliberately small in V1.

Do not create a JavaScript runtime.

---

## 10.5 If Node

Must support comparisons such as:

```text
equals
not equals
greater than
greater than or equal
less than
less than or equal
contains
exists
is empty
```

Routes:

```text
true
false
```

---

## 10.6 Switch Node

Supports multiple branches.

Example:

```text
status = approved → approved
status = rejected → rejected
default           → default
```

---

## 10.7 PostgreSQL Node

Must support:

```text
SELECT
INSERT
UPDATE
DELETE
raw SQL
parameterized queries
```

Credential fields:

```text
host
port
database
username
password
SSL mode
```

Queries must support parameters.

Avoid string interpolation for SQL values.

---

# 11. Phase 3 - OpenAI Node

**Target: approximately 10 to 15 hours**

The OpenAI node is a V1 requirement.

It must support the OpenAI Responses API.

Configuration:

```text
Credential
Model
Instructions
Input
Reasoning effort
Max output tokens
Output mode
JSON Schema
```

Output modes:

```text
Text
Structured JSON
```

Usage metadata:

```text
input tokens
cached tokens
output tokens
model
response ID
```

Example workflow:

```text
Webhook
   ↓
OpenAI
   ↓
PostgreSQL
   ↓
Respond
```

Token usage must be captured automatically by the engine.

Users must not need a separate "Record Token Usage" node.

Execution step metadata should store:

```text
provider
model
response_id
input_tokens
cached_tokens
output_tokens
```

---

# 12. Phase 4 - Visual Workflow Editor

**Target: approximately 25 to 30 hours**

Use:

```text
React
TypeScript
React Flow
```

Do not build the graph editor from scratch.

Suggested layout:

```text
┌────────────────────────────────────────────────────┐
│ Sequana                    Candidate Profiling     │
├─────────────┬──────────────────────────┬───────────┤
│ Nodes       │                          │ Settings  │
│             │      ┌─────────┐         │           │
│ Trigger     │      │ Webhook │         │ Model     │
│ Webhook     │      └────┬────┘         │ GPT...    │
│ HTTP        │           │              │           │
│ OpenAI      │           ▼              │ Prompt    │
│ Postgres    │      ┌─────────┐         │ ...       │
│ If          │      │ OpenAI  │         │           │
│ Switch      │      └────┬────┘         │           │
│ Set         │           │              │           │
│             │           ▼              │           │
│             │      ┌──────────┐        │           │
│             │      │ Postgres │        │           │
└─────────────┴──────────────────────────┴───────────┘
```

V1 editor capabilities:

```text
Add node
Delete node
Connect nodes
Disconnect nodes
Move nodes
Edit configuration
Save workflow
Activate workflow
Deactivate workflow
Test node
Run workflow
```

Do not spend V1 effort on visual polish that does not improve functionality.

No custom animations are required.

A minimap may be included if provided essentially for free by React Flow.

---

# 13. Phase 5 - Credential Management

**Target: approximately 8 to 10 hours**

Credentials are security-sensitive and must not be treated as plain workflow data.

Flow:

```text
OpenAI Prod
      ↓
credential ID
      ↓
encrypted database value
      ↓
Rust decrypts
      ↓
OpenAI
```

Workflow definitions contain:

```json
{
  "credential_id": "cred_123"
}
```

Never:

```json
{
  "api_key": "sk-..."
}
```

Credentials must be tenant-scoped.

Required credential types for V1:

```text
OpenAI API key
PostgreSQL connection
HTTP Bearer token
HTTP Basic auth
HTTP Header auth
```

Credential values must never be returned to the frontend after creation.

The UI may display:

```text
credential name
credential type
created date
updated date
```

but never decrypted secrets.

---

# 14. Phase 6 - Execution Management

**Target: approximately 15 hours**

Provide an execution history UI.

Required features:

```text
Execution history
Success
Failed
Running
Cancelled
Step output
Step input
Error message
Duration
Manual retry
Cancel execution
```

Example:

```text
Execution #1042

Webhook      ✓  12ms
OpenAI       ✓  3.8s
PostgreSQL   ✓  41ms
Respond      ✓  2ms

Total: 3.9s
```

Users must be able to inspect:

```text
workflow
workflow version
trigger
start time
finish time
total duration
status
input
output
error
individual node steps
```

---

## 14.1 Manual Retry

Retrying an execution should create a new execution.

The original execution remains unchanged.

The new execution should reference the source execution.

Suggested field:

```text
retry_of_execution_id
```

---

## 14.2 Cancel Execution

Cancellation must:

- mark the execution as cancellation requested
- prevent future nodes from starting
- attempt to abort cancellable in-flight operations
- mark the final state as cancelled

External side effects that already completed cannot be undone automatically.

---

# 15. Phase 7 - Scheduler and Background Worker

**Target: approximately 10 hours**

Use PostgreSQL as the queue.

Do not add Redis, RabbitMQ, or Kafka.

Create a `jobs` table.

Suggested fields:

```text
id
workflow_id
workflow_version_id
run_at
status
attempts
last_error
created_at
updated_at
```

Job statuses:

```text
queued
running
completed
failed
cancelled
```

Workers claim jobs using PostgreSQL row locking.

Example:

```sql
SELECT *
FROM jobs
WHERE status = 'queued'
  AND run_at <= NOW()
ORDER BY run_at
FOR UPDATE SKIP LOCKED
LIMIT 1;
```

Tokio workers execute claimed jobs.

---

## 15.1 Schedule Node

Must support cron-based schedules.

Examples:

```text
Every day at 10:00
Every hour
Every Monday at 09:00
Custom cron
```

Timezone must be explicitly stored.

Default deployment timezone:

```text
Asia/Singapore
```

---

# 16. Phase 8 - Deployment

**Target: approximately 8 to 10 hours**

Docker Compose stack:

```text
caddy
frontend
sequana
postgres
```

Example:

```yaml
services:
  sequana:
    image: sequana
    restart: unless-stopped

  frontend:
    image: sequana-ui
    restart: unless-stopped

  postgres:
    image: postgres:16
    restart: unless-stopped

  caddy:
    image: caddy
    restart: unless-stopped
```

The Rust application contains:

```text
API
Webhooks
Scheduler
Worker
Workflow execution
Credential management
```

Do not split these responsibilities into multiple application containers during V1.

---

# 17. API Surface

V1 should expose a small REST API.

Suggested endpoints:

```text
GET    /api/workflows
POST   /api/workflows
GET    /api/workflows/:id
PUT    /api/workflows/:id
DELETE /api/workflows/:id

POST   /api/workflows/:id/activate
POST   /api/workflows/:id/deactivate
POST   /api/workflows/:id/run

GET    /api/executions
GET    /api/executions/:id
POST   /api/executions/:id/retry
POST   /api/executions/:id/cancel

GET    /api/credentials
POST   /api/credentials
PUT    /api/credentials/:id
DELETE /api/credentials/:id

ANY    /webhook/:workflow_path
```

Avoid adding APIs until the frontend or runtime actually needs them.

---

# 18. Multi-Tenancy

Sequana must be tenant-aware from the beginning.

Tenant-scoped data:

```text
workflows
workflow_versions
executions
execution_steps
credentials
jobs
```

Every request must resolve a tenant before accessing tenant-owned resources.

Never trust a tenant ID supplied only by the browser.

Tenant identity must come from the authenticated backend context.

---

# 19. Security Requirements

V1 must include:

- encrypted credentials at rest
- tenant isolation
- authentication
- authorization
- parameterized SQL
- request size limits
- webhook input validation
- webhook authentication options
- SSRF protection for HTTP Request nodes
- secret masking in logs
- secret masking in execution output
- secure error handling
- restricted credential API responses

Do not log:

```text
API keys
Authorization headers
passwords
database connection passwords
credential decrypted values
```

---

# 20. Webhook Idempotency

Sequana must support duplicate webhook protection.

Clients may send:

```text
Idempotency-Key: abc123
```

The idempotency key should be associated with:

```text
workflow_id
idempotency_key
```

A duplicate request with the same key must not create a second execution.

If no idempotency key is supplied, requests are treated as independent executions.

---

# 21. Error Handling

Errors must be structured.

Suggested error shape:

```json
{
  "code": "OPENAI_TIMEOUT",
  "message": "OpenAI request timed out",
  "node_id": "openai_1",
  "retryable": true
}
```

Error categories should include:

```text
validation
authentication
authorization
timeout
network
provider
database
configuration
execution
cancelled
```

Failures must be persisted before the execution terminates.

---

# 22. Logging and Observability

Use structured logs.

Each execution log should include:

```text
execution_id
workflow_id
tenant_id
node_id
node_type
duration
status
```

Do not create a separate observability platform for V1.

Application logs plus execution history are sufficient.

---

# 23. Restart Recovery

Sequana must survive server restarts.

On startup:

1. identify executions left in `running`
2. identify jobs left in `running`
3. determine whether they can safely resume
4. otherwise mark them failed with a restart-related error
5. retain complete execution history

No execution should remain permanently stuck in `running`.

---

# 24. V1 Acceptance Workflow

The main definition of done is this workflow:

```text
POST /webhook/candidate
          ↓
       Webhook
          ↓
         Set
          ↓
       OpenAI
     JSON Schema
          ↓
          If
       ↙      ↘
    valid    invalid
      ↓          ↓
 PostgreSQL    Respond 400
      ↓
 Respond 200
```

The workflow must survive:

```text
OpenAI timeout
OpenAI provider error
invalid JSON
invalid structured output
PostgreSQL error
HTTP timeout
workflow restart
server restart
duplicate webhook
manual retry
execution cancellation
```

Execution history must make it obvious where and why the workflow failed.

If this workflow works reliably, V1 is done.

---

# 25. Testing Strategy

Do not build a huge testing framework.

Every non-trivial subsystem must leave behind at least one runnable check.

Minimum required tests:

```text
Workflow validation
Sequential execution
If branching
Switch branching
Webhook execution
Webhook idempotency
HTTP timeout
PostgreSQL query
OpenAI mocked response
OpenAI structured output failure
Credential encryption/decryption
Scheduler claim
SKIP LOCKED worker claim
Execution retry
Execution cancellation
Restart recovery
Tenant isolation
```

Integration testing is more valuable than large quantities of unit tests for the engine.

---

# 26. Development Order

Recommended implementation order:

```text
1. Project scaffold
2. Database schema
3. Workflow model
4. Workflow validation
5. Execution engine
6. Execution persistence
7. Manual Trigger
8. Set
9. If
10. Switch
11. Webhook
12. Respond to Webhook
13. HTTP Request
14. PostgreSQL
15. Credentials
16. OpenAI
17. Workflow API
18. React editor
19. Execution history UI
20. Retry
21. Cancel
22. Scheduler
23. Worker
24. Restart recovery
25. Docker deployment
26. Acceptance workflow
```

Build vertically whenever possible.

Do not build the entire backend before proving a simple workflow can run end-to-end.

---

# 27. Repository Structure

Sequana should remain a single monorepo.

Do not create separate repositories for the frontend, backend, workers, scheduler, or deployment configuration during V1.

Target structure:

```text
sequana/
├── README.md
├── PLAN.md
├── .env.example
├── .gitignore
├── compose.yaml
├── Caddyfile
│
├── backend/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── Dockerfile
│   ├── .dockerignore
│   │
│   ├── migrations/
│   │   └── 001_init.sql
│   │
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── error.rs
│   │   ├── state.rs
│   │   │
│   │   ├── api/
│   │   │   ├── mod.rs
│   │   │   ├── workflows.rs
│   │   │   ├── executions.rs
│   │   │   ├── credentials.rs
│   │   │   └── webhooks.rs
│   │   │
│   │   ├── engine/
│   │   │   ├── mod.rs
│   │   │   ├── model.rs
│   │   │   ├── validate.rs
│   │   │   └── execute.rs
│   │   │
│   │   ├── nodes/
│   │   │   ├── mod.rs
│   │   │   ├── manual.rs
│   │   │   ├── webhook.rs
│   │   │   ├── respond.rs
│   │   │   ├── http.rs
│   │   │   ├── openai.rs
│   │   │   ├── postgres.rs
│   │   │   ├── set.rs
│   │   │   ├── if_node.rs
│   │   │   ├── switch.rs
│   │   │   └── schedule.rs
│   │   │
│   │   ├── credentials.rs
│   │   ├── scheduler.rs
│   │   └── worker.rs
│   │
│   └── tests/
│       └── acceptance.rs
│
└── frontend/
    ├── package.json
    ├── package-lock.json
    ├── tsconfig.json
    ├── vite.config.ts
    ├── index.html
    ├── Dockerfile
    ├── .dockerignore
    │
    └── src/
        ├── main.tsx
        ├── App.tsx
        ├── api.ts
        ├── types.ts
        ├── styles.css
        │
        ├── pages/
        │   ├── WorkflowsPage.tsx
        │   ├── WorkflowEditorPage.tsx
        │   ├── ExecutionsPage.tsx
        │   ├── ExecutionDetailPage.tsx
        │   └── CredentialsPage.tsx
        │
        ├── workflow/
        │   ├── WorkflowCanvas.tsx
        │   ├── NodePalette.tsx
        │   ├── NodeSettings.tsx
        │   ├── WorkflowNode.tsx
        │   ├── nodeCatalog.ts
        │   └── settings/
        │       ├── WebhookSettings.tsx
        │       ├── RespondSettings.tsx
        │       ├── HttpSettings.tsx
        │       ├── OpenAiSettings.tsx
        │       ├── PostgresSettings.tsx
        │       ├── SetSettings.tsx
        │       ├── IfSettings.tsx
        │       ├── SwitchSettings.tsx
        │       └── ScheduleSettings.tsx
        │
        ├── executions/
        │   └── ExecutionSteps.tsx
        │
        └── credentials/
            └── CredentialForm.tsx
```

This is the target structure, not a requirement to create every file on day one.

Create files only when the feature that owns them is implemented.

---

## 27.1 Root Responsibilities

The repository root owns only application-wide concerns.

```text
README.md       developer setup and basic usage
PLAN.md         V1 implementation plan
.env.example    documented environment variables
compose.yaml    local/production-style service composition
Caddyfile       reverse proxy routing
.gitignore      repository-wide generated files and secrets
```

Do not create extra root folders such as:

```text
common/
shared/
packages/
libs/
platform/
infrastructure/
scripts/
tools/
```

unless a concrete requirement appears.

For V1, they add navigation cost without solving a real problem.

---

## 27.2 Backend Boundaries

The backend is one Rust crate and produces one Sequana binary.

```text
backend/src/main.rs
```

owns process startup only:

```text
load configuration
create PostgreSQL pool
build shared application state
start scheduler/worker tasks
build Axum router
start HTTP server
```

Business logic should not accumulate in `main.rs`.

---

### api/

`api/` owns HTTP transport concerns.

It may:

```text
parse requests
authenticate requests
resolve tenant context
validate HTTP-level input
call the engine or database operations
map results to HTTP responses
```

It should not contain workflow execution logic.

Route files are grouped by resource rather than by HTTP verb.

Example:

```text
api/workflows.rs

list_workflows
get_workflow
create_workflow
update_workflow
delete_workflow
activate_workflow
run_workflow
```

Do not create controller/service/repository layers around these handlers unless duplication actually appears.

---

### engine/

`engine/` owns workflow semantics.

```text
model.rs      workflow/node/edge/runtime types
validate.rs   graph and configuration validation
execute.rs    execution traversal and routing
mod.rs        public engine surface
```

The engine is the one place responsible for:

```text
finding the next node
branch routing
execution state transitions
step recording
failure propagation
cancellation checks
```

Do not duplicate execution rules inside API handlers, nodes, scheduler code, or workers.

---

### nodes/

`nodes/` owns node-specific behavior.

Each node implementation should contain only the logic unique to that node.

Shared engine concerns stay in `engine/`.

Examples:

```text
openai.rs     build OpenAI request and normalize response
http.rs       execute configured HTTP request
postgres.rs   execute parameterized SQL
if_node.rs    evaluate configured condition
switch.rs     select configured route
```

Do not create a separate crate, package, or plugin system for V1 nodes.

A node becomes a module and an enum variant.

---

### credentials.rs

Credential encryption/decryption stays in one module until its size proves otherwise.

It owns:

```text
encrypt
decrypt
mask
credential type validation
```

Secret values must not leak into normal API response models.

Do not create a generic secrets framework in V1.

---

### scheduler.rs

The scheduler owns only schedule discovery and job creation.

It must not execute workflows directly.

Conceptually:

```text
find due schedules
    ↓
create queued job
```

---

### worker.rs

The worker owns job claiming and dispatch.

Conceptually:

```text
claim queued job
    ↓
create/start execution
    ↓
call workflow engine
    ↓
finalize job state
```

The actual workflow traversal still belongs to `engine/`.

---

### state.rs

`state.rs` contains the small set of objects shared by Axum handlers and background tasks.

Expected contents:

```text
PostgreSQL pool
configuration
HTTP client
credential encryption key/context
shutdown/cancellation state
```

Do not turn `AppState` into a service locator containing dozens of unrelated objects.

---

### config.rs

`config.rs` is the single environment-variable boundary.

Read environment variables once during startup and convert them into typed configuration.

Application code should consume `Config`, not repeatedly call environment APIs.

---

### error.rs

Use one application error type unless a subsystem genuinely requires a separate one.

It should provide consistent handling for:

```text
validation errors
database errors
provider errors
timeouts
authentication errors
authorization errors
cancellation
internal errors
```

Avoid one custom error enum per folder.

---

## 27.3 Database Ownership

All V1 schema creation remains in:

```text
backend/migrations/001_init.sql
```

During active V1 development, amend `001_init.sql` directly.

Do not create:

```text
002_add_x.sql
003_fix_x.sql
004_really_fix_x.sql
005_final_fix_x.sql
```

while the initial schema is still being designed.

Start additive migrations only after the initial schema is considered released and must preserve deployed databases.

The database should enforce rules that naturally belong in PostgreSQL, including:

```text
foreign keys
unique constraints
NOT NULL constraints
tenant-scoped uniqueness
idempotency uniqueness
status checks where useful
```

Do not reproduce database constraints only in application code.

---

## 27.4 Frontend Boundaries

The frontend is one React application.

Do not create a frontend monorepo or package workspace for V1.

---

### pages/

`pages/` owns route-level screens.

A page composes feature components and performs page-level data loading.

Pages should not contain React Flow node implementations or large configuration forms.

---

### workflow/

`workflow/` owns the visual editor.

```text
WorkflowCanvas.tsx   React Flow canvas and graph events
NodePalette.tsx      available node list
NodeSettings.tsx     selected-node settings host
WorkflowNode.tsx     shared visual node shell
nodeCatalog.ts       node labels/icons/category metadata
settings/            node-specific configuration forms
```

Use one generic `WorkflowNode` visual shell unless a node genuinely needs a different appearance.

Do not create ten visually identical React node components just because there are ten backend node types.

Node-specific configuration remains separate because the forms are actually different.

---

### executions/

Execution-specific UI stays here.

Start with:

```text
ExecutionSteps.tsx
```

Split further only when the execution UI becomes large enough to justify it.

---

### credentials/

Credential-specific UI stays here.

Start with:

```text
CredentialForm.tsx
```

Do not create a frontend credential framework.

---

### api.ts

Use one small API client module for V1.

It should own:

```text
base URL
JSON request/response handling
authentication headers
common API error parsing
```

Do not introduce generated clients, GraphQL, Redux middleware, or a custom networking abstraction unless the application later needs them.

---

### types.ts

Keep shared frontend API/domain types in one file initially.

Split them by domain only when `types.ts` becomes genuinely difficult to navigate.

Do not introduce a separate shared Rust/TypeScript schema package in V1.

---

## 27.5 Tests

Testing should follow the feature rather than mirror the entire source tree.

Backend module-level tests can live beside the code they test.

Use:

```text
backend/tests/acceptance.rs
```

for the small number of end-to-end runtime checks that cross multiple modules.

The acceptance test should eventually cover the V1 reference workflow:

```text
Webhook
   ↓
Set
   ↓
OpenAI
   ↓
If
   ↓
PostgreSQL
   ↓
Respond
```

Do not create a large test folder hierarchy before tests exist.

Frontend automated tests are not required merely to mirror backend coverage.

Add them when UI logic becomes non-trivial or a regression proves they are valuable.

---

## 27.6 Docker Layout

Each deployable application owns its Dockerfile.

```text
backend/Dockerfile
frontend/Dockerfile
```

The repository root owns orchestration:

```text
compose.yaml
Caddyfile
```

Expected services remain:

```text
caddy
frontend
sequana
postgres
```

Caddy routing should remain simple:

```text
/api/*       → sequana
/webhook/*   → sequana
/*           → frontend
```

Do not add an API gateway.

---

## 27.7 Dependency Ownership

Keep dependency management local to each application.

```text
backend/Cargo.toml
frontend/package.json
```

Commit application lock files:

```text
backend/Cargo.lock
frontend/package-lock.json
```

Before adding a dependency:

1. check whether the standard library already covers it
2. check whether an existing dependency already covers it
3. add the dependency only if it materially reduces maintained code

Avoid convenience dependencies for trivial helpers.

---

## 27.8 Generated and Local Files

Do not commit:

```text
backend/target/
frontend/node_modules/
frontend/dist/
.env
logs/
temporary execution files
editor-specific local state
```

Do commit:

```text
.env.example
Cargo.lock
package-lock.json
001_init.sql
```

---

## 27.9 Structure Rules

The following rules apply throughout V1:

1. **One backend crate.**
   Do not split the Rust backend into workspace crates.

2. **One frontend app.**
   Do not introduce frontend packages or a component library.

3. **One initial migration.**
   Keep the V1 schema in `001_init.sql` until the first released schema must be preserved.

4. **No generic `utils` dumping ground.**
   A helper stays next to the feature that uses it until two real callers justify moving it.

5. **No service/repository ceremony.**
   Add a layer only when it removes actual duplication or enforces a real boundary.

6. **No plugin architecture for nodes.**
   V1 nodes are Rust modules registered in the existing node enum/dispatcher.

7. **No premature shared package between Rust and TypeScript.**
   Small duplicated transport types are cheaper than maintaining code generation during V1.

8. **No empty scaffolding.**
   The tree above describes where code belongs once needed. Do not create placeholder files for future features.

9. **Keep cross-cutting behavior centralized.**
   Execution state transitions belong in the engine, credentials in the credential module, configuration in `config.rs`, and HTTP transport in `api/`.

10. **Split files because they became hard to work with, not because an architecture diagram says they should be split.**

The repository should remain understandable from the directory tree alone.

# 28. Environment Variables

Suggested V1 variables:

```text
DATABASE_URL
SEQUANA_ENCRYPTION_KEY
SEQUANA_BASE_URL
SEQUANA_WEBHOOK_BASE_URL
SEQUANA_LOG_LEVEL
SEQUANA_WORKER_CONCURRENCY
SEQUANA_DEFAULT_TIMEZONE
```

Secrets should never be committed.

Provide `.env.example` with placeholders only.

---

# 29. V1 UI Pages

Required pages:

```text
Login
Workflow List
Workflow Editor
Execution List
Execution Detail
Credentials
```

Optional dashboard metrics may be added only if implementation is trivial.

---

# 30. V1 Non-Goals

The following are explicitly out of scope:

```text
JavaScript Code node
Python Code node
Plugin marketplace
Custom node SDK
Redis
RabbitMQ
Kafka
Kubernetes
Distributed workers
Workflow collaboration
Git integration
AI workflow builder
Hundreds of integrations
Vector database support
Sub-workflows
Advanced debugger
Mobile UI
Loop node
```

These features may be considered after V1 based on actual usage.

---

# 31. Estimated Effort

| Area | Hours |
|---|---:|
| Execution Engine | 20 |
| Core Nodes | 25 |
| OpenAI Node | 12 |
| Visual Editor | 28 |
| Credentials | 10 |
| Execution Management | 15 |
| Scheduler / Worker | 10 |
| Deployment / Testing | 10 |
| **Total** | **~130 hours** |

Recommended planning range:

```text
110 to 150 hours
```

The 130-hour figure should remain the official V1 planning estimate.

AI-assisted coding may reduce implementation time, but it should not reduce the amount of validation required around:

```text
workflow semantics
failure handling
concurrency
database transactions
credential security
webhook lifecycle
restart recovery
tenant isolation
```

---

# 32. Phase Completion Gates

A phase is complete only when:

```text
implementation works
database changes are committed
errors are handled
a runnable check exists
existing workflows still work
documentation is updated
```

Do not move to the next phase with known engine-level failures.

---

# 33. Definition of Done

Sequana V1 is complete when:

1. A user can visually create a workflow.
2. A user can configure all V1 nodes.
3. A workflow can be saved and versioned.
4. A workflow can be manually executed.
5. A workflow can be triggered by webhook.
6. A workflow can be triggered by schedule.
7. OpenAI structured output works.
8. PostgreSQL queries work.
9. HTTP requests work.
10. If and Switch routing work.
11. Credentials are encrypted and tenant-scoped.
12. Every execution is persisted.
13. Every node execution is inspectable.
14. Failed workflows show the exact failing node.
15. Workflows can be manually retried.
16. Running executions can be cancelled.
17. Duplicate webhook requests can be rejected using an idempotency key.
18. Background workers recover correctly after restart.
19. The acceptance workflow passes all failure scenarios.
20. The complete application runs through Docker Compose.

---

# 34. Guiding Principle

When choosing between two implementations, prefer the one with fewer moving parts.

Before adding infrastructure or abstractions, ask:

```text
Does Sequana actually need this now?
Does Rust or PostgreSQL already solve this?
Does an existing dependency already solve this?
Can the same behavior be implemented in the existing runtime?
```

Sequana V1 should be boring infrastructure.

That is a feature.

---

# 35. V1 Summary

Sequana V1 is:

```text
Webhook
   ↓
Logic
   ↓
OpenAI / HTTP / PostgreSQL
   ↓
Response
```

plus:

```text
Visual editor
Credentials
Execution history
Retries
Cancellation
Scheduling
Background workers
Restart recovery
```

That is enough to begin replacing production n8n workflows without attempting to reproduce the entire n8n ecosystem.

Once Sequana reliably runs the workflows we already depend on, V1 is complete.
