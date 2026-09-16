pub mod dpi;
pub mod window;

pub use dpi::{DpiScale, LogicalRect, PhysicalRect};
pub use window::NativeEmbeddingHarness;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpi_scale_factors() {
        assert_eq!(DpiScale::DPI_100.dpi, 96);
        assert_eq!(DpiScale::DPI_125.dpi, 120);
        assert_eq!(DpiScale::DPI_150.dpi, 144);
        assert_eq!(DpiScale::DPI_200.dpi, 192);

        assert!((DpiScale::DPI_100.factor - 1.0).abs() < f64::EPSILON);
        assert!((DpiScale::DPI_125.factor - 1.25).abs() < f64::EPSILON);
        assert!((DpiScale::DPI_150.factor - 1.50).abs() < f64::EPSILON);
        assert!((DpiScale::DPI_200.factor - 2.00).abs() < f64::EPSILON);
    }

    #[test]
    fn test_logical_to_physical_conversion() {
        let logical = LogicalRect { x: 100, y: 50, width: 800, height: 600 };
        let phys_100 = DpiScale::DPI_100.logical_rect_to_physical(logical);
        let phys_150 = DpiScale::DPI_150.logical_rect_to_physical(logical);

        assert_eq!(phys_100.width, 800);
        assert_eq!(phys_100.height, 600);

        assert_eq!(phys_150.width, 1200);
        assert_eq!(phys_150.height, 900);
    }

    #[test]
    fn test_physical_rect_hit_testing() {
        let rect = PhysicalRect { x: 200, y: 100, width: 400, height: 300 };
        assert!(rect.contains_point(250, 150));
        assert!(rect.contains_point(200, 100));
        assert!(!rect.contains_point(199, 100));
        assert!(!rect.contains_point(600, 400));
    }
}
