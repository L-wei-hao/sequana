import type { FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const WebhookSettings: FC<Props> = ({ config, onChange }) => {
  const path = config.path ?? "/candidate";

  return (
    <div className="settings-form">
      <label>
        Webhook Path / Slug
        <input
          type="text"
          value={path}
          placeholder="/candidate"
          onChange={(e) => onChange({ ...config, path: e.target.value })}
        />
        <span className="field-hint">
          Endpoint will be reachable at <code>/webhook{path.startsWith("/") ? path : `/${path}`}</code>
        </span>
      </label>
      <div className="info-box">
        <strong>Supported HTTP Methods:</strong>
        <p>GET, POST, PUT, PATCH, DELETE</p>
      </div>
    </div>
  );
};
