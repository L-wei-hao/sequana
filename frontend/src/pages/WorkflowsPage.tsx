import { useEffect, useState, type FC } from "react";
import type { ApiAuth } from "../api";
import { createWorkflow, deleteWorkflow, listWorkflows } from "../api";
import type { WorkflowDefinition, WorkflowSummary } from "../types";

interface Props {
  auth: ApiAuth;
  onSelectWorkflow: (id: string) => void;
}

export const WorkflowsPage: FC<Props> = ({ auth, onSelectWorkflow }) => {
  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showNewModal, setShowNewModal] = useState(false);
  const [newWorkflowName, setNewWorkflowName] = useState("");
  const [newWorkflowDesc, setNewWorkflowDesc] = useState("");
  const [creating, setCreating] = useState(false);

  const fetchWorkflows = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await listWorkflows(auth);
      setWorkflows(data);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchWorkflows();
  }, [auth.token, auth.tenantId]);

  const handleCreate = async () => {
    if (!newWorkflowName.trim()) return;
    setCreating(true);
    try {
      const initialDefinition: WorkflowDefinition = {
        nodes: [
          {
            id: `manual_trigger_${Date.now()}`,
            node_type: "manual_trigger",
            config: {},
            position: { x: 100, y: 120 },
          },
        ],
        edges: [],
      };
      const res = await createWorkflow(
        newWorkflowName.trim(),
        initialDefinition,
        auth,
        newWorkflowDesc.trim() || undefined
      );
      setShowNewModal(false);
      setNewWorkflowName("");
      setNewWorkflowDesc("");
      onSelectWorkflow(res.workflow_id);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`Are you sure you want to delete workflow "${name}"?`)) {
      return;
    }
    try {
      await deleteWorkflow(id, auth);
      await fetchWorkflows();
    } catch (err: any) {
      setError(err.message || String(err));
    }
  };

  return (
    <div className="page-shell">
      <div className="page-header">
        <div>
          <h2>Workflows</h2>
          <p className="page-subtitle">Manage automated workflows, versions, and active deployments.</p>
        </div>
        <button className="btn-primary" onClick={() => setShowNewModal(true)}>
          + Create Workflow
        </button>
      </div>

      {error && <div className="callout callout-error">{error}</div>}

      {loading ? (
        <div className="loading-indicator">Loading workflows...</div>
      ) : workflows.length === 0 ? (
        <div className="empty-state-box">
          <div className="empty-icon">📂</div>
          <h3>No workflows created yet</h3>
          <p className="muted">Create your first automated workflow to get started.</p>
          <button className="btn-primary" onClick={() => setShowNewModal(true)}>
            + Create First Workflow
          </button>
        </div>
      ) : (
        <div className="workflow-grid">
          {workflows.map((wf) => (
            <div key={wf.id} className="workflow-card">
              <div className="card-top">
                <div className="card-title-group">
                  <h3 onClick={() => onSelectWorkflow(wf.id)} className="clickable-title">
                    {wf.name}
                  </h3>
                  {wf.description && <p className="card-desc">{wf.description}</p>}
                </div>
                <span className={`status-pill ${wf.active ? "active" : "inactive"}`}>
                  {wf.active ? "● ACTIVE" : "INACTIVE"}
                </span>
              </div>

              <div className="card-meta">
                <span>Version: v{wf.latest_version}</span>
                <span>Updated: {new Date(wf.updated_at).toLocaleDateString()}</span>
              </div>

              <div className="card-actions">
                <button className="btn-secondary btn-sm" onClick={() => onSelectWorkflow(wf.id)}>
                  Edit in Canvas →
                </button>
                <button
                  className="btn-danger-text btn-sm"
                  onClick={() => handleDelete(wf.id, wf.name)}
                >
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {showNewModal && (
        <div className="modal-backdrop">
          <div className="modal-card">
            <h3>Create New Workflow</h3>
            <label>
              Workflow Name
              <input
                type="text"
                placeholder="e.g. Candidate Profile Webhook"
                value={newWorkflowName}
                onChange={(e) => setNewWorkflowName(e.target.value)}
                autoFocus
              />
            </label>
            <label>
              Description (Optional)
              <input
                type="text"
                placeholder="Processes candidate resumes with OpenAI"
                value={newWorkflowDesc}
                onChange={(e) => setNewWorkflowDesc(e.target.value)}
              />
            </label>
            <div className="modal-footer">
              <button
                className="btn-secondary"
                onClick={() => setShowNewModal(false)}
                disabled={creating}
              >
                Cancel
              </button>
              <button className="btn-primary" onClick={handleCreate} disabled={creating}>
                {creating ? "Creating..." : "Create & Edit"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
