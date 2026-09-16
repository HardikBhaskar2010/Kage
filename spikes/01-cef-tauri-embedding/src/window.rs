use crate::dpi::{DpiScale, LogicalRect};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct NativeEmbeddingHarness {
    pub parent_hwnd: HWND,
    pub child_viewport_hwnd: HWND,
    pub overlay_hwnd: HWND,
    pub current_dpi: DpiScale,
}

impl NativeEmbeddingHarness {
    /// Creates the complete native window hierarchy: Parent -> Child Viewport -> Topmost Glass Overlay
    pub unsafe fn create(dpi: DpiScale) -> Result<Self, String> {
        let instance = GetModuleHandleW(null_mut());

        // Register window classes
        let parent_class_name = wide_str("KageParentHostWindow");
        let child_class_name = wide_str("KageCefChildViewport");
        let overlay_class_name = wide_str("KageLiquidGlassOverlay");

        register_class(instance, parent_class_name.as_ptr(), Some(DefWindowProcW));
        register_class(instance, child_class_name.as_ptr(), Some(DefWindowProcW));
        register_class(instance, overlay_class_name.as_ptr(), Some(DefWindowProcW));

        // 1. Create Top-Level Parent Host Window
        let parent_logical = LogicalRect { x: 100, y: 100, width: 1280, height: 800 };
        let parent_phys = dpi.logical_rect_to_physical(parent_logical);

        let parent_hwnd = CreateWindowExW(
            0,
            parent_class_name.as_ptr(),
            wide_str("KAGE Shell Host (Tauri Host Simulated)").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            parent_phys.x,
            parent_phys.y,
            parent_phys.width,
            parent_phys.height,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );

        if parent_hwnd == null_mut() {
            return Err("Failed to create parent host window".to_string());
        }

        // 2. Create Child Viewport (Simulated CEF surface)
        // Viewport occupies area below omnibox (top 80 logical pixels)
        let viewport_logical = LogicalRect { x: 0, y: 80, width: 1280, height: 720 };
        let viewport_phys = dpi.logical_rect_to_physical(viewport_logical);

        let child_viewport_hwnd = CreateWindowExW(
            0,
            child_class_name.as_ptr(),
            wide_str("CEF Render Viewport").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            viewport_phys.x,
            viewport_phys.y,
            viewport_phys.width,
            viewport_phys.height,
            parent_hwnd,
            null_mut(),
            instance,
            null_mut(),
        );

        if child_viewport_hwnd == null_mut() {
            DestroyWindow(parent_hwnd);
            return Err("Failed to create child viewport window".to_string());
        }

        // 3. Create Liquid Glass Floating Overlay
        // Positioned at (x: 240, y: 40, width: 800, height: 260)
        // Intentionally straddles BOTH top chrome (y < 80) and child viewport (y > 80)!
        let overlay_logical = LogicalRect { x: 240, y: 40, width: 800, height: 260 };
        let overlay_phys = dpi.logical_rect_to_physical(overlay_logical);

        let overlay_hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            overlay_class_name.as_ptr(),
            wide_str("Liquid Glass Floating Overlay").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            overlay_phys.x,
            overlay_phys.y,
            overlay_phys.width,
            overlay_phys.height,
            parent_hwnd,
            null_mut(),
            instance,
            null_mut(),
        );

        if overlay_hwnd == null_mut() {
            DestroyWindow(child_viewport_hwnd);
            DestroyWindow(parent_hwnd);
            return Err("Failed to create overlay window".to_string());
        }

        // Ensure Overlay is positioned above Child Viewport in Z-order
        SetWindowPos(
            overlay_hwnd,
            HWND_TOP,
            overlay_phys.x,
            overlay_phys.y,
            overlay_phys.width,
            overlay_phys.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );

        Ok(Self {
            parent_hwnd,
            child_viewport_hwnd,
            overlay_hwnd,
            current_dpi: dpi,
        })
    }

    /// Synchronously resizes and relocates all children when parent or DPI changes
    pub unsafe fn sync_layout(&mut self, new_parent_width: i32, new_parent_height: i32, new_dpi: DpiScale) {
        self.current_dpi = new_dpi;

        // Viewport fills below 80 logical px top chrome
        let top_chrome_phys_y = new_dpi.logical_to_physical(80);
        let viewport_phys_height = (new_parent_height - top_chrome_phys_y).max(0);

        // DeferWindowPos to update both child viewport and overlay in a single atomic compositor pass (zero flicker)
        let hdwp = BeginDeferWindowPos(2);
        if hdwp != null_mut() {
            // Update child viewport
            let hdwp = DeferWindowPos(
                hdwp,
                self.child_viewport_hwnd,
                null_mut(),
                0,
                top_chrome_phys_y,
                new_parent_width,
                viewport_phys_height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );

            // Update floating overlay
            let overlay_logical = LogicalRect { x: 240, y: 40, width: 800, height: 260 };
            let overlay_phys = new_dpi.logical_rect_to_physical(overlay_logical);

            let hdwp = DeferWindowPos(
                hdwp,
                self.overlay_hwnd,
                HWND_TOP,
                overlay_phys.x,
                overlay_phys.y,
                overlay_phys.width,
                overlay_phys.height,
                SWP_NOACTIVATE,
            );

            EndDeferWindowPos(hdwp);
        }
    }

    /// Performs hit-testing to verify pointer routing:
    /// Does the overlay receive the click, or does it fall through to the child viewport?
    pub unsafe fn hit_test(&self, client_x: i32, client_y: i32) -> &'static str {
        let pt = POINT { x: client_x, y: client_y };
        let hit_hwnd = ChildWindowFromPointEx(self.parent_hwnd, pt, CWP_ALL);

        if hit_hwnd == self.overlay_hwnd {
            "OVERLAY"
        } else if hit_hwnd == self.child_viewport_hwnd {
            "CHILD_VIEWPORT"
        } else if hit_hwnd == self.parent_hwnd {
            "PARENT_CHROME"
        } else {
            "OUTSIDE"
        }
    }

    pub unsafe fn destroy(&mut self) {
        if self.overlay_hwnd != null_mut() {
            DestroyWindow(self.overlay_hwnd);
            self.overlay_hwnd = null_mut();
        }
        if self.child_viewport_hwnd != null_mut() {
            DestroyWindow(self.child_viewport_hwnd);
            self.child_viewport_hwnd = null_mut();
        }
        if self.parent_hwnd != null_mut() {
            DestroyWindow(self.parent_hwnd);
            self.parent_hwnd = null_mut();
        }
    }
}

unsafe fn register_class(instance: HMODULE, class_name: *const u16, wndproc: WNDPROC) {
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = wndproc;
    wc.hInstance = instance;
    wc.lpszClassName = class_name;
    wc.hbrBackground = (COLOR_WINDOW + 1) as HBRUSH;
    RegisterClassW(&wc);
}

fn wide_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}
