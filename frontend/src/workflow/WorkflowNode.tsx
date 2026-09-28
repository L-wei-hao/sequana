import { memo } from "react";
import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { NodeType } from "../types";
import { NODE_CATALOG_MAP } from "./nodeCatalog";

export interface WorkflowNodeData extends Record<string, unknown> {
  label: string;
  nodeType: NodeType;
  config: Record<string, unknown>;
  subtitle?: string;
}

export const WorkflowNode = memo(({ id, data, selected }: NodeProps) => {
  const nodeData = data as WorkflowNodeData;
  const catalogItem = NODE_CATALOG_MAP[nodeData.nodeType];
  const isTrigger = catalogItem?.category === "trigger";
  const isTerminal = nodeData.nodeType === "respond_to_webhook";

  return (
    <div className={`workflow-node ${catalogItem?.category || "generic"} ${selected ? "selected" : ""}`}>
      {!isTrigger && (
        <Handle
          type="target"
          position={Position.Left}
          className="node-handle target-handle"
        />
      )}

      <div className="node-header">
        <div className={`node-icon-badge ${catalogItem?.category || "generic"}`}>
          {getCategoryIcon(catalogItem?.category || "generic")}
        </div>
        <div className="node-title-group">
          <div className="node-title">{nodeData.label || catalogItem?.label || id}</div>
          <div className="node-subtitle">{id}</div>
        </div>
      </div>

      <div className="node-body">
        <span className={`category-tag ${catalogItem?.category || "generic"}`}>
          {catalogItem?.category.toUpperCase() || "NODE"}
        </span>
      </div>

      {!isTerminal && (
        <Handle
          type="source"
          position={Position.Right}
          className="node-handle source-handle"
        />
      )}
    </div>
  );
});

WorkflowNode.displayName = "WorkflowNode";

function getCategoryIcon(category: string) {
  switch (category) {
    case "trigger":
      return "⚡";
    case "ai":
      return "✨";
    case "data":
      return "🗄️";
    case "logic":
      return "🔀";
    case "action":
      return "🌐";
    default:
      return "📦";
  }
}
