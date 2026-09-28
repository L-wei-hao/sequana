CREATE TABLE workflows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    active_version_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE workflow_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version > 0),
    definition JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workflow_id, version)
);

ALTER TABLE workflows
    ADD CONSTRAINT workflows_active_version_fk
    FOREIGN KEY (active_version_id)
    REFERENCES workflow_versions(id)
    ON DELETE SET NULL;

CREATE TABLE executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    workflow_version_id UUID NOT NULL REFERENCES workflow_versions(id),
    trigger_type TEXT NOT NULL,
    trigger_node_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    input JSONB NOT NULL DEFAULT '{}'::jsonb,
    output JSONB,
    error TEXT,
    idempotency_key TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX executions_idempotency_key_uq
    ON executions (workflow_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX executions_workflow_created_idx
    ON executions (workflow_id, created_at DESC);

CREATE INDEX executions_status_idx
    ON executions (status, created_at);

CREATE TABLE execution_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    execution_id UUID NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL,
    node_type TEXT NOT NULL,
    status TEXT NOT NULL
        CHECK (status IN ('running', 'succeeded', 'failed', 'skipped', 'cancelled')),
    input JSONB NOT NULL DEFAULT '{}'::jsonb,
    output JSONB,
    error TEXT,
    duration_ms BIGINT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX execution_steps_execution_idx
    ON execution_steps (execution_id, started_at);

CREATE TABLE credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    encrypted_value BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

CREATE INDEX credentials_tenant_idx
    ON credentials (tenant_id);

CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    workflow_version_id UUID NOT NULL REFERENCES workflow_versions(id),
    trigger_node_id TEXT NOT NULL,
    run_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    execution_id UUID REFERENCES executions(id) ON DELETE SET NULL,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    UNIQUE (workflow_id, workflow_version_id, trigger_node_id, run_at)
);

CREATE INDEX jobs_due_idx
    ON jobs (status, run_at);
