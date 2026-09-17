import React from "react";
import "./WaterfallTimeline.css";
import { SmartTooltip } from "./SmartTooltip";

export interface TimingPhase {
  name: string;
  durationMs: number;
  color: string;
}

export interface WaterfallTimelineProps {
  totalMs: number;
  maxScaleMs?: number;
  phases?: TimingPhase[];
  className?: string;
}

/**
 * WaterfallTimeline — Multi-phase network timing breakdown bar (Bklit UI inspired).
 * Displays DNS, TLS, TTFB (Waiting) and Content Download segments with micro tooltip inspection.
 */
export const WaterfallTimeline: React.FC<WaterfallTimelineProps> = ({
  totalMs,
  maxScaleMs = 400,
  phases,
  className = "",
}) => {
  // If detailed phases are not provided, synthesize standard browser timing proportions
  const activePhases: TimingPhase[] = phases ?? [
    { name: "DNS Lookup", durationMs: Math.max(1, Math.round(totalMs * 0.08)), color: "#38BDF8" },
    { name: "Initial Connection (TCP/TLS)", durationMs: Math.max(2, Math.round(totalMs * 0.16)), color: "#F472B6" },
    { name: "Waiting for server (TTFB)", durationMs: Math.max(4, Math.round(totalMs * 0.52)), color: "#FBBF24" },
    { name: "Content Download", durationMs: Math.max(2, Math.round(totalMs * 0.24)), color: "#34D399" },
  ];

  const totalCalculatedMs = activePhases.reduce((acc, p) => acc + p.durationMs, 0) || totalMs || 1;
  const barWidthPct = Math.min(100, Math.max(4, (totalMs / maxScaleMs) * 100));

  const tooltipContent = (
    <div className="waterfall-tooltip">
      <div className="waterfall-tooltip__total">Total: {totalMs}ms</div>
      <div className="waterfall-tooltip__phases">
        {activePhases.map((phase) => (
          <div key={phase.name} className="waterfall-tooltip__row">
            <span className="waterfall-tooltip__dot" style={{ backgroundColor: phase.color }} />
            <span className="waterfall-tooltip__name">{phase.name}:</span>
            <span className="waterfall-tooltip__ms">{phase.durationMs}ms</span>
          </div>
        ))}
      </div>
    </div>
  );

  return (
    <SmartTooltip content={tooltipContent} position="top">
      <div className={`waterfall-timeline ${className}`}>
        <div className="waterfall-timeline__track" style={{ width: `${barWidthPct}%` }}>
          {activePhases.map((phase, idx) => {
            const widthPct = (phase.durationMs / totalCalculatedMs) * 100;
            return (
              <div
                key={idx}
                className="waterfall-timeline__segment"
                style={{
                  width: `${widthPct}%`,
                  backgroundColor: phase.color,
                }}
              />
            );
          })}
        </div>
        <span className="waterfall-timeline__label">{totalMs}ms</span>
      </div>
    </SmartTooltip>
  );
};
