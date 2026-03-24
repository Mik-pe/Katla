use katla_math::{Rect2D, Vec2};

use super::UiContext;

impl UiContext {
    /// Get the current clip rectangle.
    #[inline]
    pub fn clip_rect(&self) -> Rect2D {
        *self
            .clip_stack
            .last()
            .unwrap_or(&Rect2D::from_size(self.screen_size))
    }

    /// Push a new clip rectangle (intersection with current).
    pub fn push_clip(&mut self, rect: Rect2D) {
        let current = self.clip_rect();
        let clipped = current
            .intersection(&rect)
            .unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)));
        self.clip_stack.push(clipped);
        self.draw_list.set_clip(clipped);
    }

    /// Push an absolute clip rectangle (no intersection with current).
    ///
    /// Use this for popups that need to render outside their parent container.
    pub fn push_clip_absolute(&mut self, rect: Rect2D) {
        self.clip_stack.push(rect);
        self.draw_list.set_clip(rect);
    }

    /// Pop a clip rectangle.
    pub fn pop_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
            self.draw_list.set_clip(self.clip_rect());
        }
    }
}
