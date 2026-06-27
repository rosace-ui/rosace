//! [`Center`] — a layout helper that centers a child within its container.

/// A layout helper that calculates the top-left position needed to center a
/// child widget of known dimensions inside a container of known dimensions.
///
/// No rendering is performed — the caller draws the child at the returned
/// origin.
pub struct Center {
    pub child_width: f32,
    pub child_height: f32,
    pub container_width: f32,
    pub container_height: f32,
}

impl Center {
    /// Creates a new [`Center`] with explicit child and container dimensions.
    pub fn new(child_w: f32, child_h: f32, container_w: f32, container_h: f32) -> Self {
        Self {
            child_width: child_w,
            child_height: child_h,
            container_width: container_w,
            container_height: container_h,
        }
    }

    /// Returns the (x, y) origin at which the child should be drawn so that it
    /// appears centered inside the container whose top-left corner is `(x, y)`.
    pub fn child_origin(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x + (self.container_width  - self.child_width)  / 2.0,
            y + (self.container_height - self.child_height) / 2.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_child_origin_centers_correctly() {
        let c = Center::new(40.0, 20.0, 200.0, 100.0);
        let (cx, cy) = c.child_origin(0.0, 0.0);
        assert_eq!(cx, 80.0);  // (200 - 40) / 2
        assert_eq!(cy, 40.0);  // (100 - 20) / 2
    }

    #[test]
    fn center_child_origin_accounts_for_container_offset() {
        let c = Center::new(20.0, 10.0, 100.0, 50.0);
        let (cx, cy) = c.child_origin(50.0, 25.0);
        assert_eq!(cx, 50.0 + 40.0); // 50 + (100-20)/2
        assert_eq!(cy, 25.0 + 20.0); // 25 + (50-10)/2
    }
}
