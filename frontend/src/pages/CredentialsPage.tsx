import { useEffect, useState, type FC } from "react";
import type { ApiAuth } from "../api";
import { createCredential, deleteCredential, listCredentials } from "../api";
import type { CredentialSummary } from "../types";
import { CredentialForm } from "../credentials/CredentialForm";

interface Props {
  auth: ApiAuth;
}

export const CredentialsPage: FC<Props> = ({ auth }) => {
  const [credentials, setCredentials] = useState<CredentialSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAddModal, setShowAddModal] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const fetchCredentials = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await listCredentials(auth);
      setCredentials(data);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchCredentials();
  }, [auth.token, auth.tenantId]);

  const handleSaveCredential = async (name: string, kind: string, value: any) => {
    await createCredential(name, kind, value, auth);
    setShowAddModal(false);
    setNotice(`Credential "${name}" encrypted and saved.`);
    await fetchCredentials();
  };

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`Delete credential "${name}"? Workflows using this credential will fail.`)) {
      return;
    }
    try {
      await deleteCredential(id, auth);
      setNotice(`Credential "${name}" deleted.`);
      await fetchCredentials();
    } catch (err: any) {
      setError(err.message || String(err));
    }
  };

  return (
    <div className="page-shell">
      <div className="page-header">
        <div>
          <h2>Credentials</h2>
          <p className="page-subtitle">
            Securely manage encrypted API keys, database credentials, and HTTP tokens.
          </p>
        </div>
        <button className="btn-primary" onClick={() => setShowAddModal(true)}>
          + Add Credential
        </button>
      </div>

      {notice && (
        <div className="callout callout-success banner-alert">
          {notice}
          <button onClick={() => setNotice(null)}>✕</button>
        </div>
      )}
      {error && (
        <div className="callout callout-error banner-alert">
          {error}
          <button onClick={() => setError(null)}>✕</button>
        </div>
      )}

      <div className="security-notice-card">
        <div className="security-icon">🛡️</div>
        <div>
          <h4>AES-256-GCM Tenant-Bounded Security</h4>
          <p className="muted">
            All credentials are encrypted at rest with your unique tenant identity. Workflow definitions
            store only the Credential ID. Plaintext secrets are never returned to the frontend.
          </p>
        </div>
      </div>

      {loading ? (
        <div className="loading-indicator">Loading credentials...</div>
      ) : credentials.length === 0 ? (
        <div className="empty-state-box">
          <div className="empty-icon">🔑</div>
          <h3>No credentials stored</h3>
          <p className="muted">Add OpenAI API keys or database connections to use them in nodes.</p>
          <button className="btn-primary" onClick={() => setShowAddModal(true)}>
            + Add First Credential
          </button>
        </div>
      ) : (
        <div className="credential-grid">
          {credentials.map((cred) => (
            <div key={cred.id} className="credential-card">
              <div className="cred-card-header">
                <div>
                  <h4 className="cred-name">{cred.name}</h4>
                  <span className={`tag tag-${cred.kind}`}>{cred.kind.toUpperCase()}</span>
                </div>
                <button
                  className="btn-danger-outline btn-sm"
                  onClick={() => handleDelete(cred.id, cred.name)}
                >
                  Delete
                </button>
              </div>

              <div className="cred-card-body">
                <div className="cred-field">
                  <span className="field-label">Credential ID:</span>
                  <code className="id-code">{cred.id}</code>
                </div>
                <div className="cred-field">
                  <span className="field-label">Created:</span>
                  <span className="muted">{new Date(cred.created_at).toLocaleDateString()}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {showAddModal && (
        <div className="modal-backdrop">
          <CredentialForm
            onSave={handleSaveCredential}
            onCancel={() => setShowAddModal(false)}
          />
        </div>
      )}
    </div>
  );
};
