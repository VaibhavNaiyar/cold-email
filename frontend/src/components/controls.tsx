"use client";

// Toolbar controls shared by every view, so each looks and behaves the same.

const FIELD = "rounded-md border border-line bg-paper px-2 py-2 text-sm";

export function SearchBox({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="min-w-0 flex-1 basis-full sm:basis-56">
      <span className="sr-only">{label}</span>
      <input
        type="search"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className={`w-full px-3 placeholder:text-muted ${FIELD}`}
      />
    </label>
  );
}

export type Option<T extends string | number> = { value: T; label: string };

export function Choice<T extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly Option<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <label>
      <span className="sr-only">{label}</span>
      <select
        value={String(value)}
        onChange={(event) => {
          const picked = options.find((option) => String(option.value) === event.target.value);
          if (picked) onChange(picked.value);
        }}
        className={FIELD}
      >
        {options.map((option) => (
          <option key={String(option.value)} value={String(option.value)}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="size-4 accent-[var(--navy)]"
      />
      {label}
    </label>
  );
}

export function Toolbar({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-line bg-surface px-4 py-3 lg:px-6">
      {children}
    </div>
  );
}

/** "Showing 12 of 40", announced to screen readers as filters change. */
export function Count({ shown, total }: { shown: number; total: number }) {
  return (
    <p className="px-4 py-2 text-xs text-muted lg:px-6" aria-live="polite">
      Showing {shown} of {total}
    </p>
  );
}

export const DATE_FILTERS: readonly Option<number>[] = [
  { value: 0, label: "Any date" },
  { value: 1, label: "Last 24 hours" },
  { value: 7, label: "Last 7 days" },
  { value: 10, label: "Last 10 days" },
  { value: 30, label: "Last 30 days" },
  { value: 90, label: "Last 3 months" },
];
