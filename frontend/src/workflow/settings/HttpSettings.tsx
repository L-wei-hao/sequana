import { useState, type FC } from "react";
import type { CredentialSummary } from "../../types";

interface Props {
  config: Record<string, any>;
  credentials: CredentialSummary[];
  onChange: (updated: Record<string, any>) => void;
}

export const HttpSettings: FC<Props> = ({ config, credentials, onChange }) => {
  const method = config.method ?? "GET";
  const url = typeof config.url === "string" ? config.url : JSON.stringify(config.url ?? "");
  const [headersText, setHeadersText] = useState(() =>
    JSON.stringify(config.headers ?? {}, null, 2)
  );
  const [bodyText, setBodyText] = useState(() =>
    config.body ? JSON.stringify(config.body, null, 2) : ""
  );

  const httpCredentials = credentials.filter((c) =>
    ["http_bearer", "http_basic", "http_header", "openai"].includes(c.kind)
  );

  return (
    <div className="settings-form">
      <div className="form-row">
        <label style={{ flex: "0 0 120px" }}>
          Method
          <select
            value={method}
            onChange={(e) => onChange({ ...config, method: e.target.value })}
          >
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="PATCH">PATCH</option>
            <option value="DELETE">DELETE</option>
          </select>
        </label>

        <label style={{ flex: 1 }}>
          URL
          <input
            type="text"
            value={url}
            placeholder="https://api.example.com/v1/resource"
            onChange={(e) => onChange({ ...config, url: e.target.value })}
          />
        </label>
      </div>

      <label>
        Authentication Credential (Optional)
        <select
          value={config.credential_id ?? ""}
          onChange={(e) =>
            onChange({
              ...config,
              credential_id: e.target.value ? e.target.value : undefined,
            })
          }
        >
          <option value="">No credential (public or manual headers)</option>
          {httpCredentials.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name} ({c.kind})
            </option>
          ))}
        </select>
      </label>

      <label>
        Headers JSON
        <textarea
          rows={4}
          value={headersText}
          onChange={(e) => {
            setHeadersText(e.target.value);
            try {
              onChange({ ...config, headers: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
      </label>

      {["POST", "PUT", "PATCH"].includes(method) && (
        <label>
          Request Body JSON / Template
          <textarea
            rows={5}
            value={bodyText}
            placeholder='{"data": "{{$json.payload}}"}'
            onChange={(e) => {
              setBodyText(e.target.value);
              try {
                onChange({ ...config, body: JSON.parse(e.target.value) });
              } catch {
                onChange({ ...config, body: e.target.value });
              }
            }}
          />
        </label>
      )}

      <div className="form-row">
        <label style={{ flex: 1 }}>
          Timeout (ms)
          <input
            type="number"
            value={config.timeout_ms ?? 30000}
            onChange={(e) =>
              onChange({
                ...config,
                timeout_ms: parseInt(e.target.value, 10) || undefined,
              })
            }
          />
        </label>

        <label className="checkbox-label" style={{ flex: 1, marginTop: "24px" }}>
          <input
            type="checkbox"
            checked={Boolean(config.continue_on_http_error)}
            onChange={(e) =>
              onChange({ ...config, continue_on_http_error: e.target.checked })
            }
          />
          Continue on HTTP Error
        </label>
      </div>
    </div>
  );
};
