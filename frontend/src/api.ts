import type {
  CredentialSummary,
  ExecutionDetail,
  ExecutionSummary,
  WorkflowDefinition,
  WorkflowDetail,
  WorkflowSummary,
} from "./types";

export interface ApiAuth {
  token: string;
  tenantId: string;
}

export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
    public code?: string,
    public nodeId?: string
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export async function apiRequest<T>(
  path: string,
  auth: ApiAuth,
  init: RequestInit = {}
): Promise<T> {
  const headers = new Headers(init.headers);
  if (auth.token) {
    headers.set("Authorization", `Bearer ${auth.token}`);
  }
  if (auth.tenantId) {
    headers.set("x-tenant-id", auth.tenantId);
  }
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(path, { ...init, headers });
  const text = await response.text();
  let body: any = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }

  if (!response.ok) {
    const errorMsg =
      body && typeof body === "object"
        ? body.error || body.message || `HTTP ${response.status}`
        : typeof body === "string"
        ? body
        : `HTTP ${response.status}`;
    const code = body && typeof body === "object" ? body.code : undefined;
    const nodeId = body && typeof body === "object" ? body.node_id : undefined;
    throw new ApiError(errorMsg, response.status, code, nodeId);
  }

  return body as T;
}

// Workflows API
export async function listWorkflows(auth: ApiAuth): Promise<WorkflowSummary[]> {
  return apiRequest<WorkflowSummary[]>("/api/workflows", auth);
}

export async function getWorkflow(id: string, auth: ApiAuth): Promise<WorkflowDetail> {
  return apiRequest<WorkflowDetail>(`/api/workflows/${id}`, auth);
}

export async function createWorkflow(
  name: string,
  definition: WorkflowDefinition,
  auth: ApiAuth,
  description?: string
): Promise<{ workflow_id: string; version_id: string }> {
  return apiRequest("/api/workflows", auth, {
    method: "POST",
    body: JSON.stringify({ name, description, definition }),
  });
}

export async function updateWorkflow(
  id: string,
  name: string,
  auth: ApiAuth,
  description?: string
): Promise<void> {
  await apiRequest(`/api/workflows/${id}`, auth, {
    method: "PUT",
    body: JSON.stringify({ name, description }),
  });
}

export async function deleteWorkflow(id: string, auth: ApiAuth): Promise<void> {
  await apiRequest(`/api/workflows/${id}`, auth, {
    method: "DELETE",
  });
}

export async function saveWorkflowVersion(
  id: string,
  definition: WorkflowDefinition,
  auth: ApiAuth
): Promise<{ version_id: string }> {
  return apiRequest(`/api/workflows/${id}/versions`, auth, {
    method: "POST",
    body: JSON.stringify(definition),
  });
}

export async function activateWorkflow(
  id: string,
  versionId: string,
  auth: ApiAuth
): Promise<void> {
  await apiRequest(`/api/workflows/${id}/activate/${versionId}`, auth, {
    method: "POST",
  });
}

export async function deactivateWorkflow(id: string, auth: ApiAuth): Promise<void> {
  await apiRequest(`/api/workflows/${id}/deactivate`, auth, {
    method: "POST",
  });
}

export async function runWorkflow(
  id: string,
  triggerNodeId: string,
  input: unknown,
  auth: ApiAuth
): Promise<{ execution_id: string; output: unknown }> {
  return apiRequest(`/api/workflows/${id}/run/${triggerNodeId}`, auth, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function testNode(
  node: unknown,
  input: unknown,
  auth: ApiAuth
): Promise<{ output: unknown; route?: string; metadata?: unknown }> {
  return apiRequest("/api/nodes/test", auth, {
    method: "POST",
    body: JSON.stringify({ node, input }),
  });
}

// Executions API
export async function listExecutions(
  auth: ApiAuth,
  workflowId?: string,
  limit?: number
): Promise<ExecutionSummary[]> {
  const query = new URLSearchParams();
  if (workflowId) query.set("workflow_id", workflowId);
  if (limit) query.set("limit", String(limit));
  const qs = query.toString() ? `?${query.toString()}` : "";
  return apiRequest<ExecutionSummary[]>(`/api/executions${qs}`, auth);
}

export async function getExecution(id: string, auth: ApiAuth): Promise<ExecutionDetail> {
  return apiRequest<ExecutionDetail>(`/api/executions/${id}`, auth);
}

export async function retryExecution(
  id: string,
  auth: ApiAuth
): Promise<{ execution_id: string; retried_from: string; output: unknown }> {
  return apiRequest(`/api/executions/${id}/retry`, auth, {
    method: "POST",
  });
}

export async function cancelExecution(id: string, auth: ApiAuth): Promise<void> {
  await apiRequest(`/api/executions/${id}/cancel`, auth, {
    method: "POST",
  });
}

// Credentials API
export async function listCredentials(auth: ApiAuth): Promise<CredentialSummary[]> {
  return apiRequest<CredentialSummary[]>("/api/credentials", auth);
}

export async function createCredential(
  name: string,
  kind: string,
  value: unknown,
  auth: ApiAuth
): Promise<{ credential_id: string }> {
  return apiRequest("/api/credentials", auth, {
    method: "POST",
    body: JSON.stringify({ name, kind, value }),
  });
}

export async function updateCredential(
  id: string,
  name: string,
  value: unknown,
  auth: ApiAuth
): Promise<void> {
  await apiRequest(`/api/credentials/${id}`, auth, {
    method: "PUT",
    body: JSON.stringify({ name, value }),
  });
}

export async function deleteCredential(id: string, auth: ApiAuth): Promise<void> {
  await apiRequest(`/api/credentials/${id}`, auth, {
    method: "DELETE",
  });
}
