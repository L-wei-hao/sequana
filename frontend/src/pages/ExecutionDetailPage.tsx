import { useCallback, useEffect, useState, type FC } from "react";
import type { ApiAuth } from "../api";
import { cancelExecution, getExecution, retryExecution } from "../api";
import type { ExecutionDetail } from "../types";
import { ExecutionSteps } from "../executions/ExecutionSteps";

interface Props {
  executionId: string;
  auth: ApiAuth;
  onBack: () => void;
  onSelectExecution: (id: string) => void;
}

export const ExecutionDetailPage: FC<Props> = ({
  executionId,
  auth,
  onBack,
  onSelectExecution,
}) => {
  const [execution, setExecution] = useState<ExecutionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const fetchDetail = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getExecution(executionId, auth);
      setExecution(data);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  }, [executionId, auth.token, auth.tenantId]);

  useEffect(() => {
    fetchDetail();
  }, [fetchDetail]);

  const handleRetry = async () => {
    setActionLoading(true);
    setError(null);
    try {
      const res = await retryExecution(executionId, auth);
      setNotice("Execution retried successfully! Redirecting to new execution run...");
      setTimeout(() => {
        onSelectExecution(res.execution_id);
      }, 1000);
    } catch (err: any) {
      setError(err.message || String(err));
      setActionLoading(false);
    }
  };

  const handleCancel = async () => {
    setActionLoading(true);
    setError(null);
    try {
      await cancelExecution(executionId, auth);
      setNotice("Execution cancelled.");
      await fetchDetail();
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setActionLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="page-shell">
        <div className="loading-indicator">Loading execution details...</div>
      </div>
    );
  }

  if (!execution) {
    return (
      <div className="page-shell">
        <button className="btn-secondary btn-sm" onClick={onBack}>
          ← Back to Executions
        </button>
        <div className="callout callout-error">Execution not found.</div>
      </div>
    );
  }

  const isCancellable = ["queued", "running"].includes(execution.status);

  return (
    <div className="page-shell">
      <div className="page-header">
        <div className="header-breadcrumbs">
          <button className="btn-secondary btn-sm" onClick={onBack}>
            ← Back to Executions
          </button>
          <h2>Execution #{execution.id.slice(0, 8)}</h2>
          <span className={`status-badge status-${execution.status}`}>
            {execution.status}
          </span>
        </div>

        <div className="header-actions">
          {isCancellable && (
            <button
              className="btn-danger"
              onClick={handleCancel}
              disabled={actionLoading}
            >
              Cancel Execution
            </button>
          )}
          <button
            className="btn-secondary"
            onClick={handleRetry}
            disabled={actionLoading || isCancellable}
          >
            🔄 Retry Execution
          </button>
        </div>
      </div>

      {notice && <div className="callout callout-success">{notice}</div>}
      {error && <div className="callout callout-error">{error}</div>}

      <div className="meta-card-grid">
        <div className="meta-card">
          <span className="meta-label">Trigger</span>
          <span className="meta-value">{execution.trigger_type} ({execution.trigger_node_id})</span>
        </div>
        <div className="meta-card">
          <span className="meta-label">Total Duration</span>
          <span className="meta-value">
            {execution.duration_ms != null ? `${execution.duration_ms} ms` : "In Progress"}
          </span>
        </div>
        <div className="meta-card">
          <span className="meta-label">Started At</span>
          <span className="meta-value">
            {execution.started_at ? new Date(execution.started_at).toLocaleString() : "Queued"}
          </span>
        </div>
        {execution.retry_of_execution_id && (
          <div className="meta-card">
            <span className="meta-label">Retry Of</span>
            <span
              className="meta-value clickable-link"
              onClick={() => onSelectExecution(execution.retry_of_execution_id!)}
            >
              #{execution.retry_of_execution_id.slice(0, 8)}
            </span>
          </div>
        )}
      </div>

      {execution.error && (
        <div className="callout callout-error" style={{ marginBottom: "24px" }}>
          <strong>Execution Error:</strong> {execution.error}
        </div>
      )}

      <div className="detail-sections">
        <section className="steps-container">
          <h3>Execution Node Steps</h3>
          <ExecutionSteps steps={execution.steps} />
        </section>

        <section className="io-container">
          <h3>Execution Payload</h3>
          <div className="io-panels">
            <div className="io-panel">
              <h4>Workflow Input</h4>
              <pre className="mono-preview">
                {JSON.stringify(execution.input, null, 2)}
              </pre>
            </div>
            <div className="io-panel">
              <h4>Final Output</h4>
              <pre className="mono-preview">
                {JSON.stringify(execution.output, null, 2)}
              </pre>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
};
