//! Native Win32 Surface Composition Coordinator.
//!
//! Enforces Gate CEF-06 (Physical Bounds Composition):
//! - Tauri WebView2 chrome is physically bounded to the chrome region.
//! - CEF child HWND physically owns the content region.
//! - Strict mathematical assertion of ZERO overlap between native surfaces.
//! - Synchronous resize updates both native surfaces.
//! - WebView2 cannot occlude CEF content; CEF cannot occlude interactive WebView2 chrome.

use crate::coordinates::DpiContext;
use crate::errors::EngineError;
use std::sync::RwLock;

/// Physical pixel rectangle for native Win32 child surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ViewportRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    /// Check whether two rectangles intersect.
    pub fn intersects(&self, other: &ViewportRect) -> bool {
        let no_overlap = self.x + self.width <= other.x
            || other.x + other.width <= self.x
            || self.y + self.height <= other.y
            || other.y + other.height <= self.y;
        !no_overlap
    }
}

/// Layout configuration for the KAGE window chrome bands.
#[derive(Debug, Clone, Copy)]
pub struct ChromeLayoutConfig {
    /// Height of the top omnibox and tab strip in logical CSS pixels (default: 88.0).
    pub top_bar_height: f64,
    /// Width of the collapsible sidebar in logical CSS pixels (default: 320.0).
    pub sidebar_width: f64,
    /// Whether the sidebar is currently expanded.
    pub sidebar_visible: bool,
}

impl Default for ChromeLayoutConfig {
    fn default() -> Self {
        Self {
            top_bar_height: 88.0,
            sidebar_width: 320.0,
            sidebar_visible: true,
        }
    }
}

/// The dual-surface physical layout defining exact bounds for WebView2 and CEF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DualSurfaceLayout {
    /// Physical rect for top chrome (tabs + omnibox).
    pub top_chrome_rect: ViewportRect,
    /// Physical rect for sidebar chrome (optional).
    pub sidebar_chrome_rect: Option<ViewportRect>,
    /// Physical rect for the native CEF content browser surface.
    pub cef_content_rect: ViewportRect,
}

impl DualSurfaceLayout {
    /// Validates that CEF content rect does NOT overlap with any WebView2 chrome rect.
    pub fn validate_no_overlap(&self) -> Result<(), EngineError> {
        if self.cef_content_rect.intersects(&self.top_chrome_rect) {
            return Err(EngineError::Surface(format!(
                "Physical collision: CEF rect {:?} intersects Top Chrome rect {:?}",
                self.cef_content_rect, self.top_chrome_rect
            )));
        }
        if let Some(ref sidebar) = self.sidebar_chrome_rect {
            if self.cef_content_rect.intersects(sidebar) {
                return Err(EngineError::Surface(format!(
                    "Physical collision: CEF rect {:?} intersects Sidebar rect {:?}",
                    self.cef_content_rect, sidebar
                )));
            }
        }
        Ok(())
    }
}

/// Coordinator that calculates physical Win32 bounds for non-overlapping composition.
///
/// Thread-safe: `update_layout` may be called from the Tauri async IPC thread;
/// `last_layout()` may be read from any thread.
pub struct NativeSurfaceManager {
    config: ChromeLayoutConfig,
    /// Most recently computed physical layout, cached for read-only access.
    last_layout: RwLock<Option<DualSurfaceLayout>>,
}

impl NativeSurfaceManager {
    pub fn new(config: ChromeLayoutConfig) -> Self {
        Self {
            config,
            last_layout: RwLock::new(None),
        }
    }

    /// Compute the dual physical pixel layout for WebView2 chrome and CEF content.
    pub fn compute_layout(
        &self,
        total_window_width: i32,
        total_window_height: i32,
        dpi: &DpiContext,
    ) -> Result<DualSurfaceLayout, EngineError> {
        let top_height = (self.config.top_bar_height * dpi.scale_y).round() as i32;
        let sidebar_width = if self.config.sidebar_visible {
            (self.config.sidebar_width * dpi.scale_x).round() as i32
        } else {
            0
        };

        let top_chrome_rect = ViewportRect::new(0, 0, total_window_width, top_height);

        let sidebar_chrome_rect = if sidebar_width > 0 {
            Some(ViewportRect::new(
                0,
                top_height,
                sidebar_width,
                (total_window_height - top_height).max(0),
            ))
        } else {
            None
        };

        let cef_x = sidebar_width;
        let cef_y = top_height;
        let cef_width = (total_window_width - cef_x).max(0);
        let cef_height = (total_window_height - cef_y).max(0);

        let cef_content_rect = ViewportRect::new(cef_x, cef_y, cef_width, cef_height);

        let layout = DualSurfaceLayout {
            top_chrome_rect,
            sidebar_chrome_rect,
            cef_content_rect,
        };

        layout.validate_no_overlap()?;
        Ok(layout)
    }

    /// Helper computing just the CEF viewport rectangle.
    pub fn compute_cef_viewport(
        &self,
        total_window_width: i32,
        total_window_height: i32,
        scale_factor: f64,
    ) -> ViewportRect {
        let dpi = DpiContext::with_scale(scale_factor, scale_factor);
        self.compute_layout(total_window_width, total_window_height, &dpi)
            .map(|l| l.cef_content_rect)
            .unwrap_or(ViewportRect::new(0, 0, 0, 0))
    }

    /// Compute layout from an IPC `ViewportBounds` payload, cache it, and return it.
    ///
    /// Callers should then call [`set_hwnd_bounds`] with the resulting rects.
    pub fn update_layout(
        &self,
        total_window_width: i32,
        total_window_height: i32,
        scale_factor: f64,
    ) -> Result<DualSurfaceLayout, EngineError> {
        let dpi = DpiContext::with_scale(scale_factor, scale_factor);
        let layout = self.compute_layout(total_window_width, total_window_height, &dpi)?;
        {
            let mut guard = self.last_layout.write()
                .map_err(|_| EngineError::Surface("NativeSurfaceManager layout lock poisoned".to_string()))?;
            *guard = Some(layout);
        }
        tracing::debug!(
            "NativeSurfaceManager: layout updated for {}x{} @ {:.2}x scale — cef_rect={:?}",
            total_window_width, total_window_height, scale_factor, layout.cef_content_rect
        );
        Ok(layout)
    }

    /// Return the last computed layout, if any.
    pub fn last_layout(&self) -> Option<DualSurfaceLayout> {
        self.last_layout.read().ok()?.as_ref().copied()
    }

    /// Synchronously reposition a native HWND using Win32 `SetWindowPos`.
    #[cfg(windows)]
    pub unsafe fn set_hwnd_bounds(hwnd: isize, rect: &ViewportRect) -> Result<(), EngineError> {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
        };

        let res = SetWindowPos(
            hwnd as HWND,
            0 as HWND,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );

        if res == 0 {
            Err(EngineError::Win32("SetWindowPos failed".to_string()))
        } else {
            Ok(())
        }
    }

    /// Complete Physical Bounds Enforcement Contract (CEF-06):
    /// 1. Calculate layout bounds from window dimensions and DPI.
    /// 2. Apply computed bounds to WebView2 presentation chrome HWND.
    /// 3. Apply computed bounds to CEF child content HWND.
    /// 4. Read back actual physical screen rectangles via Win32 `GetWindowRect`.
    /// 5. Assert zero physical overlap between native surfaces.
    #[cfg(target_os = "windows")]
    pub fn apply_and_verify_bounds(
        &self,
        webview2_hwnd: isize,
        cef_hwnd: isize,
        total_window_width: i32,
        total_window_height: i32,
        scale_factor: f64,
    ) -> Result<DualSurfaceLayout, EngineError> {
        let layout = self.update_layout(total_window_width, total_window_height, scale_factor)?;

        unsafe {
            // Apply CEF child content bounds
            Self::set_hwnd_bounds(cef_hwnd, &layout.cef_content_rect)?;
            // Apply WebView2 presentation chrome bounds
            Self::set_hwnd_bounds(webview2_hwnd, &layout.top_chrome_rect)?;
        }

        // Query actual OS rectangles and assert zero overlap
        verify_no_hwnd_overlap(webview2_hwnd, cef_hwnd)?;

        Ok(layout)
    }
}

/// Verify that two real Win32 HWNDs do not overlap on screen (Gate CEF-06B).
///
/// Calls Win32 `GetWindowRect` on both window handles and asserts zero intersection.
#[cfg(target_os = "windows")]
pub fn verify_no_hwnd_overlap(
    webview2_hwnd: isize,
    cef_hwnd: isize,
) -> Result<(), EngineError> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let mut wv2_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut cef_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    unsafe {
        if GetWindowRect(webview2_hwnd as _, &mut wv2_rect) == 0 {
            return Err(EngineError::Surface("Failed to GetWindowRect for WebView2 HWND".into()));
        }
        if GetWindowRect(cef_hwnd as _, &mut cef_rect) == 0 {
            return Err(EngineError::Surface("Failed to GetWindowRect for CEF HWND".into()));
        }
    }

    // Intersection check in Win32 RECT coordinates
    let overlap_x = wv2_rect.right > cef_rect.left && cef_rect.right > wv2_rect.left;
    let overlap_y = wv2_rect.bottom > cef_rect.top && cef_rect.bottom > wv2_rect.top;

    if overlap_x && overlap_y {
        return Err(EngineError::Surface(format!(
            "CEF-06B VIOLATION: WebView2 HWND [{},{},{},{}] overlaps CEF HWND [{},{},{},{}]",
            wv2_rect.left, wv2_rect.top, wv2_rect.right, wv2_rect.bottom,
            cef_rect.left, cef_rect.top, cef_rect.right, cef_rect.bottom,
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_layout_at_100_dpi() {
        let manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
        let dpi = DpiContext::standard();
        let layout = manager.compute_layout(1440, 900, &dpi).unwrap();

        assert_eq!(layout.top_chrome_rect, ViewportRect::new(0, 0, 1440, 88));
        assert_eq!(
            layout.sidebar_chrome_rect,
            Some(ViewportRect::new(0, 88, 320, 812))
        );
        assert_eq!(
            layout.cef_content_rect,
            ViewportRect::new(320, 88, 1120, 812)
        );
        assert!(layout.validate_no_overlap().is_ok());
    }

    #[test]
    fn test_compute_layout_at_150_dpi() {
        let manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
        let dpi = DpiContext::with_scale(1.5, 1.5);
        let layout = manager.compute_layout(2160, 1350, &dpi).unwrap();

        // 88 * 1.5 = 132; 320 * 1.5 = 480
        assert_eq!(layout.top_chrome_rect, ViewportRect::new(0, 0, 2160, 132));
        assert_eq!(
            layout.sidebar_chrome_rect,
            Some(ViewportRect::new(0, 132, 480, 1218))
        );
        assert_eq!(
            layout.cef_content_rect,
            ViewportRect::new(480, 132, 1680, 1218)
        );
        assert!(layout.validate_no_overlap().is_ok());
    }

    #[test]
    fn test_overlap_detector() {
        let r1 = ViewportRect::new(0, 0, 100, 100);
        let r2 = ViewportRect::new(50, 50, 100, 100);
        assert!(r1.intersects(&r2));

        let r3 = ViewportRect::new(100, 0, 100, 100);
        assert!(!r1.intersects(&r3)); // edge adjacent is not intersecting
    }

    #[test]
    fn test_update_layout_caches_result() {
        let manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());

        // Initially no cached layout
        assert!(manager.last_layout().is_none());

        // After update_layout, the cache is populated
        let layout = manager.update_layout(1440, 900, 1.0).unwrap();
        let cached = manager.last_layout().expect("Layout should be cached after update_layout");

        assert_eq!(layout, cached);
        assert_eq!(cached.cef_content_rect, ViewportRect::new(320, 88, 1120, 812));
    }

    #[test]
    fn test_update_layout_overrides_previous_cache() {
        let manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());

        manager.update_layout(1440, 900, 1.0).unwrap();
        manager.update_layout(1920, 1080, 1.0).unwrap();

        let cached = manager.last_layout().unwrap();
        // Second call's dimensions should be in the cache
        assert_eq!(cached.top_chrome_rect, ViewportRect::new(0, 0, 1920, 88));
    }

    #[test]
    fn test_update_layout_150_dpi_cef_rect_no_overlap() {
        let manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
        let layout = manager.update_layout(2160, 1350, 1.5).unwrap();
        // At 1.5x: top=132px, sidebar=480px
        assert_eq!(layout.cef_content_rect, ViewportRect::new(480, 132, 1680, 1218));
        assert!(layout.validate_no_overlap().is_ok());
    }
}
