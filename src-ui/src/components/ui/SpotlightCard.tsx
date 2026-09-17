import React, { useRef, useState, useCallback } from "react";
import "./SpotlightCard.css";

export interface SpotlightCardProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
  spotlightColor?: string;
  borderColor?: string;
  className?: string;
  withBeam?: boolean;
}

/**
 * SpotlightCard — Interactive card with mouse-following radial glow (Skiper / Unlumen inspired).
 * Traces cursor position across the card surface to generate dynamic liquid-glass highlights.
 */
export const SpotlightCard: React.FC<SpotlightCardProps> = ({
  children,
  spotlightColor = "rgba(249, 219, 189, 0.12)",
  borderColor = "rgba(218, 98, 125, 0.4)",
  className = "",
  withBeam = false,
  style,
  ...rest
}) => {
  const cardRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<{ x: number; y: number }>({ x: -1000, y: -1000 });
  const [isHovered, setIsHovered] = useState(false);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (!cardRef.current) return;
    const rect = cardRef.current.getBoundingClientRect();
    setCoords({
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    });
  }, []);

  const handleMouseEnter = useCallback(() => setIsHovered(true), []);
  const handleMouseLeave = useCallback(() => {
    setIsHovered(false);
    setCoords({ x: -1000, y: -1000 });
  }, []);

  return (
    <div
      ref={cardRef}
      className={`spotlight-card ${isHovered ? "spotlight-card--hovered" : ""} ${className}`}
      onMouseMove={handleMouseMove}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      style={
        {
          "--spotlight-x": `${coords.x}px`,
          "--spotlight-y": `${coords.y}px`,
          "--spotlight-color": spotlightColor,
          "--spotlight-border": borderColor,
          ...style,
        } as React.CSSProperties
      }
      {...rest}
    >
      <div className="spotlight-card__glow" aria-hidden="true" />
      <div className="spotlight-card__border" aria-hidden="true" />
      <div className="spotlight-card__content">{children}</div>
    </div>
  );
};
