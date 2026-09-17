import React from "react";
import "./ShimmerButton.css";

export interface ShimmerButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  children: React.ReactNode;
  variant?: "primary" | "secondary" | "glass" | "danger";
  size?: "sm" | "md" | "lg";
  shimmerColor?: string;
  icon?: React.ReactNode;
  className?: string;
}

/**
 * ShimmerButton — High-polish action button with sweeping liquid light sheen (Skiper UI inspired).
 * Adheres strictly to Emil Kowalski's tactile micro-interaction principles:
 * - Instant active scale(0.97) feedback
 * - Non-jarring transform-only transitions
 */
export const ShimmerButton: React.FC<ShimmerButtonProps> = ({
  children,
  variant = "primary",
  size = "md",
  shimmerColor = "rgba(255, 255, 255, 0.25)",
  icon,
  className = "",
  disabled,
  style,
  ...rest
}) => {
  return (
    <button
      className={`shimmer-btn shimmer-btn--${variant} shimmer-btn--${size} ${className}`}
      disabled={disabled}
      style={
        {
          "--shimmer-color": shimmerColor,
          ...style,
        } as React.CSSProperties
      }
      {...rest}
    >
      <span className="shimmer-btn__sweep" aria-hidden="true" />
      {icon && <span className="shimmer-btn__icon" aria-hidden="true">{icon}</span>}
      <span className="shimmer-btn__label">{children}</span>
    </button>
  );
};
