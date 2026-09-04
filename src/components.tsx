import type { ButtonHTMLAttributes, PropsWithChildren, ReactNode } from "react";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "danger" | "ghost";
  busy?: boolean;
  icon?: ReactNode;
};

export function Button({
  variant = "secondary",
  busy = false,
  icon,
  children,
  className = "",
  disabled,
  ...props
}: ButtonProps) {
  return (
    <button
      type="button"
      className={`button button--${variant} ${className}`}
      disabled={disabled || busy}
      aria-busy={busy}
      {...props}
    >
      <span className="button__slot" aria-hidden="true">
        {busy ? <span className="spinner" /> : icon}
      </span>
      <span>{children}</span>
    </button>
  );
}

export function Panel({ children, className = "" }: PropsWithChildren<{ className?: string }>) {
  return <section className={`panel ${className}`}>{children}</section>;
}

export function StatusDot({ tone }: { tone: "idle" | "busy" | "success" | "error" }) {
  return <span className={`status-dot status-dot--${tone}`} aria-hidden="true" />;
}
