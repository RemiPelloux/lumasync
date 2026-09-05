interface RangeControlProps {
  label: string;
  value: number;
  min: number;
  max: number;
  suffix: string;
  disabled?: boolean;
  onChange: (value: number) => void;
}

export function RangeControl({ label, value, min, max, suffix, disabled, onChange }: RangeControlProps) {
  const id = `range-${label.toLowerCase().replaceAll(" ", "-")}`;
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <div className="range-control">
      <div className="range-control__label"><label htmlFor={id}>{label}</label><output htmlFor={id}>{value}{suffix}</output></div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        value={value}
        disabled={disabled}
        style={{ "--range-fill": `${fill}%` } as React.CSSProperties}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
    </div>
  );
}
