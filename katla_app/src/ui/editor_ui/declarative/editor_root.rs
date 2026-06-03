use std::boxed::Box;

use katla_math::Vec2;
use katla_ui::declarative::FlexProps;
use katla_ui::declarative::widgets::dock_space::{DockDragState, DockSpace};
use katla_ui::declarative::{
    Alignment, Anchor, Build, BuildContext, Widget, WidgetBox, overlay, zstack,
};
use katla_ui::dock::DockTree;

use super::asset_browser::AssetBrowserDrawCtx;
use super::asset_browser::AssetBrowserView;
use super::co_creator::CoCreatorView;
use super::console::ConsoleDrawCtx;
use super::console::ConsoleView;
use super::gizmo::GizmoButtonsView;
use super::hierarchy::HierarchyDrawCtx;
use super::hierarchy::HierarchyView;
use super::inspector::InspectorDrawCtx;
use super::inspector::InspectorView;
use super::mixer::MixerDrawCtx;
use super::mixer::MixerView;
use super::particle_inspector::ParticleInspectorView;
use super::preferences::PreferencesView;
use super::status_bar::StatusBarView;
use super::toolbar::{TOOLBAR_HEIGHT, ToolbarView};
use super::viewport_grid::ViewportGridDrawCtx;
use super::viewport_grid::ViewportGridView;

use super::super::types::EditorPanel;

pub(crate) const STATUS_BAR_HEIGHT: f32 = 22.0;

/// Panel labels for the DockSpace tab bars.
fn panel_labels() -> Vec<(u64, String)> {
    EditorPanel::all_editor_panels()
        .iter()
        .map(|p| (p.id(), p.name().to_string()))
        .collect()
}

/// Full editor view. Docked panels are positioned via Overlay with their
/// dock-computed bounds. The DockSpace widget renders chrome (tab bars,
/// splitter handles, drag overlay) on top. Overlay panels (toolbar, status bar,
/// gizmo, floating panels) use ZStack alignment.
pub(crate) struct EditorOverlayView;

impl Build for EditorOverlayView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        // Docked panels: read bounds.min from their specific env context, wrap in Overlay
        let viewport_offset = ctx
            .env::<ViewportGridDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let viewport_grid = overlay(
            Anchor::TopLeft,
            viewport_offset,
            ViewportGridView.build(ctx),
        );

        let hierarchy_offset = ctx
            .env::<HierarchyDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let hierarchy = overlay(Anchor::TopLeft, hierarchy_offset, HierarchyView.build(ctx));

        let inspector_offset = ctx
            .env::<InspectorDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let inspector = overlay(Anchor::TopLeft, inspector_offset, InspectorView.build(ctx));

        let asset_offset = ctx
            .env::<AssetBrowserDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let asset_browser = overlay(Anchor::TopLeft, asset_offset, AssetBrowserView.build(ctx));

        let console_offset = ctx
            .env::<ConsoleDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let console = overlay(Anchor::TopLeft, console_offset, ConsoleView.build(ctx));

        let mixer_offset = ctx
            .env::<MixerDrawCtx>()
            .map(|c| c.bounds.min)
            .unwrap_or_default();
        let mixer = overlay(Anchor::TopLeft, mixer_offset, MixerView.build(ctx));

        // DockSpace: reads DockTree from StateArena, renders chrome only (no children)
        let initial_tree: DockTree<u64> = ctx
            .env::<DockTree<u64>>()
            .cloned()
            .unwrap_or_else(|| DockTree::new(katla_ui::dock::DockNode::Empty));
        let dock_state_id = ctx.state(initial_tree);
        let drag_state_id = ctx.state(DockDragState::<u64>::default());
        let mut dockspace = DockSpace::new(
            dock_state_id,
            drag_state_id,
            panel_labels(),
            vec![],
            FlexProps::default(),
        );
        dockspace.content_inset_top = TOOLBAR_HEIGHT;
        dockspace.content_inset_bottom = STATUS_BAR_HEIGHT;

        // Overlay panels: use ZStack alignment
        let toolbar = ToolbarView.build(ctx);
        let status_bar = StatusBarView.build(ctx);
        let gizmo_offset = ctx
            .env::<ViewportGridDrawCtx>()
            .map(|c| Vec2::new(c.bounds.min.x() + 8.0, c.bounds.min.y() + 8.0))
            .unwrap_or(Vec2::new(0.0, TOOLBAR_HEIGHT));
        let gizmo = overlay(Anchor::TopLeft, gizmo_offset, GizmoButtonsView.build(ctx));
        let co_creator = CoCreatorView.build(ctx);
        let particle_inspector = ParticleInspectorView.build(ctx);
        let preferences = PreferencesView.build(ctx);

        zstack([
            // Docked panels — positioned via Overlay offset
            (Alignment::TopLeading, viewport_grid.boxed()),
            (Alignment::TopLeading, hierarchy.boxed()),
            (Alignment::TopLeading, inspector.boxed()),
            (Alignment::TopLeading, asset_browser.boxed()),
            (Alignment::TopLeading, console.boxed()),
            (Alignment::TopLeading, mixer.boxed()),
            // DockSpace chrome — tabs, splitters, drag overlay
            (Alignment::TopLeading, dockspace.boxed()),
            // Overlay panels — positioned via ZStack alignment
            (Alignment::TopLeading, toolbar),
            (Alignment::BottomLeading, status_bar),
            (Alignment::TopLeading, gizmo.boxed()),
            (Alignment::TopLeading, co_creator),
            (Alignment::TopLeading, particle_inspector),
            (Alignment::TopLeading, preferences),
        ])
        .boxed()
    }
}
