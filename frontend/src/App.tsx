import { useState } from "react";
import type { ApiAuth } from "./api";
import { WorkflowsPage } from "./pages/WorkflowsPage";
import { WorkflowEditorPage } from "./pages/WorkflowEditorPage";
import { ExecutionsPage } from "./pages/ExecutionsPage";
import { ExecutionDetailPage } from "./pages/ExecutionDetailPage";
import { CredentialsPage } from "./pages/CredentialsPage";

type View =
  | { type: "workflows" }
  | { type: "editor"; workflowId: string }
  | { type: "executions" }
  | { type: "execution_detail"; executionId: string }
  | { type: "credentials" };

export default function App() {
  const [token, setToken] = useState(() => sessionStorage.getItem("sequana.token") ?? "");
  const [tenantId, setTenantId] = useState(() => sessionStorage.getItem("sequana.tenant") ?? "");
  const [connected, setConnected] = useState(() => Boolean(token && tenantId));
  const [view, setView] = useState<View>({ type: "workflows" });
  const [authError, setAuthError] = useState("");

  const handleConnect = (e: React.FormEvent) => {
    e.preventDefault();
    if (!token.trim() || !tenantId.trim()) {
      setAuthError("Admin token and tenant ID are required.");
      return;
    }
    sessionStorage.setItem("sequana.token", token.trim());
    sessionStorage.setItem("sequana.tenant", tenantId.trim());
    setConnected(true);
    setAuthError("");
  };

  const handleDisconnect = () => {
    sessionStorage.removeItem("sequana.token");
    sessionStorage.removeItem("sequana.tenant");
    setConnected(false);
    setView({ type: "workflows" });
  };

  const auth: ApiAuth = {
    token: token.trim(),
    tenantId: tenantId.trim(),
  };

  if (!connected) {
    return (
      <main className="connect-shell">
        <div className="connect-card">
          <div className="brand-logo">
            <span className="brand-icon">🌊</span>
            <h1>Sequana</h1>
          </div>
          <p className="brand-tagline">
            Lightweight workflow automation, built for AI.
          </p>

          <form onSubmit={handleConnect}>
            <label>
              Tenant ID (UUID)
              <input
                type="text"
                value={tenantId}
                placeholder="00000000-0000-0000-0000-000000000001"
                onChange={(e) => setTenantId(e.target.value)}
                required
              />
            </label>

            <label>
              Admin API Bearer Token
              <input
                type="password"
                value={token}
                placeholder="SEQUANA_ADMIN_TOKEN"
                onChange={(e) => setToken(e.target.value)}
                required
              />
            </label>

            {authError && <div className="callout callout-error">{authError}</div>}

            <button type="submit" className="btn-primary btn-block">
              Connect to Sequana Engine →
            </button>
          </form>
        </div>
      </main>
    );
  }

  const activeNav =
    view.type === "workflows" || view.type === "editor"
      ? "workflows"
      : view.type === "executions" || view.type === "execution_detail"
      ? "executions"
      : "credentials";

  return (
    <div className="app-layout">
      <header className="app-topbar">
        <div className="brand-area" onClick={() => setView({ type: "workflows" })}>
          <span className="brand-icon">🌊</span>
          <span className="brand-name">Sequana</span>
          <span className="badge-v1">V1</span>
        </div>

        <nav className="nav-tabs">
          <button
            className={`nav-tab ${activeNav === "workflows" ? "active" : ""}`}
            onClick={() => setView({ type: "workflows" })}
          >
            Workflows
          </button>
          <button
            className={`nav-tab ${activeNav === "executions" ? "active" : ""}`}
            onClick={() => setView({ type: "executions" })}
          >
            Executions
          </button>
          <button
            className={`nav-tab ${activeNav === "credentials" ? "active" : ""}`}
            onClick={() => setView({ type: "credentials" })}
          >
            Credentials
          </button>
        </nav>

        <div className="topbar-user">
          <div className="tenant-chip" title={tenantId}>
            <span className="tenant-dot"></span>
            Tenant: <code>{tenantId.slice(0, 8)}...</code>
          </div>
          <button className="btn-secondary btn-sm" onClick={handleDisconnect}>
            Disconnect
          </button>
        </div>
      </header>

      <main className="app-main-content">
        {view.type === "workflows" && (
          <WorkflowsPage
            auth={auth}
            onSelectWorkflow={(id) => setView({ type: "editor", workflowId: id })}
          />
        )}

        {view.type === "editor" && (
          <WorkflowEditorPage
            workflowId={view.workflowId}
            auth={auth}
            onBack={() => setView({ type: "workflows" })}
            onNavigateToExecution={(id) =>
              setView({ type: "execution_detail", executionId: id })
            }
          />
        )}

        {view.type === "executions" && (
          <ExecutionsPage
            auth={auth}
            onSelectExecution={(id) =>
              setView({ type: "execution_detail", executionId: id })
            }
          />
        )}

        {view.type === "execution_detail" && (
          <ExecutionDetailPage
            executionId={view.executionId}
            auth={auth}
            onBack={() => setView({ type: "executions" })}
            onSelectExecution={(id) =>
              setView({ type: "execution_detail", executionId: id })
            }
          />
        )}

        {view.type === "credentials" && <CredentialsPage auth={auth} />}
      </main>
    </div>
  );
}
