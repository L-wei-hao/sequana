import { useEffect, useState, type FC } from "react";
import type { ApiAuth } from "../api";
import { listExecutions, listWorkflows } from "../api";
import type { ExecutionSummary, WorkflowSummary } from "../types";

interface Props {
  auth: ApiAuth;
  onSelectExecution: (id: string) => void;
}

export const ExecutionsPage: FC<Props> = ({ auth, onSelectExecution }) => {
  const [executions, setExecutions] = useState<ExecutionSummary[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = async () => {
    setLoading(true);
    setError(null);
    try {
      const [execs, wfs] = await Promise.all([
        listExecutions(auth, selectedWorkflowId || undefined, 100),
        listWorkflows(auth),
      ]);
      setExecutions(execs);
      setWorkflows(wfs);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [selectedWorkflowId, auth.token, auth.tenantId]);

  return (
    <div className="page-shell">
      <div className="page-header">
        <div>
          <h2>Executions</h2>
          <p className="page-subtitle">Inspect historical workflow runs, trace step execution, and debug failures.</p>
        </div>
        <div className="header-actions">
          <select
            className="filter-select"
            value={selectedWorkflowId}
            onChange={(e) => setSelectedWorkflowId(e.target.value)}
          >
            <option value="">All Workflows</option>
            {workflows.map((wf) => (
              <option key={wf.id} value={wf.id}>
                {wf.name}
              </option>
            ))}
          </select>
          <button className="btn-secondary" onClick={fetchData}>
            🔄 Refresh
          </button>
        </div>
      </div>

      {error && <div className="callout callout-error">{error}</div>}

      {loading ? (
        <div className="loading-indicator">Loading execution history...</div>
      ) : executions.length === 0 ? (
        <div className="empty-state-box">
          <div className="empty-icon">⚡</div>
          <h3>No executions recorded yet</h3>
          <p className="muted">Trigger a workflow manually or via webhook to see runs here.</p>
        </div>
      ) : (
        <div className="table-wrapper">
          <table className="data-table">
            <thead>
              <tr>
                <th>Status</th>
                <th>Execution ID</th>
                <th>Trigger</th>
                <th>Duration</th>
                <th>Started</th>
                <th>Error</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {executions.map((ex) => (
                <tr
                  key={ex.id}
                  className="clickable-row"
                  onClick={() => onSelectExecution(ex.id)}
                >
                  <td>
                    <span className={`status-badge status-${ex.status}`}>
                      {ex.status}
                    </span>
                  </td>
                  <td>
                    <code className="id-code">{ex.id.slice(0, 8)}</code>
                  </td>
                  <td>
                    <span className="trigger-tag">{ex.trigger_type}</span>
                  </td>
                  <td>
                    {ex.duration_ms != null ? `${ex.duration_ms} ms` : "—"}
                  </td>
                  <td className="muted">
                    {new Date(ex.created_at).toLocaleString()}
                  </td>
                  <td>
                    {ex.error ? (
                      <span className="error-preview" title={ex.error}>
                        {ex.error.length > 50 ? `${ex.error.slice(0, 50)}...` : ex.error}
                      </span>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                  <td className="action-col">
                    <button
                      className="btn-secondary btn-sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onSelectExecution(ex.id);
                      }}
                    >
                      Inspect →
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
