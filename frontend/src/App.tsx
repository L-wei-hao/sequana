import { useCallback, useEffect, useMemo, useState } from "react";
import {
  addEdge,
  Background,
  Controls,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge as FlowEdge,
  type Node as FlowNode
} from "@xyflow/react";

type NodeType =
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

type BackendNode = {
  id: string;
  node_type: NodeType;
  config: unknown;
  position?: { x: number; y: number };
};

type BackendEdge = {
  source: string;
  target: string;
  route?: string | null;
};

type WorkflowDefinition = {
  nodes: BackendNode[];
  edges: BackendEdge[];
};

type WorkflowSummary = {
  id: string;
  name: string;
  active: boolean;
  active_version_id: string | null;
  latest_version_id: string;
  latest_version: number;
};

type WorkflowDetail = WorkflowSummary & {
  definition: WorkflowDefinition;
};

type CredentialSummary = {
  id: string;
  name: string;
  kind: string;
};

type ExecutionSummary = {
  id: string;
  workflow_id: string;
  workflow_version_id: string;
  trigger_type: string;
  trigger_node_id: string;
  status: string;
  error: string | null;
  created_at: string;
  duration_ms: number | null;
};

type ExecutionStep = {
  id: string;
  node_id: string;
  node_type: string;
  status: string;
  input: unknown;
  output: unknown;
  error: string | null;
  duration_ms: number | null;
  started_at: string;
  finished_at: string | null;
};

type ExecutionDetail = {
  id: string;
  workflow_id: string;
  workflow_version_id: string;
  trigger_type: string;
  trigger_node_id: string;
  status: string;
  input: unknown;
  output: unknown;
  error: string | null;
  started_at: string | null;
  finished_at: string | null;
  duration_ms: number | null;
  steps: ExecutionStep[];
};

type EditorNodeData = Record<string, unknown> & {
  label: string;
  nodeType: NodeType;
  config: unknown;
};

type EditorNode = FlowNode<EditorNodeData>;
type EditorEdge = FlowEdge<{ route?: string | null }>;

const NODE_TYPES: Array<{ type: NodeType; label: string }> = [
  { type: "manual_trigger", label: "Manual Trigger" },
  { type: "webhook", label: "Webhook" },
  { type: "schedule", label: "Schedule" },
  { type: "respond_to_webhook", label: "Respond to Webhook" },
  { type: "http_request", label: "HTTP Request" },
  { type: "open_ai", label: "OpenAI" },
  { type: "postgres", label: "PostgreSQL" },
  { type: "set", label: "Set / Transform" },
  { type: "if", label: "If" },
  { type: "switch", label: "Switch" }
];

const NODE_LABEL = Object.fromEntries(
  NODE_TYPES.map(({ type, label }) => [type, label])
) as Record<NodeType, string>;

function defaultNodeConfig(nodeType: NodeType): unknown {
  switch (nodeType) {
    case "schedule":
      return { cron: "0 9 * * *", timezone: "Asia/Singapore", input: {} };
    case "respond_to_webhook":
      return { status: 200 };
    case "http_request":
      return { method: "GET", url: "https://example.com", headers: {} };
    case "open_ai":
      return {
        credential_id: "",
        model: "gpt-5.6-luna",
        input: { "$from": "/prompt" },
        output: { type: "text" }
      };
    case "postgres":
      return { mode: "query", query: "SELECT 1 AS ok", params: [] };
    case "set":
      return { values: {}, merge_input: true };
    case "if":
      return { left: { "$from": "/value" }, operator: "eq", right: true };
    case "switch":
      return { value: { "$from": "/value" }, cases: [], default_route: "default" };
    default:
      return {};
  }
}

async function api<T>(
  path: string,
  token: string,
  tenantId: string,
  init: RequestInit = {}
): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${token}`);
  headers.set("x-tenant-id", tenantId);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(path, { ...init, headers });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;

  if (!response.ok) {
    throw new Error(body?.error ?? `HTTP ${response.status}`);
  }

  return body as T;
}

function parseJson(value: string, label: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    throw new Error(`${label} must be valid JSON`);
  }
}

function defaultPosition(index: number) {
  return {
    x: 80 + (index % 4) * 190,
    y: 80 + Math.floor(index / 4) * 120
  };
}

function toEditor(definition: WorkflowDefinition) {
  const nodes: EditorNode[] = definition.nodes.map((node, index) => ({
    id: node.id,
    position: node.position ?? defaultPosition(index),
    data: {
      label: NODE_LABEL[node.node_type],
      nodeType: node.node_type,
      config: node.config ?? {}
    },
    className: `sequana-node node-${node.node_type}`
  }));

  const edges: EditorEdge[] = definition.edges.map((edge, index) => ({
    id: `${edge.source}-${edge.target}-${index}`,
    source: edge.source,
    target: edge.target,
    label: edge.route || undefined,
    data: { route: edge.route ?? null }
  }));

  return { nodes, edges };
}

function toBackend(nodes: EditorNode[], edges: EditorEdge[]): WorkflowDefinition {
  return {
    nodes: nodes.map((node) => ({
      id: node.id,
      node_type: node.data.nodeType,
      config: node.data.config ?? {},
      position: node.position
    })),
    edges: edges.map((edge) => ({
      source: edge.source,
      target: edge.target,
      route: edge.data?.route?.trim() || null
    }))
  };
}

export default function App() {
  const [token, setToken] = useState(() => sessionStorage.getItem("sequana.token") ?? "");
  const [tenantId, setTenantId] = useState(() => sessionStorage.getItem("sequana.tenant") ?? "");
  const [connected, setConnected] = useState(() => Boolean(token && tenantId));
  const [view, setView] = useState<"editor" | "executions" | "credentials">("editor");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [workflowId, setWorkflowId] = useState("");
  const [workflow, setWorkflow] = useState<WorkflowDetail | null>(null);
  const [credentials, setCredentials] = useState<CredentialSummary[]>([]);
  const [credentialName, setCredentialName] = useState("");
  const [credentialValue, setCredentialValue] = useState("");
  const [executions, setExecutions] = useState<ExecutionSummary[]>([]);
  const [execution, setExecution] = useState<ExecutionDetail | null>(null);

  const [nodes, setNodes, onNodesChange] = useNodesState<EditorNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<EditorEdge>([]);
  const [selectedNodeId, setSelectedNodeId] = useState("");
  const [selectedEdgeId, setSelectedEdgeId] = useState("");
  const [configText, setConfigText] = useState("{}");
  const [routeText, setRouteText] = useState("");
  const [testInput, setTestInput] = useState("{}");
  const [testResult, setTestResult] = useState("");
  const [runInput, setRunInput] = useState("{}");

  const selectedNode = useMemo(
    () => nodes.find((node) => node.id === selectedNodeId) ?? null,
    [nodes, selectedNodeId]
  );
  const selectedEdge = useMemo(
    () => edges.find((edge) => edge.id === selectedEdgeId) ?? null,
    [edges, selectedEdgeId]
  );

  const refreshWorkflows = useCallback(async () => {
    if (!connected) return;
    const list = await api<WorkflowSummary[]>("/api/workflows", token, tenantId);
    setWorkflows(list);
    setWorkflowId((current) => current || list[0]?.id || "");
  }, [connected, tenantId, token]);

  const refreshCredentials = useCallback(async () => {
    if (!connected) return;
    const list = await api<CredentialSummary[]>("/api/credentials", token, tenantId);
    setCredentials(list);
  }, [connected, tenantId, token]);

  const refreshExecutions = useCallback(async () => {
    if (!connected) return;
    const query = workflowId ? `?workflow_id=${encodeURIComponent(workflowId)}` : "";
    const list = await api<ExecutionSummary[]>(
      `/api/executions${query}`,
      token,
      tenantId
    );
    setExecutions(list);
  }, [connected, tenantId, token, workflowId]);

  useEffect(() => {
    if (!connected) return;

    setError("");
    Promise.all([refreshWorkflows(), refreshCredentials()]).catch((cause: unknown) => {
      setError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [connected, refreshCredentials, refreshWorkflows]);

  useEffect(() => {
    if (!connected || !workflowId) {
      setWorkflow(null);
      setNodes([]);
      setEdges([]);
      return;
    }

    api<WorkflowDetail>(`/api/workflows/${workflowId}`, token, tenantId)
      .then((detail) => {
        const graph = toEditor(detail.definition);
        setWorkflow(detail);
        setNodes(graph.nodes);
        setEdges(graph.edges);
        setSelectedNodeId("");
        setSelectedEdgeId("");
      })
      .catch((cause: unknown) => {
        setError(cause instanceof Error ? cause.message : String(cause));
      });
  }, [connected, setEdges, setNodes, tenantId, token, workflowId]);

  useEffect(() => {
    if (view !== "executions") return;
    refreshExecutions().catch((cause: unknown) => {
      setError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [refreshExecutions, view]);

  useEffect(() => {
    if (!selectedNode) return;
    setConfigText(JSON.stringify(selectedNode.data.config ?? {}, null, 2));
    setTestResult("");
  }, [selectedNode]);

  useEffect(() => {
    if (!selectedEdge) return;
    setRouteText(selectedEdge.data?.route ?? "");
  }, [selectedEdge]);

  function connect() {
    if (!token.trim() || !tenantId.trim()) {
      setError("Admin token and tenant ID are required");
      return;
    }
    sessionStorage.setItem("sequana.token", token.trim());
    sessionStorage.setItem("sequana.tenant", tenantId.trim());
    setConnected(true);
    setError("");
  }

  function disconnect() {
    sessionStorage.removeItem("sequana.token");
    sessionStorage.removeItem("sequana.tenant");
    setConnected(false);
    setWorkflows([]);
    setWorkflowId("");
    setWorkflow(null);
    setExecutions([]);
    setExecution(null);
  }

  async function createCredential() {
    if (!credentialName.trim() || !credentialValue) {
      setError("Credential name and API key are required");
      return;
    }

    try {
      await api("/api/credentials", token, tenantId, {
        method: "POST",
        body: JSON.stringify({
          name: credentialName.trim(),
          kind: "openai",
          value: credentialValue
        })
      });
      setCredentialValue("");
      setCredentialName("");
      await refreshCredentials();
      setNotice("OpenAI credential saved");
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function createWorkflow() {
    const name = window.prompt("Workflow name");
    if (!name?.trim()) return;

    const definition: WorkflowDefinition = {
      nodes: [
        {
          id: `manual-${Date.now()}`,
          node_type: "manual_trigger",
          config: defaultNodeConfig("manual_trigger"),
          position: { x: 100, y: 120 }
        }
      ],
      edges: []
    };

    try {
      const created = await api<{ workflow_id: string }>(
        "/api/workflows",
        token,
        tenantId,
        {
          method: "POST",
          body: JSON.stringify({ name: name.trim(), definition })
        }
      );
      await refreshWorkflows();
      setWorkflowId(created.workflow_id);
      setNotice("Workflow created");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  function addNode(nodeType: NodeType) {
    const index = nodes.length;
    setNodes((current) => [
      ...current,
      {
        id: `${nodeType}-${Date.now()}`,
        position: defaultPosition(index),
        data: {
          label: NODE_LABEL[nodeType],
          nodeType,
          config: defaultNodeConfig(nodeType)
        },
        className: `sequana-node node-${nodeType}`
      }
    ]);
  }

  function applyConfig() {
    if (!selectedNode) return;

    try {
      const config = parseJson(configText, "Node config");
      setNodes((current) =>
        current.map((node) =>
          node.id === selectedNode.id
            ? { ...node, data: { ...node.data, config } }
            : node
        )
      );
      setNotice("Node config applied");
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  function setOpenAiCredential(credentialId: string) {
    if (!selectedNode) return;

    const current =
      typeof selectedNode.data.config === "object" &&
      selectedNode.data.config !== null &&
      !Array.isArray(selectedNode.data.config)
        ? selectedNode.data.config
        : {};

    const config = { ...current, credential_id: credentialId };
    setNodes((items) =>
      items.map((node) =>
        node.id === selectedNode.id
          ? { ...node, data: { ...node.data, config } }
          : node
      )
    );
    setConfigText(JSON.stringify(config, null, 2));
  }

  function applyRoute() {
    if (!selectedEdge) return;

    setEdges((current) =>
      current.map((edge) =>
        edge.id === selectedEdge.id
          ? {
              ...edge,
              label: routeText.trim() || undefined,
              data: { ...edge.data, route: routeText.trim() || null }
            }
          : edge
      )
    );
    setNotice("Edge route applied");
  }

  function deleteSelection() {
    if (selectedNodeId) {
      setNodes((current) => current.filter((node) => node.id !== selectedNodeId));
      setEdges((current) =>
        current.filter(
          (edge) => edge.source !== selectedNodeId && edge.target !== selectedNodeId
        )
      );
      setSelectedNodeId("");
    }

    if (selectedEdgeId) {
      setEdges((current) => current.filter((edge) => edge.id !== selectedEdgeId));
      setSelectedEdgeId("");
    }
  }

  async function saveWorkflow() {
    if (!workflow) return;

    try {
      await api<{ version_id: string }>(
        `/api/workflows/${workflow.id}/versions`,
        token,
        tenantId,
        {
          method: "POST",
          body: JSON.stringify(toBackend(nodes, edges))
        }
      );
      const detail = await api<WorkflowDetail>(
        `/api/workflows/${workflow.id}`,
        token,
        tenantId
      );
      setWorkflow(detail);
      await refreshWorkflows();
      setNotice("Workflow saved");
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function activateWorkflow() {
    if (!workflow) return;

    try {
      await api(
        `/api/workflows/${workflow.id}/activate/${workflow.latest_version_id}`,
        token,
        tenantId,
        { method: "POST" }
      );
      setWorkflow({ ...workflow, active: true, active_version_id: workflow.latest_version_id });
      await refreshWorkflows();
      setNotice("Workflow activated");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function testSelectedNode() {
    if (!selectedNode) return;

    try {
      const input = parseJson(testInput, "Test input");
      const result = await api<unknown>("/api/nodes/test", token, tenantId, {
        method: "POST",
        body: JSON.stringify({
          node: {
            id: selectedNode.id,
            node_type: selectedNode.data.nodeType,
            config: selectedNode.data.config ?? {},
            position: selectedNode.position
          },
          input
        })
      });
      setTestResult(JSON.stringify(result, null, 2));
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function runWorkflow() {
    if (!workflow) return;

    const trigger = nodes.find((node) => node.data.nodeType === "manual_trigger");
    if (!trigger) {
      setError("Workflow needs a Manual Trigger to run from the editor");
      return;
    }

    try {
      const input = parseJson(runInput, "Run input");
      const result = await api<unknown>(
        `/api/workflows/${workflow.id}/run/${trigger.id}`,
        token,
        tenantId,
        {
          method: "POST",
          body: JSON.stringify(input)
        }
      );
      setNotice("Workflow completed");
      setTestResult(JSON.stringify(result, null, 2));
      await refreshExecutions();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function openExecution(executionId: string) {
    try {
      const detail = await api<ExecutionDetail>(
        `/api/executions/${executionId}`,
        token,
        tenantId
      );
      setExecution(detail);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function retryExecution() {
    if (!execution) return;

    try {
      await api(`/api/executions/${execution.id}/retry`, token, tenantId, {
        method: "POST"
      });
      await refreshExecutions();
      setNotice("Execution retried");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function cancelExecution() {
    if (!execution) return;

    try {
      await api(`/api/executions/${execution.id}/cancel`, token, tenantId, {
        method: "POST"
      });
      await refreshExecutions();
      await openExecution(execution.id);
      setNotice("Execution cancelled");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  if (!connected) {
    return (
      <main className="connect-page">
        <section className="connect-card">
          <h1>Sequana</h1>
          <p>Lightweight workflow automation, built for AI.</p>
          <label>
            Tenant ID
            <input value={tenantId} onChange={(event) => setTenantId(event.target.value)} />
          </label>
          <label>
            Admin token
            <input
              type="password"
              value={token}
              onChange={(event) => setToken(event.target.value)}
            />
          </label>
          {error && <div className="error">{error}</div>}
          <button onClick={connect}>Connect</button>
        </section>
      </main>
    );
  }

  return (
    <main className="app">
      <header className="topbar">
        <div>
          <strong>Sequana</strong>
          <span className="muted">workflow automation</span>
        </div>
        <nav>
          <button className={view === "editor" ? "active" : ""} onClick={() => setView("editor")}>
            Editor
          </button>
          <button
            className={view === "executions" ? "active" : ""}
            onClick={() => setView("executions")}
          >
            Executions
          </button>
          <button
            className={view === "credentials" ? "active" : ""}
            onClick={() => setView("credentials")}
          >
            Credentials
          </button>
          <button onClick={disconnect}>Disconnect</button>
        </nav>
      </header>

      {(error || notice) && (
        <div className={error ? "banner error" : "banner notice"}>
          {error || notice}
          <button
            className="banner-close"
            onClick={() => {
              setError("");
              setNotice("");
            }}
          >
            ×
          </button>
        </div>
      )}

      {view === "editor" ? (
        <section className="editor-shell">
          <aside className="sidebar">
            <div className="panel-section">
              <div className="section-title">
                <span>Workflow</span>
                <button onClick={createWorkflow}>New</button>
              </div>
              <select value={workflowId} onChange={(event) => setWorkflowId(event.target.value)}>
                <option value="">Select workflow</option>
                {workflows.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}{item.active ? " • active" : ""}
                  </option>
                ))}
              </select>
            </div>

            <div className="panel-section">
              <div className="section-title">Nodes</div>
              <div className="node-list">
                {NODE_TYPES.map((item) => (
                  <button key={item.type} onClick={() => addNode(item.type)}>
                    + {item.label}
                  </button>
                ))}
              </div>
            </div>
          </aside>

          <section className="canvas-panel">
            <div className="canvas-toolbar">
              <div>
                <strong>{workflow?.name ?? "No workflow selected"}</strong>
                {workflow && <span className="muted"> v{workflow.latest_version}</span>}
              </div>
              <div className="toolbar-actions">
                <button onClick={saveWorkflow} disabled={!workflow}>Save</button>
                <button onClick={activateWorkflow} disabled={!workflow}>Activate</button>
                <button onClick={runWorkflow} disabled={!workflow}>Run workflow</button>
              </div>
            </div>

            <div className="canvas">
              <ReactFlow<EditorNode, EditorEdge>
                nodes={nodes}
                edges={edges}
                onNodesChange={onNodesChange}
                onEdgesChange={onEdgesChange}
                onConnect={(connection: Connection) =>
                  setEdges((current) =>
                    addEdge({ ...connection, data: { route: null } }, current)
                  )
                }
                onNodeClick={(_, node) => {
                  setSelectedNodeId(node.id);
                  setSelectedEdgeId("");
                }}
                onEdgeClick={(_, edge) => {
                  setSelectedEdgeId(edge.id);
                  setSelectedNodeId("");
                }}
                onPaneClick={() => {
                  setSelectedNodeId("");
                  setSelectedEdgeId("");
                }}
                fitView
              >
                <Background />
                <Controls />
              </ReactFlow>
            </div>
          </section>

          <aside className="settings">
            {selectedNode ? (
              <>
                <div className="section-title">{selectedNode.data.label}</div>
                <div className="muted mono">{selectedNode.id}</div>

                {selectedNode.data.nodeType === "open_ai" && (
                  <label>
                    OpenAI credential
                    <select
                      value={
                        typeof selectedNode.data.config === "object" &&
                        selectedNode.data.config !== null &&
                        "credential_id" in selectedNode.data.config
                          ? String(
                              (selectedNode.data.config as Record<string, unknown>).credential_id ?? ""
                            )
                          : ""
                      }
                      onChange={(event) => setOpenAiCredential(event.target.value)}
                    >
                      <option value="">Select credential</option>
                      {credentials
                        .filter((credential) => credential.kind === "openai")
                        .map((credential) => (
                          <option key={credential.id} value={credential.id}>
                            {credential.name}
                          </option>
                        ))}
                    </select>
                  </label>
                )}

                <label>
                  Config JSON
                  <textarea
                    rows={12}
                    value={configText}
                    onChange={(event) => setConfigText(event.target.value)}
                  />
                </label>
                <button onClick={applyConfig}>Apply config</button>

                <label>
                  Test input
                  <textarea
                    rows={6}
                    value={testInput}
                    onChange={(event) => setTestInput(event.target.value)}
                  />
                </label>
                <button onClick={testSelectedNode}>Test node</button>
                <button className="danger" onClick={deleteSelection}>Delete node</button>
              </>
            ) : selectedEdge ? (
              <>
                <div className="section-title">Connection</div>
                <label>
                  Route
                  <input
                    placeholder="true, false, case_1..."
                    value={routeText}
                    onChange={(event) => setRouteText(event.target.value)}
                  />
                </label>
                <button onClick={applyRoute}>Apply route</button>
                <button className="danger" onClick={deleteSelection}>Delete connection</button>
              </>
            ) : (
              <>
                <div className="section-title">Run input</div>
                <label>
                  JSON
                  <textarea
                    rows={8}
                    value={runInput}
                    onChange={(event) => setRunInput(event.target.value)}
                  />
                </label>
                <p className="muted">
                  Select a node to edit or test it. Select a connection to set an If/Switch route.
                </p>
              </>
            )}

            {testResult && (
              <div className="result">
                <div className="section-title">Result</div>
                <pre>{testResult}</pre>
              </div>
            )}
          </aside>
        </section>
      ) : view === "executions" ? (
        <section className="executions-shell">
          <aside className="execution-list">
            <div className="section-title">
              <span>Execution history</span>
              <button onClick={() => refreshExecutions()}>Refresh</button>
            </div>
            {executions.map((item) => (
              <button
                key={item.id}
                className={execution?.id === item.id ? "execution active" : "execution"}
                onClick={() => openExecution(item.id)}
              >
                <span className={`status status-${item.status}`}>{item.status}</span>
                <strong>{item.trigger_type}</strong>
                <span className="muted mono">{item.id.slice(0, 8)}</span>
                <span className="muted">
                  {item.duration_ms == null ? item.created_at : `${item.duration_ms} ms • ${item.created_at}`}
                </span>
              </button>
            ))}
            {!executions.length && <p className="muted">No executions yet.</p>}
          </aside>

          <section className="execution-detail">
            {execution ? (
              <>
                <div className="execution-header">
                  <div>
                    <h2>Execution {execution.id.slice(0, 8)}</h2>
                    <span className={`status status-${execution.status}`}>
                      {execution.status}
                    </span>
                    {execution.duration_ms != null && (
                      <span className="muted"> {execution.duration_ms} ms</span>
                    )}
                  </div>
                  <div>
                    <button onClick={retryExecution}>Retry</button>
                    <button
                      className="danger"
                      onClick={cancelExecution}
                      disabled={!["queued", "running"].includes(execution.status)}
                    >
                      Cancel
                    </button>
                  </div>
                </div>

                {execution.error && <div className="error">{execution.error}</div>}

                <div className="steps">
                  {execution.steps.map((step) => (
                    <details key={step.id} className="step">
                      <summary>
                        <span className={`status status-${step.status}`}>{step.status}</span>
                        <strong>{step.node_id}</strong>
                        <span>{step.node_type}</span>
                        <span className="muted">
                          {step.duration_ms == null ? "" : `${step.duration_ms} ms`}
                        </span>
                      </summary>
                      {step.error && <div className="error">{step.error}</div>}
                      <div className="step-grid">
                        <div>
                          <h4>Input</h4>
                          <pre>{JSON.stringify(step.input, null, 2)}</pre>
                        </div>
                        <div>
                          <h4>Output</h4>
                          <pre>{JSON.stringify(step.output, null, 2)}</pre>
                        </div>
                      </div>
                    </details>
                  ))}
                </div>
              </>
            ) : (
              <div className="empty-state">Select an execution to inspect it.</div>
            )}
          </section>
        </section>
      ) : (
        <section className="credentials-shell">
          <div className="credentials-card">
            <div className="section-title">OpenAI credentials</div>
            <p className="muted">
              API keys are encrypted by the Rust backend. Workflow definitions store only the credential ID.
            </p>

            <label>
              Name
              <input
                placeholder="OpenAI Prod"
                value={credentialName}
                onChange={(event) => setCredentialName(event.target.value)}
              />
            </label>
            <label>
              API key
              <input
                type="password"
                autoComplete="off"
                value={credentialValue}
                onChange={(event) => setCredentialValue(event.target.value)}
              />
            </label>
            <button onClick={createCredential}>Save credential</button>

            <div className="credential-list">
              {credentials.map((credential) => (
                <div key={credential.id} className="credential-row">
                  <div>
                    <strong>{credential.name}</strong>
                    <div className="muted">{credential.kind}</div>
                  </div>
                  <span className="muted mono">{credential.id}</span>
                </div>
              ))}
              {!credentials.length && <p className="muted">No credentials saved.</p>}
            </div>
          </div>
        </section>
      )}
    </main>
  );
}
