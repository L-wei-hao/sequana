import { useState, type FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const SetSettings: FC<Props> = ({ config, onChange }) => {
  const [valuesText, setValuesText] = useState(() =>
    JSON.stringify(config.values ?? {}, null, 2)
  );
  const [renameText, setRenameText] = useState(() =>
    JSON.stringify(config.rename ?? {}, null, 2)
  );
  const [removeText, setRemoveText] = useState(() =>
    JSON.stringify(config.remove ?? [], null, 2)
  );

  return (
    <div className="settings-form">
      <label className="checkbox-label" style={{ marginBottom: "16px" }}>
        <input
          type="checkbox"
          checked={config.merge_input ?? true}
          onChange={(e) => onChange({ ...config, merge_input: e.target.checked })}
        />
        Merge Output into Incoming Input Object
      </label>

      <label>
        Set Fields (JSON Object)
        <textarea
          rows={6}
          className="mono-editor"
          value={valuesText}
          placeholder='{"score": "{{$json.eval.score}}", "reviewed": true}'
          onChange={(e) => {
            setValuesText(e.target.value);
            try {
              onChange({ ...config, values: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
        <span className="field-hint">
          Set keys or use expression templates (<code>{"{{$json.path}}"}</code> or <code>{"{\"$from\": \"/path\"}"}</code>)
        </span>
      </label>

      <label>
        Rename Fields (JSON Object)
        <textarea
          rows={4}
          className="mono-editor"
          value={renameText}
          placeholder='{"old_key": "new_key"}'
          onChange={(e) => {
            setRenameText(e.target.value);
            try {
              onChange({ ...config, rename: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
      </label>

      <label>
        Remove Fields (JSON Array)
        <textarea
          rows={3}
          className="mono-editor"
          value={removeText}
          placeholder='["unneeded_field", "temp"]'
          onChange={(e) => {
            setRemoveText(e.target.value);
            try {
              onChange({ ...config, remove: JSON.parse(e.target.value) });
            } catch {}
          }}
        />
      </label>
    </div>
  );
};
