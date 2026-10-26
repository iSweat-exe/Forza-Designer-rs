interface Props {
  value: number;
  max: number;
  label?: string;
  /** Unknown total: render an animated indeterminate bar. */
  indeterminate?: boolean;
}

export function ProgressBar({ value, max, label, indeterminate }: Props) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    <div
      className={indeterminate ? 'progress indeterminate' : 'progress'}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={indeterminate ? undefined : value}
      aria-label={label}
    >
      <div className="progress-fill" style={indeterminate ? undefined : { width: `${pct}%` }} />
    </div>
  );
}
