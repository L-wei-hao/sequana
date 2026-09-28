import { useState, type FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const SwitchSettings: FC<Props> = ({ config, onChange }) => {
  const [valueText, setValueText] = useState(() =>
    typeof config.value === "object"
      ? JSON.stringify(config.value)
      : String(config.value ?? "{{$json.status}}")
  );
  const [casesText, setCasesText] = useState(() =>
    JSON.stringify(
      config.cases ?? [
        { equals: "approved", route: "approved" },
        { equals: "rejected", route: "rejected" },
      ],
      null,
      2
    )
  );

  return (
    <div className="settings-form">
      <div className="info-box">
        <strong>Multi-Branch Routing:</strong>
        <p>Label outgoing edges with matching route names.</p>
      </div>

      <label>
        Evaluated Expression / Value
        <input
          type="text"
          value={valueText}
          placeholder="{{$json.status}}"
          onChange={(e) => {
            setValueText(e.target.value);
            try {
              onChange({ ...config, value: JSON.parse(e.target.value) });
            } catch {
              onChange({ ...config, value: e.target.value });
            }
          }}
        />
      </label>

      <label>
        Cases JSON (Array of {`{"equals": ..., "route": ...}`})
        <textarea
          rows={6}
          className="mono-editor"
          value={casesText}
          onChange={(e) => {
            setCasesText(e.target.value);
            try {
              onChange({ ...config, cases: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
      </label>

      <label>
        Default Route
        <input
          type="text"
          value={config.default_route ?? "default"}
          placeholder="default"
          onChange={(e) => onChange({ ...config, default_route: e.target.value })}
        />
      </label>
    </div>
  );
};
