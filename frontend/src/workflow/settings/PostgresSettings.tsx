import { useState, type FC } from "react";
import type { CredentialSummary } from "../../types";

interface Props {
  config: Record<string, any>;
  credentials: CredentialSummary[];
  onChange: (updated: Record<string, any>) => void;
}

export const PostgresSettings: FC<Props> = ({ config, credentials, onChange }) => {
  const pgCredentials = credentials.filter((c) => c.kind === "postgres");
  const mode = config.mode ?? "query";
  const [paramsText, setParamsText] = useState(() =>
    JSON.stringify(config.params ?? [], null, 2)
  );

  return (
    <div className="settings-form">
      <label>
        Database Connection (PostgreSQL Credential)
        <select
          value={config.credential_id ?? ""}
          onChange={(e) =>
            onChange({
              ...config,
              credential_id: e.target.value ? e.target.value : undefined,
            })
          }
        >
          <option value="">Sequana Internal Database (Local default)</option>
          {pgCredentials.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </label>

      <div className="form-row">
        <label style={{ flex: 1 }}>
          Execution Mode
          <select
            value={mode}
            onChange={(e) => onChange({ ...config, mode: e.target.value })}
          >
            <option value="query">SELECT Query (Returns Rows Array)</option>
            <option value="execute">INSERT/UPDATE/DELETE (Returns Rows Affected)</option>
          </select>
        </label>
      </div>

      <label>
        SQL Query
        <textarea
          rows={6}
          className="mono-editor"
          value={config.query ?? ""}
          placeholder="SELECT * FROM candidates WHERE status = $1"
          onChange={(e) => onChange({ ...config, query: e.target.value })}
        />
        <span className="field-hint">
          Use parameterized placeholders (<code>$1</code>, <code>$2</code>) to prevent SQL injection.
        </span>
      </label>

      <label>
        Query Parameters JSON (Optional)
        <textarea
          rows={5}
          className="mono-editor"
          value={paramsText}
          placeholder='[{"type": "text", "value": "{{$json.candidate.id}}"}]'
          onChange={(e) => {
            setParamsText(e.target.value);
            try {
              onChange({ ...config, params: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
        <span className="field-hint">
          Types: <code>text</code>, <code>i64</code>, <code>f64</code>, <code>bool</code>, <code>uuid</code>, <code>json</code>
        </span>
      </label>
    </div>
  );
};
