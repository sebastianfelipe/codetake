// Small, unstyled-by-default form controls shared across the settings panel.

import type { ReactNode } from "react";

export function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="section">
      <header className="section-header">
        <h2>{title}</h2>
        {aside}
      </header>
      <div className="section-body">{children}</div>
    </section>
  );
}

export function Toggle({
  label,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className="toggle">
      <input
        type="checkbox"
        role="switch"
        aria-checked={checked}
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-track" aria-hidden="true" />
      <span>{label}</span>
    </label>
  );
}

export interface Option<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

export function Select<T extends string>({
  label,
  value,
  options,
  onChange,
  disabled,
  placeholder,
}: {
  label: string;
  value: T | null;
  options: Option<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
  placeholder?: string;
}) {
  return (
    <select
      className="select"
      aria-label={label}
      value={value ?? ""}
      disabled={disabled || options.length === 0}
      onChange={(e) => onChange(e.target.value as T)}
    >
      {value === null && <option value="">{placeholder ?? "Select…"}</option>}
      {options.map((option) => (
        <option key={option.value} value={option.value} disabled={option.disabled}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

export function Segmented<T extends string | number>({
  label,
  value,
  options,
  onChange,
  disabled,
}: {
  label: string;
  value: T;
  options: { value: T; label: ReactNode; title?: string; disabled?: boolean }[];
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <fieldset className="segmented" aria-label={label}>
      {options.map((option) => (
        <button
          key={String(option.value)}
          type="button"
          aria-pressed={option.value === value}
          title={option.title}
          className={option.value === value ? "active" : undefined}
          disabled={disabled || option.disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </fieldset>
  );
}

export function Slider({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  return (
    <input
      className="slider"
      type="range"
      aria-label={label}
      min={0}
      max={100}
      step={1}
      value={Math.round(value * 100)}
      disabled={disabled}
      onChange={(e) => onChange(Number(e.target.value) / 100)}
    />
  );
}

export function LevelMeter({ level, label }: { level: number; label: string }) {
  // Perceptual scale: map -60..0 dBFS onto the bar.
  const db = level > 0 ? 20 * Math.log10(level) : -60;
  const fill = Math.min(1, Math.max(0, (db + 60) / 60));
  return (
    <div className="meter" title={label} aria-hidden="true">
      <div className="meter-fill" style={{ transform: `scaleX(${fill})` }} />
    </div>
  );
}

export function Hint({ children, tone }: { children: ReactNode; tone?: "warning" | "error" }) {
  return <p className={`hint ${tone ?? ""}`}>{children}</p>;
}
