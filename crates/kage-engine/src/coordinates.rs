//! Strongly-typed coordinate systems for KAGE's multi-plane architecture.
//!
//! Enforces exact mathematical transformations across the 5 coordinate spaces
//! to guarantee sub-pixel precision for Micro Inspect and mouse hit-testing.
//!
//! # Pipeline
//! ```text
//! TauriLogicalPoint
//!       │ (DpiContext: scale_x, scale_y, per-monitor awareness)
//!       ▼
//! Win32ClientPoint
//!       │ (Top-level window client-to-screen translation)
//!       ▼
//! PhysicalPixelPoint
//!       │ (Subtract CEF child HWND viewport offset)
//!       ▼
//! CefViewPoint
//!       │ (Chromium page zoom factor)
//!       ▼
//! CssPoint
//! ```

/// Windows DPI Awareness context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiAwareness {
    Unaware,
    SystemAware,
    PerMonitorV2,
}

/// Explicit DPI context capturing display and monitor configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiContext {
    pub scale_x: f64,
    pub scale_y: f64,
    pub monitor_handle: Option<isize>,
    pub dpi_awareness: DpiAwareness,
}

impl DpiContext {
    /// Standard 100% DPI (96 DPI) baseline context.
    pub fn standard() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            monitor_handle: None,
            dpi_awareness: DpiAwareness::PerMonitorV2,
        }
    }

    /// Create with explicit scale factors.
    pub fn with_scale(scale_x: f64, scale_y: f64) -> Self {
        Self {
            scale_x,
            scale_y,
            monitor_handle: None,
            dpi_awareness: DpiAwareness::PerMonitorV2,
        }
    }
}

impl Default for DpiContext {
    fn default() -> Self {
        Self::standard()
    }
}

/// Geometry of the host window and child viewports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub window_origin_x: i32,
    pub window_origin_y: i32,
    pub cef_offset_x: i32,
    pub cef_offset_y: i32,
}

/// [1] Logical layout coordinates in Tauri / React shell space (e.g. CSS pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TauriLogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl TauriLogicalPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Convert to Win32 client coordinates using explicit DpiContext.
    pub fn to_win32_client(&self, dpi: &DpiContext) -> Win32ClientPoint {
        Win32ClientPoint {
            x: (self.x * dpi.scale_x).round() as i32,
            y: (self.y * dpi.scale_y).round() as i32,
        }
    }
}

/// [2] Win32 client window coordinates relative to top-level HWND client rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Win32ClientPoint {
    pub x: i32,
    pub y: i32,
}

impl Win32ClientPoint {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Convert to physical screen pixels given top-level window origin.
    pub fn to_physical_screen(&self, window_origin_x: i32, window_origin_y: i32) -> PhysicalPixelPoint {
        PhysicalPixelPoint {
            x: self.x + window_origin_x,
            y: self.y + window_origin_y,
        }
    }

    /// Convert directly to CEF child viewport coordinates given the CEF child HWND offset.
    pub fn to_cef_view(&self, cef_offset_x: i32, cef_offset_y: i32) -> CefViewPoint {
        CefViewPoint {
            x: self.x - cef_offset_x,
            y: self.y - cef_offset_y,
        }
    }
}

/// [3] Physical screen pixels accounting for native monitor resolution and scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPixelPoint {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPixelPoint {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// [4] Coordinates relative to the CEF child HWND viewport rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CefViewPoint {
    pub x: i32,
    pub y: i32,
}

/// Context for browser viewport transformations into DOM/CSS coordinates.
/// Accounts for browser zoom, device scale factor, and document scroll offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserViewportTransform {
    pub page_zoom: f64,
    pub device_scale_factor: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

impl BrowserViewportTransform {
    pub fn new(page_zoom: f64, device_scale_factor: f64, scroll_x: f64, scroll_y: f64) -> Self {
        Self {
            page_zoom: if page_zoom <= 0.0 { 1.0 } else { page_zoom },
            device_scale_factor: if device_scale_factor <= 0.0 { 1.0 } else { device_scale_factor },
            scroll_x,
            scroll_y,
        }
    }
}

impl Default for BrowserViewportTransform {
    fn default() -> Self {
        Self {
            page_zoom: 1.0,
            device_scale_factor: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

impl CefViewPoint {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Convert to Blink CSS DOM coordinates inside the Chromium document.
    pub fn to_css_point(&self, transform: &BrowserViewportTransform) -> CssPoint {
        let zoom_effective = transform.page_zoom * transform.device_scale_factor;
        CssPoint {
            x: (self.x as f64) / zoom_effective + transform.scroll_x,
            y: (self.y as f64) / zoom_effective + transform.scroll_y,
        }
    }
}

/// [5] Blink CSS DOM coordinates inside the Chromium rendering document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssPoint {
    pub x: f64,
    pub y: f64,
}

impl CssPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Transform an entire point end-to-end taking `(point, dpi_context, window_geometry, viewport_transform)`.
pub fn transform_logical_to_css(
    point: TauriLogicalPoint,
    dpi: &DpiContext,
    geometry: &WindowGeometry,
    viewport_transform: &BrowserViewportTransform,
) -> CssPoint {
    let win32 = point.to_win32_client(dpi);
    let cef = win32.to_cef_view(geometry.cef_offset_x, geometry.cef_offset_y);
    cef.to_css_point(viewport_transform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinate_pipeline_across_dpi_scales() {
        let scales = [1.0, 1.25, 1.5, 2.0];
        let logical = TauriLogicalPoint::new(400.0, 300.0);

        for &scale in &scales {
            let dpi = DpiContext::with_scale(scale, scale);
            let win32 = logical.to_win32_client(&dpi);
            assert_eq!(win32.x, (400.0 * scale).round() as i32);
            assert_eq!(win32.y, (300.0 * scale).round() as i32);

            let geometry = WindowGeometry {
                window_origin_x: 100,
                window_origin_y: 100,
                cef_offset_x: (320.0 * scale).round() as i32,
                cef_offset_y: (88.0 * scale).round() as i32,
            };

            let viewport = BrowserViewportTransform::default();
            let css = transform_logical_to_css(logical, &dpi, &geometry, &viewport);
            let expected_cef_x = (400.0 * scale).round() as i32 - (320.0 * scale).round() as i32;
            let expected_cef_y = (300.0 * scale).round() as i32 - (88.0 * scale).round() as i32;
            assert_eq!(css.x, expected_cef_x as f64);
            assert_eq!(css.y, expected_cef_y as f64);
        }
    }
}
