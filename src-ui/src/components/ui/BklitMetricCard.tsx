import React from "react";
import "./BklitMetricCard.css";
import { SparklineChart } from "./SparklineChart";
import { SpotlightCard } from "./SpotlightCard";

export interface BklitMetricCardProps {
  title: string;
  value: string | number;
  unit?: string;
  icon?: React.ReactNode;
  status?: "good" | "warn" | "crit" | "neutral";
  badgeText?: string;
  sparklineData?: number[];
  sparklineColor?: string;
  delta?: string;
  deltaPositive?: boolean;
  className?: string;
}

/**
 * BklitMetricCard — High-density technical telemetry card (Bklit UI inspired).
 * Features live pulsing status, monospace metrics, delta chips, and integrated micro-sparklines.
 */
export const BklitMetricCard: React.FC<BklitMetricCardProps> = ({
  title,
  value,
  unit = "",
  icon,
  status = "neutral",
  badgeText,
  sparklineData,
  sparklineColor = "#DA627D",
  delta,
  deltaPositive = true,
  className = "",
}) => {
  return (
    <SpotlightCard className={`bklit-card bklit-card--${status} ${className}`}>
      <div className="bklit-card__header">
        <div className="bklit-card__title-wrap">
          {icon && <span className="bklit-card__icon" aria-hidden="true">{icon}</span>}
          <span className="bklit-card__title">{title}</span>
        </div>
        <div className="bklit-card__status-wrap">
          <span className={`bklit-pulse bklit-pulse--${status}`} aria-hidden="true" />
          {badgeText && <span className={`bklit-badge bklit-badge--${status}`}>{badgeText}</span>}
        </div>
      </div>

      <div className="bklit-card__body">
        <div className="bklit-card__metric">
          <span className="bklit-card__val">{value}</span>
          {unit && <span className="bklit-card__unit">{unit}</span>}
        </div>

        {delta && (
          <span className={`bklit-card__delta ${deltaPositive ? "bklit-card__delta--up" : "bklit-card__delta--down"}`}>
            {deltaPositive ? "↑" : "↓"} {delta}
          </span>
        )}
      </div>

      {sparklineData && sparklineData.length > 0 && (
        <div className="bklit-card__sparkline">
          <SparklineChart
            data={sparklineData}
            height={38}
            color={sparklineColor}
            unit={unit}
          />
        </div>
      )}
    </SpotlightCard>
  );
};
