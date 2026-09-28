import { useCallback, useEffect, useState, type FC } from "react";
import {
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
} from "@xyflow/react";
import type { ApiAuth } from "../api";
import {
  activateWorkflow,
  deactivateWorkflow,
  getWorkflow,
  listCredentials,
  runWorkflow,
  saveWorkflowVersion,
  testNode,
} from "../api";
import type {
  CredentialSummary,
  NodeType,
  WorkflowDefinition,
  WorkflowDetail,
} from "../types";
import { WorkflowCanvas, type EditorEdge, type EditorNode } from "../workflow/WorkflowCanvas";
import { NodePalette } from "../workflow/NodePalette";
import { NodeSettings } from "../workflow/NodeSettings";
import { NODE_CATALOG_MAP } from "../workflow/nodeCatalog";

interface Props {
  workflowId: string;
  auth: ApiAuth;
  onBack: () => void;
  onNavigateToExecution: (executionId: string) => void;
}

export const WorkflowEditorPage: FC<Props> = ({
  workflowId,
  auth,
  onBack,
  onNavigateToExecution,
}) => {
  const [workflow, setWorkflow] = useState<WorkflowDetail | null>(null);
  const [credentials, setCredentials] = useState<CredentialSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activating, setActivating] = useState(false);
  const [running, setRunning] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [nodes, setNodes, onNodesChange] = useNodesState<EditorNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<EditorEdge>([]);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);

  // Run modal state
  const [showRunModal, setShowRunModal] = useState(false);
  const [runInput, setRunInput] = useState("{}");

  const loadWorkflowData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [detail, creds] = await Promise.all([
        getWorkflow(workflowId, auth),
        listCredentials(auth),
      ]);
      setWorkflow(detail);
      setCredentials(creds);

      const flowNodes: EditorNode[] = detail.definition.nodes.map((n, idx) => ({
        id: n.id,
        type: "workflowNode",
        position: n.position || { x: 100 + (idx % 3) * 220, y: 100 + Math.floor(idx / 3) * 150 },
        data: {
          label: NODE_CATALOG_MAP[n.node_type]?.label || n.id,
          nodeType: n.node_type,
          config: (n.config as Record<string, unknown>) || {},
        },
      }));

      const flowEdges: EditorEdge[] = detail.definition.edges.map((e, idx) => ({
        id: `e-${e.source}-${e.target}-${idx}`,
        source: e.source,
        target: e.target,
        label: e.route || undefined,
        data: { route: e.route ?? null },
      }));

      setNodes(flowNodes);
      setEdges(flowEdges);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  }, [workflowId, auth.token, auth.tenantId, setNodes, setEdges]);

  useEffect(() => {
    loadWorkflowData();
  }, [loadWorkflowData]);

  const handleConnect = useCallback(
    (connection: Connection) => {
      setEdges((current) =>
        addEdge(
          {
            ...connection,
            data: { route: null },
          },
          current
        )
      );
    },
    [setEdges]
  );

  const handleAddNode = useCallback(
    (type: NodeType) => {
      const catalog = NODE_CATALOG_MAP[type];
      const count = nodes.length;
      const newNode: EditorNode = {
        id: `${type}_${Date.now()}`,
        type: "workflowNode",
        position: { x: 150 + (count % 3) * 200, y: 150 + Math.floor(count / 3) * 120 },
        data: {
          label: catalog?.label || type,
          nodeType: type,
          config: { ...(catalog?.defaultConfig || {}) },
        },
      };
      setNodes((nds) => [...nds, newNode]);
      setSelectedNodeId(newNode.id);
    },
    [nodes.length, setNodes]
  );

  const handleDeleteNode = useCallback(
    (nodeId: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== nodeId));
      setEdges((eds) => eds.filter((e) => e.source !== nodeId && e.target !== nodeId));
      if (selectedNodeId === nodeId) {
        setSelectedNodeId(null);
      }
    },
    [selectedNodeId, setNodes, setEdges]
  );

  const handleUpdateNodeConfig = useCallback(
    (newConfig: Record<string, any>) => {
      if (!selectedNodeId) return;
      setNodes((nds) =>
        nds.map((n) =>
          n.id === selectedNodeId
            ? { ...n, data: { ...n.data, config: newConfig } }
            : n
        )
      );
    },
    [selectedNodeId, setNodes]
  );

  const handleSave = async () => {
    setSaving(true);
    setNotice(null);
    setError(null);
    try {
      const definition: WorkflowDefinition = {
        nodes: nodes.map((n) => ({
          id: n.id,
          node_type: n.data.nodeType,
          config: n.data.config,
          position: n.position,
        })),
        edges: edges.map((e) => ({
          source: e.source,
          target: e.target,
          route: e.data?.route || undefined,
        })),
      };

      await saveWorkflowVersion(workflowId, definition, auth);
      await loadWorkflowData();
      setNotice("Workflow version saved successfully!");
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleToggleActive = async () => {
    if (!workflow) return;
    setActivating(true);
    setNotice(null);
    setError(null);
    try {
      if (workflow.active) {
        await deactivateWorkflow(workflowId, auth);
        setNotice("Workflow deactivated.");
      } else {
        await activateWorkflow(workflowId, workflow.latest_version_id, auth);
        setNotice("Workflow activated! Webhooks and schedules are now live.");
      }
      await loadWorkflowData();
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setActivating(false);
    }
  };

  const handleRunWorkflow = async () => {
    const trigger = nodes.find((n) => n.data.nodeType === "manual_trigger");
    if (!trigger) {
      setError("Workflow must have a Manual Trigger node to run manually.");
      return;
    }

    setRunning(true);
    setError(null);
    try {
      let parsed = {};
      try {
        parsed = JSON.parse(runInput);
      } catch {
        throw new Error("Run input must be valid JSON");
      }
      const res = await runWorkflow(workflowId, trigger.id, parsed, auth);
      setShowRunModal(false);
      onNavigateToExecution(res.execution_id);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setRunning(false);
    }
  };

  const selectedNode = nodes.find((n) => n.id === selectedNodeId) || null;
  const selectedEdge = edges.find((e) => e.id === selectedEdgeId) || null;

  return (
    <div className="editor-page-shell">
      <header className="editor-topbar">
        <div className="topbar-left">
          <button className="btn-secondary btn-sm" onClick={onBack}>
            ← Back to Workflows
          </button>
          <div className="editor-title-block">
            <h3>{workflow?.name ?? "Workflow Editor"}</h3>
            {workflow && (
              <span className={`status-pill ${workflow.active ? "active" : "inactive"}`}>
                v{workflow.latest_version} • {workflow.active ? "ACTIVE" : "INACTIVE"}
              </span>
            )}
          </div>
        </div>

        <div className="topbar-right">
          <button className="btn-secondary" onClick={() => setShowRunModal(true)} disabled={running}>
            ▶ Run Workflow
          </button>
          <button
            className={workflow?.active ? "btn-warning" : "btn-success"}
            onClick={handleToggleActive}
            disabled={activating || !workflow}
          >
            {activating ? "Processing..." : workflow?.active ? "Deactivate" : "Activate"}
          </button>
          <button className="btn-primary" onClick={handleSave} disabled={saving || !workflow}>
            {saving ? "Saving..." : "Save Version"}
          </button>
        </div>
      </header>

      {notice && (
        <div className="callout callout-success banner-alert">
          {notice}
          <button onClick={() => setNotice(null)}>✕</button>
        </div>
      )}
      {error && (
        <div className="callout callout-error banner-alert">
          {error}
          <button onClick={() => setError(null)}>✕</button>
        </div>
      )}

      {loading ? (
        <div className="loading-indicator">Loading canvas...</div>
      ) : (
        <div className="editor-workspace">
          <NodePalette onAddNode={handleAddNode} />

          <WorkflowCanvas
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={handleConnect}
            selectedNodeId={selectedNodeId}
            selectedEdgeId={selectedEdgeId}
            onSelectNode={(n) => setSelectedNodeId(n ? n.id : null)}
            onSelectEdge={(e) => setSelectedEdgeId(e ? e.id : null)}
          />

          <aside className="editor-sidebar-panel">
            {selectedEdge ? (
              <div className="edge-settings-panel">
                <h4>Connection Route</h4>
                <p className="field-hint">
                  Specify the condition route branch this edge represents.
                </p>
                <label>
                  Route Name
                  <input
                    type="text"
                    placeholder="e.g. true, false, or case_name"
                    value={selectedEdge.data?.route ?? ""}
                    onChange={(e) => {
                      const newRoute = e.target.value.trim() || null;
                      setEdges((eds) =>
                        eds.map((edge) =>
                          edge.id === selectedEdge.id
                            ? {
                                ...edge,
                                label: newRoute || undefined,
                                data: { ...edge.data, route: newRoute },
                              }
                            : edge
                        )
                      );
                    }}
                  />
                </label>
                <button
                  className="btn-danger-outline btn-sm"
                  onClick={() => {
                    setEdges((eds) => eds.filter((edge) => edge.id !== selectedEdge.id));
                    setSelectedEdgeId(null);
                  }}
                >
                  Delete Connection
                </button>
              </div>
            ) : (
              <NodeSettings
                node={selectedNode}
                credentials={credentials}
                onUpdateConfig={handleUpdateNodeConfig}
                onDeleteNode={handleDeleteNode}
                onTestNode={async (n, input) => {
                  return testNode(
                    {
                      id: n.id,
                      node_type: n.data.nodeType,
                      config: n.data.config,
                    },
                    input,
                    auth
                  );
                }}
              />
            )}
          </aside>
        </div>
      )}

      {showRunModal && (
        <div className="modal-backdrop">
          <div className="modal-card">
            <h3>Run Workflow Manually</h3>
            <p className="field-hint">
              Triggers the workflow at the <strong>Manual Trigger</strong> node with JSON payload.
            </p>
            <label>
              Input Payload (JSON)
              <textarea
                rows={8}
                className="mono-editor"
                value={runInput}
                onChange={(e) => setRunInput(e.target.value)}
              />
            </label>
            <div className="modal-footer">
              <button
                className="btn-secondary"
                onClick={() => setShowRunModal(false)}
                disabled={running}
              >
                Cancel
              </button>
              <button className="btn-primary" onClick={handleRunWorkflow} disabled={running}>
                {running ? "Starting..." : "Start Execution →"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
