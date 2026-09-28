import { useState, type FC } from "react";
import type { ExecutionStep } from "../types";

interface Props {
  steps: ExecutionStep[];
}

export const ExecutionSteps: FC<Props> = ({ steps }) => {
  const [expandedId, setExpandedId] = useState<string | null>(() => steps[0]?.id || null);

  if (steps.length === 0) {
    return <p className="muted">No execution steps recorded.</p>;
  }

  return (
    <div className="execution-steps-timeline">
      {steps.map((step, index) => {
        const isExpanded = expandedId === step.id;
        return (
          <div key={step.id} className={`step-card status-${step.status}`}>
            <div
              className="step-card-header"
              onClick={() => setExpandedId(isExpanded ? null : step.id)}
            >
              <div className="step-badge-col">
                <span className="step-index">{index + 1}</span>
                <span className={`status-badge status-${step.status}`}>
                  {step.status}
                </span>
              </div>

              <div className="step-info-col">
                <div className="step-node-title">
                  <strong>{step.node_id}</strong>
                  <span className="step-node-type">({step.node_type})</span>
                </div>
                <div className="step-meta">
                  {step.duration_ms != null && (
                    <span>⏱ {step.duration_ms} ms</span>
                  )}
                  {step.started_at && (
                    <span className="step-time">{new Date(step.started_at).toLocaleTimeString()}</span>
                  )}
                </div>
              </div>

              <div className="step-expand-toggle">
                {isExpanded ? "▲" : "▼"}
              </div>
            </div>

            {step.error && (
              <div className="step-error-banner">
                <strong>Error:</strong> {step.error}
              </div>
            )}

            {isExpanded && (
              <div className="step-card-content">
                {step.metadata && (
                  <div className="step-section">
                    <h5>Usage & Metadata</h5>
                    <pre className="mono-preview">
                      {JSON.stringify(step.metadata, null, 2)}
                    </pre>
                  </div>
                )}

                <div className="step-io-grid">
                  <div className="step-section">
                    <h5>Input</h5>
                    <pre className="mono-preview">
                      {JSON.stringify(step.input, null, 2)}
                    </pre>
                  </div>

                  <div className="step-section">
                    <h5>Output</h5>
                    <pre className="mono-preview">
                      {JSON.stringify(step.output, null, 2)}
                    </pre>
                  </div>
                </div>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
};
