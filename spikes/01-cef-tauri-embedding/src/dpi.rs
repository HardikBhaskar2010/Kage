#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiScale {
    pub dpi: u32,
    pub factor: f64,
}

impl DpiScale {
    pub fn from_dpi(dpi: u32) -> Self {
        Self {
            dpi,
            factor: dpi as f64 / 96.0,
        }
    }

    pub const DPI_100: DpiScale = DpiScale { dpi: 96, factor: 1.0 };
    pub const DPI_125: DpiScale = DpiScale { dpi: 120, factor: 1.25 };
    pub const DPI_150: DpiScale = DpiScale { dpi: 144, factor: 1.50 };
    pub const DPI_200: DpiScale = DpiScale { dpi: 192, factor: 2.0 };

    pub fn logical_to_physical(&self, logical_px: i32) -> i32 {
        ((logical_px as f64) * self.factor).round() as i32
    }

    pub fn physical_to_logical(&self, physical_px: i32) -> i32 {
        ((physical_px as f64) / self.factor).round() as i32
    }

    pub fn logical_rect_to_physical(&self, rect: LogicalRect) -> PhysicalRect {
        PhysicalRect {
            x: self.logical_to_physical(rect.x),
            y: self.logical_to_physical(rect.y),
            width: self.logical_to_physical(rect.width),
            height: self.logical_to_physical(rect.height),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PhysicalRect {
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < (self.x + self.width) && py >= self.y && py < (self.y + self.height)
    }
}
