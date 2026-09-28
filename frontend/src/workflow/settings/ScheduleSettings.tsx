import type { FC } from "react";

interface Props {
  config: Record<string, any>;
  onChange: (updated: Record<string, any>) => void;
}

export const ScheduleSettings: FC<Props> = ({ config, onChange }) => {
  return (
    <div className="settings-form">
      <label>
        Cron Expression
        <input
          type="text"
          value={config.cron ?? "0 10 * * *"}
          placeholder="0 10 * * * (Every day at 10:00)"
          onChange={(e) => onChange({ ...config, cron: e.target.value })}
        />
        <span className="field-hint">
          Supports 5-field (standard) or 6/7-field cron expressions.
        </span>
      </label>

      <label>
        Timezone
        <input
          type="text"
          value={config.timezone ?? "Asia/Singapore"}
          placeholder="Asia/Singapore or UTC"
          onChange={(e) => onChange({ ...config, timezone: e.target.value })}
        />
      </label>
    </div>
  );
};
