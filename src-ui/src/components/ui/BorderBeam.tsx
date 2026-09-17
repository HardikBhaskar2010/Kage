import React from "react";
import "./BorderBeam.css";

export interface BorderBeamProps {
  size?: number;
  duration?: number;
  delay?: number;
  colorFrom?: string;
  colorTo?: string;
  className?: string;
  style?: React.CSSProperties;
}

/**
 * BorderBeam — Modern animated glowing border sweep (Skiper UI inspired).
 * Traces the border perimeter of any relative container with a vibrant gradient beam.
 */
export const BorderBeam: React.FC<BorderBeamProps> = ({
  size = 200,
  duration = 12,
  delay = 0,
  colorFrom = "#F9DBBD",
  colorTo = "#DA627D",
  className = "",
  style,
}) => {
  return (
    <div
      aria-hidden="true"
      className={`border-beam ${className}`}
      style={
        {
          "--beam-size": `${size}px`,
          "--beam-duration": `${duration}s`,
          "--beam-delay": `${delay}s`,
          "--beam-color-from": colorFrom,
          "--beam-color-to": colorTo,
          ...style,
        } as React.CSSProperties
      }
    />
  );
};
