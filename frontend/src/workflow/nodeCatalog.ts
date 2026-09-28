import type { NodeType } from "../types";

export interface NodeCatalogItem {
  type: NodeType;
  label: string;
  category: "trigger" | "action" | "logic" | "ai" | "data";
  description: string;
  defaultConfig: Record<string, unknown>;
  icon: string;
}

export const NODE_CATALOG: NodeCatalogItem[] = [
  {
    type: "manual_trigger",
    label: "Manual Trigger",
    category: "trigger",
    description: "Start workflow manually for testing or ad-hoc runs",
    defaultConfig: {},
    icon: "play",
  },
  {
    type: "webhook",
    label: "Webhook",
    category: "trigger",
    description: "Receive incoming HTTP calls with GET, POST, PUT, DELETE",
    defaultConfig: { path: "/candidate" },
    icon: "webhook",
  },
  {
    type: "schedule",
    label: "Schedule",
    category: "trigger",
    description: "Trigger runs on a cron schedule in a designated timezone",
    defaultConfig: { cron: "0 10 * * *", timezone: "Asia/Singapore", input: {} },
    icon: "clock",
  },
  {
    type: "set",
    label: "Set / Transform",
    category: "logic",
    description: "Map, rename, remove fields or transform JSON with templates",
    defaultConfig: { values: {}, rename: {}, remove: [], merge_input: true },
    icon: "transform",
  },
  {
    type: "if",
    label: "If Condition",
    category: "logic",
    description: "Branch execution based on boolean, comparisons, or pattern checks",
    defaultConfig: { left: { "$from": "/value" }, operator: "eq", right: true },
    icon: "git-branch",
  },
  {
    type: "switch",
    label: "Switch",
    category: "logic",
    description: "Route to multiple paths depending on matching case values",
    defaultConfig: { value: { "$from": "/status" }, cases: [], default_route: "default" },
    icon: "git-merge",
  },
  {
    type: "open_ai",
    label: "OpenAI",
    category: "ai",
    description: "Call OpenAI Responses API with structured JSON Schema",
    defaultConfig: {
      credential_id: "",
      model: "gpt-5.6-luna",
      input: { "$from": "/prompt" },
      output: { type: "text" },
      max_output_tokens: 2000,
      reasoning_effort: "medium",
    },
    icon: "sparkles",
  },
  {
    type: "postgres",
    label: "PostgreSQL",
    category: "data",
    description: "Execute SQL queries or updates with safe parameterized inputs",
    defaultConfig: { query: "SELECT 1 AS ok", mode: "query", params: [] },
    icon: "database",
  },
  {
    type: "http_request",
    label: "HTTP Request",
    category: "action",
    description: "Make outbound HTTP API requests with credentials and headers",
    defaultConfig: { method: "GET", url: "https://api.example.com", headers: {} },
    icon: "globe",
  },
  {
    type: "respond_to_webhook",
    label: "Respond to Webhook",
    category: "action",
    description: "Return custom HTTP status code and response payload to the caller",
    defaultConfig: { status: 200, headers: {}, body: { "$from": "/" } },
    icon: "reply",
  },
];

export const NODE_CATALOG_MAP = Object.fromEntries(
  NODE_CATALOG.map((item) => [item.type, item])
) as Record<NodeType, NodeCatalogItem>;
