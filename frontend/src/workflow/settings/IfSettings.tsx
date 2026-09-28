import { useState, type FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const IfSettings: FC<Props> = ({ config, onChange }) => {
  const operator = config.operator ?? "eq";
  const [leftText, setLeftText] = useState(() =>
    typeof config.left === "object"
      ? JSON.stringify(config.left)
      : String(config.left ?? "{{$json.qualified}}")
  );
  const [rightText, setRightText] = useState(() =>
    typeof config.right === "object"
      ? JSON.stringify(config.right)
      : String(config.right ?? "true")
  );

  const needsRight = !["truthy", "falsy", "exists", "is_empty"].includes(operator);

  return (
    <div className="settings-form">
      <div className="info-box">
        <strong>Branching Outputs:</strong>
        <p>Connect downstream nodes and set edge route labels to <code>true</code> or <code>false</code>.</p>
      </div>

      <label>
        Left Expression / Value
        <input
          type="text"
          value={leftText}
          placeholder="{{$json.score}}"
          onChange={(e) => {
            setLeftText(e.target.value);
            try {
              onChange({ ...config, left: JSON.parse(e.target.value) });
            } catch {
              onChange({ ...config, left: e.target.value });
            }
          }}
        />
      </label>

      <label>
        Comparison Operator
        <select
          value={operator}
          onChange={(e) => onChange({ ...config, operator: e.target.value })}
        >
          <option value="eq">Equals (=)</option>
          <option value="ne">Not Equals (!=)</option>
          <option value="gt">Greater Than (&gt;)</option>
          <option value="gte">Greater Than or Equal (&gt;=)</option>
          <option value="lt">Less Than (&lt;)</option>
          <option value="lte">Less Than or Equal (&lt;=)</option>
          <option value="contains">Contains (substring / array)</option>
          <option value="exists">Exists (is not null)</option>
          <option value="is_empty">Is Empty (empty string/array/obj/null)</option>
          <option value="truthy">Truthy</option>
          <option value="falsy">Falsy</option>
        </select>
      </label>

      {needsRight && (
        <label>
          Right Comparison Value
          <input
            type="text"
            value={rightText}
            placeholder="80"
            onChange={(e) => {
              setRightText(e.target.value);
              try {
                onChange({ ...config, right: JSON.parse(e.target.value) });
              } catch {
                onChange({ ...config, right: e.target.value });
              }
            }}
          />
        </label>
      )}
    </div>
  );
};
