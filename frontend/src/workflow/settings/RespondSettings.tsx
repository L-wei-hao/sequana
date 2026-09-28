import { useState, type FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const RespondSettings: FC<Props> = ({ config, onChange }) => {
  const status = config.status ?? 200;
  const [bodyText, setBodyText] = useState(() =>
    JSON.stringify(config.body ?? { status: "ok" }, null, 2)
  );

  return (
    <div className="settings-form">
      <label>
        HTTP Status Code
        <input
          type="number"
          min={100}
          max={599}
          value={status}
          onChange={(e) =>
            onChange({ ...config, status: parseInt(e.target.value, 10) || 200 })
          }
        />
      </label>

      <label>
        Response Body (JSON or Expression Template)
        <textarea
          rows={6}
          value={bodyText}
          onChange={(e) => {
            setBodyText(e.target.value);
            try {
              const parsed = JSON.parse(e.target.value);
              onChange({ ...config, body: parsed });
            } catch {
              onChange({ ...config, body: e.target.value });
            }
          }}
        />
        <span className="field-hint">
          Supports <code>{"{\"$from\": \"/key\"}"}</code> or <code>{"{{$json.key}}"}</code>
        </span>
      </label>
    </div>
  );
};
