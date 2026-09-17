import React, { useState, useRef, useEffect, useCallback } from "react";
import "./SmartTooltip.css";

export interface SmartTooltipProps {
  content: React.ReactNode;
  children: React.ReactElement;
  position?: "top" | "bottom" | "left" | "right";
  delayMs?: number;
  className?: string;
}

// Global state tracking whether any tooltip is currently open, to enable instant transition for subsequent hovers
let globalTooltipActiveTime = 0;
const TOOLTIP_WARM_WINDOW_MS = 450;

export const SmartTooltip: React.FC<SmartTooltipProps> = ({
  content,
  children,
  position = "top",
  delayMs = 280,
  className = "",
}) => {
  const [isVisible, setIsVisible] = useState(false);
  const [isInstant, setIsInstant] = useState(false);
  const timerRef = useRef<number | null>(null);

  const handleMouseEnter = useCallback(() => {
    const now = Date.now();
    const isWarm = now - globalTooltipActiveTime < TOOLTIP_WARM_WINDOW_MS;

    if (isWarm) {
      // Skip delay and transition animation on subsequent adjacent hovers
      setIsInstant(true);
      setIsVisible(true);
      globalTooltipActiveTime = now;
    } else {
      setIsInstant(false);
      timerRef.current = window.setTimeout(() => {
        setIsVisible(true);
        globalTooltipActiveTime = Date.now();
      }, delayMs);
    }
  }, [delayMs]);

  const handleMouseLeave = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (isVisible) {
      globalTooltipActiveTime = Date.now();
    }
    setIsVisible(false);
  }, [isVisible]);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  // Determine origin for Emil Kowalski's origin-aware animations
  const originMap: Record<string, string> = {
    top: "bottom center",
    bottom: "top center",
    left: "center right",
    right: "center left",
  };

  return (
    <div
      className="smart-tooltip-wrap"
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onFocus={handleMouseEnter}
      onBlur={handleMouseLeave}
    >
      {children}
      {isVisible && (
        <div
          role="tooltip"
          className={`smart-tooltip smart-tooltip--${position} ${isInstant ? "smart-tooltip--instant" : ""} ${className}`}
          style={{ "--tooltip-origin": originMap[position] } as React.CSSProperties}
        >
          {content}
        </div>
      )}
    </div>
  );
};
