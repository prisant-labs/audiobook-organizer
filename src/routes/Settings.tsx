import { useState } from "react";
import { STRINGS } from "@/lib/strings";
import { pickLibraryFolder } from "@/lib/dialog";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import type { AppSettings } from "@/lib/settings";
import { ThemeToggle } from "@/components/shell/ThemeToggle";

export interface SettingsProps {
  settings: AppSettings;
  // Persist a full replacement settings row (optimistic; see useAppSettings).
  onUpdate: (next: AppSettings) => Promise<void>;
}

// Settings surface (F-803 + F-909 re-selection, AC-34/AC-35). Calm maintenance
// screen: the library folder is the focal control (its "Change folder" is the
// one primary action), everything else is a quiet row. Theme changes apply live
// through the same path as the titlebar toggle (one source of truth: onUpdate).
// Plain-language register, "set aside" for the quarantine root (FD-31); no
// engine jargon (design-system Sections 4-6).
export function Settings({ settings, onUpdate }: SettingsProps) {
  const s = STRINGS.settings;
  const [note, setNote] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const theme: Theme = isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;

  async function apply(next: AppSettings) {
    setNote(null);
    try {
      await onUpdate(next);
      setNote({ kind: "ok", text: "Saved." });
    } catch (e) {
      setNote({ kind: "err", text: e instanceof Error ? e.message : String(e) });
    }
  }

  async function pickInto(field: "library_root" | "set_aside_root" | "reports_root") {
    const path = await pickLibraryFolder();
    if (path === null) return; // cancelled
    await apply({ ...settings, [field]: path });
  }

  function resetToDefault(field: "set_aside_root" | "reports_root") {
    void apply({ ...settings, [field]: null });
  }

  return (
    <div className="max-w-[62ch]">
      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em]">{s.heading}</h1>
      <p className="mt-2 text-[14px] leading-relaxed text-ink-2">{s.intro}</p>

      <div className="mt-8 flex flex-col gap-7">
        {/* Library folder: the focal control, primary-styled action. */}
        <section className="rounded-md border border-border bg-surface p-5">
          <h2 className="text-[14px] font-semibold text-ink">{s.libraryLabel}</h2>
          <p className="mt-1 text-[12.5px] leading-relaxed text-ink-2">{s.libraryHelp}</p>
          <p className="mt-3 break-all font-mono text-[12.5px] text-ink-3">
            {settings.library_root ?? s.defaultLocation}
          </p>
          <button
            type="button"
            onClick={() => void pickInto("library_root")}
            className="mt-3 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
          >
            {settings.library_root ? s.libraryChange : s.libraryChoose}
          </button>
        </section>

        {/* Set-aside folder (quarantine_root; FD-31 plain-language name). */}
        <FolderRow
          label={s.setAsideLabel}
          help={s.setAsideHelp}
          value={settings.set_aside_root}
          defaultLabel={s.defaultLocation}
          onChange={() => void pickInto("set_aside_root")}
          onUseDefault={() => resetToDefault("set_aside_root")}
          changeLabel={s.change}
        />

        {/* Reports folder override. */}
        <FolderRow
          label={s.reportsLabel}
          help={s.reportsHelp}
          value={settings.reports_root}
          defaultLabel={s.defaultLocation}
          onChange={() => void pickInto("reports_root")}
          onUseDefault={() => resetToDefault("reports_root")}
          changeLabel={s.change}
        />

        {/* Appearance (theme) - live via onUpdate, one source of truth. */}
        <section className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-[14px] font-semibold text-ink">{s.themeLabel}</h2>
            <p className="mt-1 text-[12.5px] leading-relaxed text-ink-2">{s.themeHelp}</p>
          </div>
          <ThemeToggle theme={theme} onChange={(t) => void apply({ ...settings, theme: t })} />
        </section>

        {/* Snapshot retention (FD-20, AC-35). */}
        <RetentionRow
          label={s.retentionLabel}
          help={s.retentionHelp}
          value={settings.scan_retention_count}
          onCommit={(n) => void apply({ ...settings, scan_retention_count: n })}
        />
      </div>

      <p
        role="status"
        aria-live="polite"
        className={`mt-6 min-h-[1.25rem] text-[12.5px] ${
          note?.kind === "err" ? "text-danger" : "text-ink-3"
        }`}
      >
        {note?.text ?? ""}
      </p>
    </div>
  );
}

function FolderRow({
  label,
  help,
  value,
  defaultLabel,
  onChange,
  onUseDefault,
  changeLabel,
}: {
  label: string;
  help: string;
  value: string | null;
  defaultLabel: string;
  onChange: () => void;
  onUseDefault: () => void;
  changeLabel: string;
}) {
  return (
    <section>
      <h2 className="text-[14px] font-semibold text-ink">{label}</h2>
      <p className="mt-1 max-w-[52ch] text-[12.5px] leading-relaxed text-ink-2">{help}</p>
      <p className="mt-2 break-all font-mono text-[12.5px] text-ink-3">{value ?? defaultLabel}</p>
      <div className="mt-2 flex items-center gap-3">
        <button
          type="button"
          onClick={onChange}
          className="rounded border border-border-2 px-3 py-1.5 text-[12.5px] font-semibold text-ink-2 transition-colors hover:bg-surface-2 hover:text-ink"
        >
          {changeLabel}
        </button>
        {value !== null && (
          <button
            type="button"
            onClick={onUseDefault}
            className="text-[12.5px] text-link hover:underline"
          >
            Use default
          </button>
        )}
      </div>
    </section>
  );
}

function RetentionRow({
  label,
  help,
  value,
  onCommit,
}: {
  label: string;
  help: string;
  value: number;
  onCommit: (n: number) => void;
}) {
  const [draft, setDraft] = useState<string>(String(value));

  // Commit on blur (or Enter): clamp to a sane minimum of 1, ignore a blank or
  // non-numeric draft (reset it to the current value), and only persist when the
  // value actually changed, so leaving the field untouched writes nothing.
  function commit() {
    const parsed = Number.parseInt(draft, 10);
    if (!Number.isFinite(parsed) || parsed < 1) {
      setDraft(String(value));
      return;
    }
    if (parsed !== value) onCommit(parsed);
    setDraft(String(parsed));
  }

  return (
    <section className="flex items-start justify-between gap-4">
      <div>
        <h2 className="text-[14px] font-semibold text-ink">{label}</h2>
        <p className="mt-1 max-w-[52ch] text-[12.5px] leading-relaxed text-ink-2">{help}</p>
      </div>
      <input
        type="number"
        min={1}
        aria-label={label}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
        }}
        className="tabular-nums w-20 rounded border border-border-2 bg-surface px-2 py-1.5 text-[13px] text-ink"
      />
    </section>
  );
}
