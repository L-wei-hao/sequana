export type NodeType =
  | "manual_trigger"
  | "webhook"
  | "respond_to_webhook"
  | "http_request"
  | "open_ai"
  | "postgres"
  | "set"
  | "if"
  | "switch"
  | "schedule";

export interface NodePosition {
  x: number;
  y: number;
}

export interface BackendNode {
  id: string;
  node_type: NodeType;
  config: unknown;
  position?: NodePosition;
}

export interface BackendEdge {
  source: string;
  target: string;
  route?: string | null;
}

export interface WorkflowDefinition {
  nodes: BackendNode[];
  edges: BackendEdge[];
}

export interface WorkflowSummary {
  id: string;
  name: string;
  description?: string | null;
  active: boolean;
  active_version_id: string | null;
  latest_version_id: string;
  latest_version: number;
  updated_at: string;
}

export interface WorkflowDetail extends WorkflowSummary {
  definition: WorkflowDefinition;
}

export interface CredentialSummary {
  id: string;
  name: string;
  kind: "openai" | "postgres" | "http_bearer" | "http_basic" | "http_header" | string;
  created_at: string;
  updated_at: string;
}

export interface ExecutionSummary {
  id: string;
  workflow_id: string;
  workflow_version_id: string;
  trigger_type: string;
  trigger_node_id: string;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled" | string;
  error: string | null;
  created_at: string;
  duration_ms: number | null;
}

export interface ExecutionStep {
  id: string;
  node_id: string;
  node_type: string;
  status: "running" | "succeeded" | "failed" | "skipped" | "cancelled" | string;
  input: unknown;
  output: unknown;
  error: string | null;
  duration_ms: number | null;
  metadata?: unknown;
  started_at: string;
  finished_at: string | null;
}

export interface ExecutionDetail extends ExecutionSummary {
  input: unknown;
  output: unknown;
  retry_of_execution_id?: string | null;
  started_at: string | null;
  finished_at: string | null;
  steps: ExecutionStep[];
}
