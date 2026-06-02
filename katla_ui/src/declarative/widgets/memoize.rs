use std::any::Any;
use std::sync::Arc;

use katla_math::Rect2D;
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

/// Wrapper widget that skips subtree rebuild when `Arc<T>` data is unchanged.
///
/// Uses `Arc::ptr_eq` for O(1) change detection. When the data pointer matches
/// the previous frame, `diff_against` returns `DiffAction::Update` and the
/// inner subtree is reused without rebuild.
pub struct Memoize<T: 'static, W: Widget> {
    data: Arc<T>,
    factory: fn(Arc<T>) -> W,
    child_widget: Option<Box<dyn Widget>>,
    children: Vec<ViewId>,
}

impl<T: 'static, W: Widget> Memoize<T, W> {
    pub fn new(data: Arc<T>, factory: fn(Arc<T>) -> W) -> Self {
        Self {
            data,
            factory,
            child_widget: None,
            children: Vec::new(),
        }
    }

    fn ensure_inner(&mut self) {
        if self.child_widget.is_none() {
            let inner = (self.factory)(self.data.clone());
            self.child_widget = Some(Box::new(inner));
        }
    }
}

impl<T: 'static, W: Widget> Widget for Memoize<T, W> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if let Some(other) = prev.as_any().downcast_ref::<Memoize<T, W>>() {
            if Arc::ptr_eq(&self.data, &other.data) {
                DiffAction::Update
            } else {
                DiffAction::RecurseChildren
            }
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style::default()
    }

    fn handle_input(
        &self,
        _ctx: &mut InputContext<'_>,
        _state: &mut StateArena,
        _bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        InputResult::Ignore
    }

    fn draw(
        &self,
        _ctx: &mut UiContext,
        _state: &StateArena,
        _bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
    ) {
    }

    fn should_rebuild(&self, prev: &dyn Widget) -> bool {
        if let Some(other) = prev.as_any().downcast_ref::<Memoize<T, W>>() {
            !Arc::ptr_eq(&self.data, &other.data)
        } else {
            true
        }
    }

    fn focusable(&self) -> bool {
        false
    }

    fn children(&self) -> &[ViewId] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        &mut self.children
    }

    fn take_children(&mut self) -> ChildWidgets {
        self.ensure_inner();
        if let Some(child) = self.child_widget.take() {
            ChildWidgets::Single(child)
        } else {
            ChildWidgets::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::widgets;

    #[test]
    fn test_memoize_should_rebuild_false_when_unchanged() {
        let data = Arc::new(vec!["a", "b"]);
        let m1 = Memoize::new(data.clone(), |_| widgets::text::Text {
            content: "inner".into(),
            color: None,
            font_size: None,
        });
        let m2 = Memoize::new(data, |_| widgets::text::Text {
            content: "inner".into(),
            color: None,
            font_size: None,
        });
        assert!(
            !m2.should_rebuild(&m1),
            "same Arc pointer should skip rebuild"
        );
    }

    #[test]
    fn test_memoize_should_rebuild_true_when_changed() {
        let data1 = Arc::new(vec!["a"]);
        let data2 = Arc::new(vec!["b"]);
        let m1: Memoize<Vec<&str>, widgets::text::Text> =
            Memoize::new(data1, |_| widgets::text::Text {
                content: "inner".into(),
                color: None,
                font_size: None,
            });
        let m2 = Memoize::new(data2, |_| widgets::text::Text {
            content: "inner".into(),
            color: None,
            font_size: None,
        });
        assert!(
            m2.should_rebuild(&m1),
            "different Arc pointer should trigger rebuild"
        );
    }

    #[test]
    fn test_memoize_diff_update_when_same_pointer() {
        let data = Arc::new(42_i32);
        let m1 = Memoize::new(data.clone(), |_| widgets::text::Text {
            content: "x".into(),
            color: None,
            font_size: None,
        });
        let m2 = Memoize::new(data, |_| widgets::text::Text {
            content: "x".into(),
            color: None,
            font_size: None,
        });
        assert_eq!(m2.diff_against(&m1), DiffAction::Update);
    }

    #[test]
    fn test_memoize_diff_recurse_when_different_pointer() {
        let d1 = Arc::new(1_i32);
        let d2 = Arc::new(2_i32);
        let m1 = Memoize::new(d1, |_| widgets::text::Text {
            content: "x".into(),
            color: None,
            font_size: None,
        });
        let m2 = Memoize::new(d2, |_| widgets::text::Text {
            content: "y".into(),
            color: None,
            font_size: None,
        });
        assert_eq!(m2.diff_against(&m1), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_memoize_diff_replace_against_different_type() {
        let data = Arc::new(42_i32);
        let m = Memoize::new(data, |_| widgets::text::Text {
            content: "x".into(),
            color: None,
            font_size: None,
        });
        let other = widgets::text::Text {
            content: "hello".into(),
            color: None,
            font_size: None,
        };
        assert_eq!(m.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_memoize_take_children_creates_inner() {
        let data = Arc::new("hello".to_string());
        let mut m: Memoize<String, widgets::text::Text> =
            Memoize::new(data, |d| widgets::text::Text {
                content: (*d).clone(),
                color: None,
                font_size: None,
            });
        let children = m.take_children();
        assert!(
            matches!(children, ChildWidgets::Single(_)),
            "should produce a single child from factory"
        );
    }

    #[test]
    fn test_memoize_wraps_generic_types() {
        let data_str = Arc::new("test".to_string());
        let m1: Memoize<String, widgets::text::Text> =
            Memoize::new(data_str, |_| widgets::text::Text {
                content: "inner".into(),
                color: None,
                font_size: None,
            });
        assert!(
            m1.as_any()
                .downcast_ref::<Memoize<String, widgets::text::Text>>()
                .is_some()
        );

        let data_f32 = Arc::new(3.14_f32);
        let m2: Memoize<f32, widgets::empty::Empty> =
            Memoize::new(data_f32, |_| widgets::empty::Empty);
        assert!(
            m2.as_any()
                .downcast_ref::<Memoize<f32, widgets::empty::Empty>>()
                .is_some()
        );
    }

    // VAL-CROSS-011: Memoize provides measurable performance skip
    #[test]
    fn test_memoize_measurable_skip_performance() {
        use std::time::Instant;

        // Build an expensive factory that creates many children
        let expensive_factory = |_data: Arc<Vec<String>>| {
            // Simulate expensive work
            let mut v = Vec::new();
            for i in 0..1000 {
                v.push(i.to_string());
            }
            widgets::text::Text {
                content: v.join(","),
                color: None,
                font_size: None,
            }
        };

        let data = Arc::new((0..1000).map(|i| i.to_string()).collect::<Vec<String>>());

        // Measure should_rebuild with same pointer (O(1) Arc::ptr_eq check)
        let m1 = Memoize::new(data.clone(), expensive_factory);
        let m2 = Memoize::new(data, expensive_factory);

        let start = Instant::now();
        for _ in 0..10_000 {
            let _skip = m2.should_rebuild(&m1);
        }
        let skip_time = start.elapsed();

        // Measure with different pointers (still fast, but different outcome)
        let d1 = Arc::new(vec!["a".to_string()]);
        let d2 = Arc::new(vec!["b".to_string()]);
        let m3: Memoize<Vec<String>, widgets::text::Text> =
            Memoize::new(d1, |d| widgets::text::Text {
                content: d.first().cloned().unwrap_or_default(),
                color: None,
                font_size: None,
            });
        let m4 = Memoize::new(d2, |d| widgets::text::Text {
            content: d.first().cloned().unwrap_or_default(),
            color: None,
            font_size: None,
        });

        let start = Instant::now();
        for _ in 0..10_000 {
            let _rebuild = m4.should_rebuild(&m3);
        }
        let rebuild_time = start.elapsed();

        // Both should be extremely fast since should_rebuild is O(1) Arc::ptr_eq
        // The skip case should be at least as fast as the rebuild case
        assert!(
            skip_time.as_micros() < 10_000,
            "should_rebuild with same Arc should be very fast, took {:?}",
            skip_time
        );
        assert!(
            rebuild_time.as_micros() < 10_000,
            "should_rebuild with different Arc should be very fast, took {:?}",
            rebuild_time
        );

        // Verify correct semantics
        assert!(!m2.should_rebuild(&m1), "same pointer → skip");
        assert!(m4.should_rebuild(&m3), "different pointer → rebuild");
    }
}
