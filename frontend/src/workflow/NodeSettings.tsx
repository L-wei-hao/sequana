import { useState, type FC } from "react";
import type { Node as FlowNode } from "@xyflow/react";
import type { CredentialSummary } from "../types";
import { NODE_CATALOG_MAP } from "./nodeCatalog";
import type { WorkflowNodeData } from "./WorkflowNode";
import { WebhookSettings } from "./settings/WebhookSettings";
import { RespondSettings } from "./settings/RespondSettings";
import { HttpSettings } from "./settings/HttpSettings";
import { OpenAiSettings } from "./settings/OpenAiSettings";
import { PostgresSettings } from "./settings/PostgresSettings";
import { SetSettings } from "./settings/SetSettings";
import { IfSettings } from "./settings/IfSettings";
import { SwitchSettings } from "./settings/SwitchSettings";
import { ScheduleSettings } from "./settings/ScheduleSettings";

interface Props {
  node: FlowNode<WorkflowNodeData> | null;
  credentials: CredentialSummary[];
  onUpdateConfig: (config: Record<string, any>) => void;
  onDeleteNode: (id: string) => void;
  onTestNode: (node: FlowNode<WorkflowNodeData>, input: unknown) => Promise<unknown>;
}

export const NodeSettings: FC<Props> = ({
  node,
  credentials,
  onUpdateConfig,
  onDeleteNode,
  onTestNode,
}) => {
  const [showRawJson, setShowRawJson] = useState(false);
  const [testInput, setTestInput] = useState("{}");
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testError, setTestError] = useState<string | null>(null);

  if (!node) {
    return (
      <div className="settings-empty">
        <p className="muted">Select a node to inspect and configure its properties.</p>
      </div>
    );
  }

  const catalog = NODE_CATALOG_MAP[node.data.nodeType];
  const config = node.data.config || {};

  const handleRunTest = async () => {
    setTesting(true);
    setTestError(null);
    setTestResult(null);
    try {
      let parsed = {};
      try {
        parsed = JSON.parse(testInput);
      } catch {
        throw new Error("Test input must be valid JSON");
      }
      const res = await onTestNode(node, parsed);
      setTestResult(JSON.stringify(res, null, 2));
    } catch (err: any) {
      setTestError(err.message || String(err));
    } finally {
      setTesting(false);
    }
  };

  const renderSpecificSettings = () => {
    switch (node.data.nodeType) {
      case "webhook":
        return <WebhookSettings config={config} onChange={onUpdateConfig} />;
      case "respond_to_webhook":
        return <RespondSettings config={config} onChange={onUpdateConfig} />;
      case "http_request":
        return (
          <HttpSettings
            config={config}
            credentials={credentials}
            onChange={onUpdateConfig}
          />
        );
      case "open_ai":
        return (
          <OpenAiSettings
            config={config}
            credentials={credentials}
            onChange={onUpdateConfig}
          />
        );
      case "postgres":
        return (
          <PostgresSettings
            config={config}
            credentials={credentials}
            onChange={onUpdateConfig}
          />
        );
      case "set":
        return <SetSettings config={config} onChange={onUpdateConfig} />;
      case "if":
        return <IfSettings config={config} onChange={onUpdateConfig} />;
      case "switch":
        return <SwitchSettings config={config} onChange={onUpdateConfig} />;
      case "schedule":
        return <ScheduleSettings config={config} onChange={onUpdateConfig} />;
      default:
        return <p className="muted">No specific options for this trigger.</p>;
    }
  };

  return (
    <div className="node-settings-panel">
      <div className="panel-header">
        <div>
          <h3>{catalog?.label || node.data.label}</h3>
          <span className="node-id-chip">{node.id}</span>
        </div>
        <button
          className="btn-danger-outline btn-sm"
          onClick={() => onDeleteNode(node.id)}
          title="Delete Node"
        >
          Delete
        </button>
      </div>

      <div className="settings-mode-switch">
        <button
          className={`switch-tab ${!showRawJson ? "active" : ""}`}
          onClick={() => setShowRawJson(false)}
        >
          Form View
        </button>
        <button
          className={`switch-tab ${showRawJson ? "active" : ""}`}
          onClick={() => setShowRawJson(true)}
        >
          Raw JSON
        </button>
      </div>

      <div className="panel-body">
        {showRawJson ? (
          <div className="settings-form">
            <label>
              Node Config JSON
              <textarea
                rows={12}
                className="mono-editor"
                value={JSON.stringify(config, null, 2)}
                onChange={(e) => {
                  try {
                    onUpdateConfig(JSON.parse(e.target.value));
                  } catch {}
                }}
              />
            </label>
          </div>
        ) : (
          renderSpecificSettings()
        )}

        <hr className="divider" />

        <div className="test-runner-section">
          <h4>Test Node Execution</h4>
          <p className="field-hint">Run this single node in isolation with sample JSON input.</p>

          <label>
            Mock Input JSON
            <textarea
              rows={4}
              className="mono-editor"
              value={testInput}
              onChange={(e) => setTestInput(e.target.value)}
            />
          </label>

          <button
            className="btn-secondary btn-sm"
            onClick={handleRunTest}
            disabled={testing}
          >
            {testing ? "Testing..." : "Execute Test"}
          </button>

          {testError && <div className="callout callout-error">{testError}</div>}

          {testResult && (
            <div className="test-result-box">
              <span className="result-label">Output Result</span>
              <pre>{testResult}</pre>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
