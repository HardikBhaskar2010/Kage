import React, { useId, useState, useCallback, useMemo } from "react";
import "./SparklineChart.css";

export interface SparklineChartProps {
  data: number[];
  width?: number | string;
  height?: number;
  color?: string;
  gradientFrom?: string;
  gradientTo?: string;
  unit?: string;
  min?: number;
  max?: number;
  className?: string;
}

/**
 * SparklineChart — Composable SVG telemetry sparkline (Bklit UI inspired).
 * Features smooth polyline/spline curve, gradient area fill, and interactive hover crosshair value readout.
 */
export const SparklineChart: React.FC<SparklineChartProps> = ({
  data,
  width = "100%",
  height = 48,
  color = "#DA627D",
  gradientFrom = "rgba(218, 98, 125, 0.35)",
  gradientTo = "rgba(69, 9, 32, 0.02)",
  unit = "",
  min: customMin,
  max: customMax,
  className = "",
}) => {
  const gradientId = useId();
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  const points = useMemo(() => {
    if (!data || data.length === 0) return [];
    const minVal = customMin ?? Math.min(...data);
    const maxVal = customMax ?? Math.max(...data);
    const range = maxVal - minVal || 1;

    return data.map((val, idx) => {
      const x = (idx / (data.length - 1 || 1)) * 100;
      const normalizedY = (val - minVal) / range;
      // Invert Y coordinate for SVG viewBox (0 at top, 100 at bottom)
      const y = 92 - normalizedY * 84;
      return { x, y, val };
    });
  }, [data, customMin, customMax]);

  const pathD = useMemo(() => {
    if (points.length === 0) return "";
    return points.reduce((acc, p, idx) => {
      return idx === 0 ? `M ${p.x.toFixed(2)} ${p.y.toFixed(2)}` : `${acc} L ${p.x.toFixed(2)} ${p.y.toFixed(2)}`;
    }, "");
  }, [points]);

  const areaD = useMemo(() => {
    if (points.length === 0) return "";
    return `${pathD} L 100 100 L 0 100 Z`;
  }, [pathD, points]);

  const handleMouseMove = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const idx = Math.round(ratio * (data.length - 1));
    setHoverIndex(idx);
  }, [data.length]);

  const handleMouseLeave = useCallback(() => {
    setHoverIndex(null);
  }, []);

  const hoveredPoint = hoverIndex !== null && points[hoverIndex] ? points[hoverIndex] : null;

  return (
    <div className={`sparkline-wrap ${className}`} style={{ width, height }}>
      <svg
        className="sparkline-svg"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
      >
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={gradientFrom} />
            <stop offset="100%" stopColor={gradientTo} />
          </linearGradient>
        </defs>

        {/* Area fill under curve */}
        {areaD && <path d={areaD} fill={`url(#${gradientId})`} />}

        {/* Line curve */}
        {pathD && (
          <path
            d={pathD}
            fill="none"
            stroke={color}
            strokeWidth="2.5"
            vectorEffect="non-scaling-stroke"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        )}

        {/* Hover Crosshair */}
        {hoveredPoint && (
          <>
            <line
              x1={hoveredPoint.x}
              y1="0"
              x2={hoveredPoint.x}
              y2="100"
              stroke="rgba(249, 219, 189, 0.4)"
              strokeDasharray="2 2"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <circle
              cx={hoveredPoint.x}
              cy={hoveredPoint.y}
              r="3.5"
              fill="#FFFFFF"
              stroke={color}
              strokeWidth="2"
              vectorEffect="non-scaling-stroke"
            />
          </>
        )}
      </svg>

      {/* Value pill on hover */}
      {hoveredPoint && (
        <div
          className="sparkline-pill"
          style={{ left: `${hoveredPoint.x}%` }}
        >
          {typeof hoveredPoint.val === "number" ? hoveredPoint.val.toFixed(1) : hoveredPoint.val} {unit}
        </div>
      )}
    </div>
  );
};
