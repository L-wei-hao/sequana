import { useState, type FC, type FormEvent } from "react";

interface Props {
  onSave: (name: string, kind: string, value: any) => Promise<void>;
  onCancel: () => void;
}

export const CredentialForm: FC<Props> = ({ onSave, onCancel }) => {
  const [name, setName] = useState("");
  const [kind, setKind] = useState("openai");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // OpenAI
  const [apiKey, setApiKey] = useState("");

  // Postgres
  const [pgHost, setPgHost] = useState("");
  const [pgPort, setPgPort] = useState(5432);
  const [pgDb, setPgDb] = useState("");
  const [pgUser, setPgUser] = useState("");
  const [pgPass, setPgPass] = useState("");
  const [pgSsl, setPgSsl] = useState("prefer");

  // HTTP Bearer
  const [bearerToken, setBearerToken] = useState("");

  // HTTP Basic
  const [basicUser, setBasicUser] = useState("");
  const [basicPass, setBasicPass] = useState("");

  // HTTP Header
  const [headerName, setHeaderName] = useState("");
  const [headerValue, setHeaderValue] = useState("");

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Credential name is required");
      return;
    }

    let payload: any;
    if (kind === "openai") {
      if (!apiKey.trim()) {
        setError("API key is required");
        return;
      }
      payload = apiKey.trim();
    } else if (kind === "postgres") {
      if (!pgHost.trim() || !pgDb.trim() || !pgUser.trim()) {
        setError("Host, Database, and Username are required");
        return;
      }
      payload = {
        host: pgHost.trim(),
        port: pgPort,
        database: pgDb.trim(),
        username: pgUser.trim(),
        password: pgPass,
        ssl_mode: pgSsl,
      };
    } else if (kind === "http_bearer") {
      if (!bearerToken.trim()) {
        setError("Bearer token is required");
        return;
      }
      payload = bearerToken.trim();
    } else if (kind === "http_basic") {
      if (!basicUser.trim()) {
        setError("Username is required");
        return;
      }
      payload = {
        username: basicUser.trim(),
        password: basicPass,
      };
    } else if (kind === "http_header") {
      if (!headerName.trim() || !headerValue) {
        setError("Header name and value are required");
        return;
      }
      payload = {
        header_name: headerName.trim(),
        header_value: headerValue,
      };
    }

    setLoading(true);
    setError(null);
    try {
      await onSave(name.trim(), kind, payload);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <form className="credential-modal-card" onSubmit={handleSubmit}>
      <div className="card-header">
        <h3>Add Secure Credential</h3>
        <button type="button" className="btn-close" onClick={onCancel}>
          ✕
        </button>
      </div>

      {error && <div className="callout callout-error">{error}</div>}

      <div className="form-body">
        <label>
          Credential Name
          <input
            type="text"
            placeholder="e.g. Production OpenAI or Main DB"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
        </label>

        <label>
          Credential Type
          <select value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="openai">OpenAI API Key</option>
            <option value="postgres">PostgreSQL Database Connection</option>
            <option value="http_bearer">HTTP Bearer Token</option>
            <option value="http_basic">HTTP Basic Authentication</option>
            <option value="http_header">HTTP Custom Header Auth</option>
          </select>
        </label>

        {kind === "openai" && (
          <label>
            OpenAI API Key
            <input
              type="password"
              placeholder="sk-..."
              autoComplete="off"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
          </label>
        )}

        {kind === "postgres" && (
          <>
            <div className="form-row">
              <label style={{ flex: 3 }}>
                Host
                <input
                  type="text"
                  placeholder="db.example.com"
                  value={pgHost}
                  onChange={(e) => setPgHost(e.target.value)}
                />
              </label>
              <label style={{ flex: 1 }}>
                Port
                <input
                  type="number"
                  value={pgPort}
                  onChange={(e) => setPgPort(parseInt(e.target.value, 10) || 5432)}
                />
              </label>
            </div>

            <label>
              Database Name
              <input
                type="text"
                placeholder="production_db"
                value={pgDb}
                onChange={(e) => setPgDb(e.target.value)}
              />
            </label>

            <div className="form-row">
              <label style={{ flex: 1 }}>
                Username
                <input
                  type="text"
                  placeholder="postgres"
                  value={pgUser}
                  onChange={(e) => setPgUser(e.target.value)}
                />
              </label>
              <label style={{ flex: 1 }}>
                Password
                <input
                  type="password"
                  placeholder="••••••••"
                  autoComplete="off"
                  value={pgPass}
                  onChange={(e) => setPgPass(e.target.value)}
                />
              </label>
            </div>

            <label>
              SSL Mode
              <select value={pgSsl} onChange={(e) => setPgSsl(e.target.value)}>
                <option value="prefer">Prefer</option>
                <option value="require">Require</option>
                <option value="disable">Disable</option>
              </select>
            </label>
          </>
        )}

        {kind === "http_bearer" && (
          <label>
            Bearer Token
            <input
              type="password"
              placeholder="eyJhbGciOi..."
              autoComplete="off"
              value={bearerToken}
              onChange={(e) => setBearerToken(e.target.value)}
            />
          </label>
        )}

        {kind === "http_basic" && (
          <div className="form-row">
            <label style={{ flex: 1 }}>
              Username
              <input
                type="text"
                value={basicUser}
                onChange={(e) => setBasicUser(e.target.value)}
              />
            </label>
            <label style={{ flex: 1 }}>
              Password
              <input
                type="password"
                autoComplete="off"
                value={basicPass}
                onChange={(e) => setBasicPass(e.target.value)}
              />
            </label>
          </div>
        )}

        {kind === "http_header" && (
          <div className="form-row">
            <label style={{ flex: 1 }}>
              Header Name
              <input
                type="text"
                placeholder="X-API-KEY"
                value={headerName}
                onChange={(e) => setHeaderName(e.target.value)}
              />
            </label>
            <label style={{ flex: 1 }}>
              Header Value
              <input
                type="password"
                placeholder="secret-token"
                autoComplete="off"
                value={headerValue}
                onChange={(e) => setHeaderValue(e.target.value)}
              />
            </label>
          </div>
        )}

        <div className="info-box">
          <small>
            🔒 Credentials are encrypted at rest with AES-256-GCM using your tenant key. Plaintext
            secrets never leave the server.
          </small>
        </div>
      </div>

      <div className="card-footer">
        <button type="button" className="btn-secondary" onClick={onCancel} disabled={loading}>
          Cancel
        </button>
        <button type="submit" className="btn-primary" disabled={loading}>
          {loading ? "Encrypting..." : "Save Credential"}
        </button>
      </div>
    </form>
  );
};
