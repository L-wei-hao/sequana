import { useState, type FC } from "react";
import type { CredentialSummary } from "../../types";

interface Props {
  config: Record<string, any>;
  credentials: CredentialSummary[];
  onChange: (updated: Record<string, any>) => void;
}

export const OpenAiSettings: FC<Props> = ({ config, credentials, onChange }) => {
  const openAiCredentials = credentials.filter((c) => c.kind === "openai");
  const output = config.output ?? { type: "text" };
  const isStructured = output.type === "json_schema";

  const [schemaText, setSchemaText] = useState(() =>
    output.schema ? JSON.stringify(output.schema, null, 2) : "{\n  \"type\": \"object\",\n  \"properties\": {},\n  \"required\": []\n}"
  );

  return (
    <div className="settings-form">
      <label>
        OpenAI Credential
        <select
          value={config.credential_id ?? ""}
          onChange={(e) => onChange({ ...config, credential_id: e.target.value })}
        >
          <option value="">Select credential...</option>
          {openAiCredentials.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </label>

      <div className="form-row">
        <label style={{ flex: 2 }}>
          Model
          <input
            type="text"
            value={config.model ?? "gpt-5.6-luna"}
            placeholder="gpt-5.6-luna, gpt-4o, o3-mini"
            onChange={(e) => onChange({ ...config, model: e.target.value })}
          />
        </label>

        <label style={{ flex: 1 }}>
          Reasoning Effort
          <select
            value={config.reasoning_effort ?? "medium"}
            onChange={(e) => onChange({ ...config, reasoning_effort: e.target.value })}
          >
            <option value="low">Low</option>
            <option value="medium">Medium</option>
            <option value="high">High</option>
          </select>
        </label>
      </div>

      <label>
        Instructions (System Prompt)
        <textarea
          rows={3}
          value={typeof config.instructions === "string" ? config.instructions : ""}
          placeholder="You are an expert candidate evaluation assistant..."
          onChange={(e) => onChange({ ...config, instructions: e.target.value })}
        />
      </label>

      <label>
        Input (Prompt Template)
        <textarea
          rows={4}
          value={typeof config.input === "string" ? config.input : JSON.stringify(config.input ?? "")}
          placeholder="{{$json.body.resume_text}}"
          onChange={(e) => onChange({ ...config, input: e.target.value })}
        />
        <span className="field-hint">
          Supports <code>{"{{$json.path}}"}</code> or <code>{"{\"$from\": \"/path\"}"}</code>
        </span>
      </label>

      <div className="form-row">
        <label style={{ flex: 1 }}>
          Output Format
          <select
            value={isStructured ? "json_schema" : "text"}
            onChange={(e) => {
              if (e.target.value === "json_schema") {
                onChange({
                  ...config,
                  output: {
                    type: "json_schema",
                    name: "output_schema",
                    strict: true,
                    schema: JSON.parse(schemaText || "{}"),
                  },
                });
              } else {
                onChange({
                  ...config,
                  output: { type: "text" },
                });
              }
            }}
          >
            <option value="text">Text / Unstructured</option>
            <option value="json_schema">Structured JSON Schema</option>
          </select>
        </label>

        <label style={{ flex: 1 }}>
          Max Output Tokens
          <input
            type="number"
            value={config.max_output_tokens ?? 2000}
            onChange={(e) =>
              onChange({
                ...config,
                max_output_tokens: parseInt(e.target.value, 10) || 2000,
              })
            }
          />
        </label>
      </div>

      {isStructured && (
        <label>
          JSON Schema Definition
          <textarea
            rows={8}
            className="mono-editor"
            value={schemaText}
            onChange={(e) => {
              setSchemaText(e.target.value);
              try {
                const schema = JSON.parse(e.target.value);
                onChange({
                  ...config,
                  output: {
                    type: "json_schema",
                    name: output.name || "candidate_eval",
                    strict: true,
                    schema,
                  },
                });
              } catch {}
            }}
          />
        </label>
      )}
    </div>
  );
};
