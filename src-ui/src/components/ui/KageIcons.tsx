import React from "react";

export interface KageIconProps extends React.SVGProps<SVGSVGElement> {
  size?: number;
  strokeWidth?: number;
  color?: string;
  className?: string;
}

/**
 * Canonical KAGE Iconography System (Linear + Filled)
 * Sourced directly from public/Design System.png and public/Mockup_New Tab Screen.png
 */

// 1. HOME — Rounded house silhouette with chimney, rounded doorway & subtle playful roof accent dots
export const KageIconHome: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Subtle twin roof accent dots from mockup */}
    <circle cx="5.5" cy="4.5" r="1" fill={color} stroke="none" opacity="0.6" />
    <circle cx="18.5" cy="4.5" r="1" fill={color} stroke="none" opacity="0.6" />
    {/* Chimney */}
    <path d="M18 8.5V4h-2.5v2.2" />
    {/* House body & roof */}
    <path d="M3.5 10.5L12 3.5l8.5 7V19.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-9z" />
    {/* Rounded Door */}
    <path d="M9.5 21.5v-6a2.5 2.5 0 0 1 5 0v6" />
  </svg>
);

// 2. AI — Radiant 4-pointed sparkle star with luminous center and orbital satellites
export const KageIconAI: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Radiant 4-pointed star */}
    <path
      d="M12 2C12 7.2 7.2 12 2 12C7.2 12 12 16.8 12 22C12 16.8 16.8 12 22 12C16.8 12 12 7.2 12 2Z"
      fill={color}
      fillOpacity="0.18"
    />
    {/* Subtle 4 diagonal micro-satellite sparkles */}
    <circle cx="5" cy="5" r="0.9" fill={color} stroke="none" />
    <circle cx="19" cy="5" r="0.9" fill={color} stroke="none" />
    <circle cx="5" cy="19" r="0.9" fill={color} stroke="none" />
    <circle cx="19" cy="19" r="0.9" fill={color} stroke="none" />
  </svg>
);

// 3. INSPECT — Round inspection lens with 45-degree handle and focal center point
export const KageIconInspect: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <circle cx="11" cy="11" r="7.5" />
    <line x1="16.5" y1="16.5" x2="21.5" y2="21.5" />
    <circle cx="11" cy="11" r="1.5" fill={color} stroke="none" />
  </svg>
);

// 4. DOM — Isometric 3D wireframe cube with top, left, and right facets
export const KageIconDOM: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Outer hexagonal contour */}
    <path d="M12 2.5L20.5 7.4V16.6L12 21.5L3.5 16.6V7.4L12 2.5Z" />
    {/* Inner isometric Y-seams */}
    <line x1="12" y1="2.5" x2="12" y2="12" />
    <line x1="12" y1="12" x2="20.5" y2="7.4" />
    <line x1="12" y1="12" x2="3.5" y2="7.4" />
    <line x1="12" y1="12" x2="12" y2="21.5" />
  </svg>
);

// 5. NETWORK — Interlocking code pulse & bidirectional flow arrows
export const KageIconNetwork: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Left arrow / code chevron */}
    <path d="M8 7L4 12L8 17" />
    {/* Right arrow / code chevron */}
    <path d="M16 7L20 12L16 17" />
    {/* Center pulse nodes */}
    <line x1="12" y1="5" x2="12" y2="9" />
    <circle cx="12" cy="12" r="1.5" fill={color} />
    <line x1="12" y1="15" x2="12" y2="19" />
  </svg>
);

// 6. PERFORMANCE — 3-bar histogram + upward trending sparkline with vertex node
export const KageIconPerformance: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Upward trendline */}
    <path d="M4 16.5L9.5 11L14.5 14L20 7" />
    {/* Circle node at peak */}
    <circle cx="20" cy="7" r="2" fill={color} />
    {/* Bar stems below */}
    <line x1="6" y1="20" x2="6" y2="17.5" opacity="0.6" />
    <line x1="12" y1="20" x2="12" y2="14.5" opacity="0.6" />
    <line x1="18" y1="20" x2="18" y2="11.5" opacity="0.6" />
  </svg>
);

// 7. CONSOLE / TESTS — Science Erlenmeyer Flask with liquid meniscus
export const KageIconConsole: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    {/* Flask Rim */}
    <path d="M10 2.5H14" />
    {/* Neck & Body */}
    <path d="M10 2.5V7.5L4.5 18.5A2 2 0 0 0 6.2 21.5H17.8A2 2 0 0 0 19.5 18.5L14 7.5V2.5" />
    {/* Liquid level */}
    <path d="M7.8 15.5H16.2" opacity="0.7" />
    <circle cx="10" cy="18.5" r="0.8" fill={color} stroke="none" />
    <circle cx="14" cy="17.5" r="1.1" fill={color} stroke="none" />
  </svg>
);

// 8. STORAGE — Dual-tier rounded cylinder disk stack
export const KageIconStorage: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <ellipse cx="12" cy="5.5" rx="8" ry="3" />
    <path d="M4 5.5V12C4 13.66 7.58 15 12 15C16.42 15 20 13.66 20 12V5.5" />
    <path d="M4 12V18.5C4 20.16 7.58 21.5 12 21.5C16.42 21.5 20 20.16 20 18.5V12" />
    {/* Slot indicator */}
    <line x1="10" y1="12" x2="14" y2="12" opacity="0.7" />
  </svg>
);

// 9. SECURITY — Shield with centered verification checkmark
export const KageIconSecurity: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <path d="M12 2.5L4.5 5.5V11.5C4.5 16.8 7.7 21.6 12 22.5C16.3 21.6 19.5 16.8 19.5 11.5V5.5L12 2.5Z" />
    <polyline points="9 12 11.2 14.2 15.5 9.8" />
  </svg>
);

// 10. EXTENSIONS — 4-petal puzzle piece
export const KageIconExtensions: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

// 11. SETTINGS — Precision 8-tooth mechanical gear with circular center
export const KageIconSettings: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <circle cx="12" cy="12" r="3.2" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

// 12. RETICLE SCANNER — Circular scanner for AI Analyze Page card
export const KageIconScanner: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <path d="M4 8V5a1 1 0 0 1 1-1h3" />
    <path d="M16 4h3a1 1 0 0 1 1 1v3" />
    <path d="M20 16v3a1 1 0 0 1-1 1h-3" />
    <path d="M8 20H5a1 1 0 0 1-1-1v-3" />
    <circle cx="12" cy="12" r="4" />
    <line x1="7" y1="12" x2="17" y2="12" opacity="0.6" />
  </svg>
);

// 13. CODE FOCUS — Element explain bracket target
export const KageIconCodeFocus: React.FC<KageIconProps> = ({
  size = 18,
  strokeWidth = 1.8,
  color = "currentColor",
  className,
  ...props
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke={color}
    strokeWidth={strokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <path d="M7 8L3 12L7 16" />
    <path d="M17 8L21 12L17 16" />
    <circle cx="12" cy="12" r="2.5" fill={color} fillOpacity="0.25" />
  </svg>
);
