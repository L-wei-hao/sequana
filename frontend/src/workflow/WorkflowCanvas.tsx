import { useMemo, type FC } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type Connection,
  type Edge as FlowEdge,
  type Node as FlowNode,
  type OnNodesChange,
  type OnEdgesChange,
  BackgroundVariant,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { WorkflowNode, type WorkflowNodeData } from "./WorkflowNode";

export type EditorNode = FlowNode<WorkflowNodeData>;
export type EditorEdge = FlowEdge<{ route?: string | null }>;

interface Props {
  nodes: EditorNode[];
  edges: EditorEdge[];
  onNodesChange: OnNodesChange<EditorNode>;
  onEdgesChange: OnEdgesChange<EditorEdge>;
  onConnect: (connection: Connection) => void;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onSelectNode: (node: EditorNode | null) => void;
  onSelectEdge: (edge: EditorEdge | null) => void;
}

export const WorkflowCanvas: FC<Props> = ({
  nodes,
  edges,
  onNodesChange,
  onEdgesChange,
  onConnect,
  onSelectNode,
  onSelectEdge,
}) => {
  const nodeTypes = useMemo(() => ({ workflowNode: WorkflowNode }), []);

  return (
    <div className="canvas-wrapper">
      <ReactFlow<EditorNode, EditorEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onNodeClick={(_, node) => {
          onSelectNode(node);
          onSelectEdge(null);
        }}
        onEdgeClick={(_, edge) => {
          onSelectEdge(edge);
          onSelectNode(null);
        }}
        onPaneClick={() => {
          onSelectNode(null);
          onSelectEdge(null);
        }}
        fitView
      >
        <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="#334155" />
        <Controls />
        <MiniMap
          nodeColor={(node) => {
            const data = node.data as WorkflowNodeData;
            switch (data?.nodeType) {
              case "manual_trigger":
              case "webhook":
              case "schedule":
                return "#3b82f6";
              case "open_ai":
                return "#8b5cf6";
              case "postgres":
                return "#10b981";
              case "if":
              case "switch":
              case "set":
                return "#f59e0b";
              default:
                return "#64748b";
            }
          }}
          maskColor="rgba(15, 23, 42, 0.7)"
        />
      </ReactFlow>
    </div>
  );
};
