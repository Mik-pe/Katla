use katla_ui::declarative::{Alignment, Build, BuildContext, ViewDescriptor, zstack};

use super::asset_browser::AssetBrowserView;
use super::co_creator::CoCreatorView;
use super::console::ConsoleView;
use super::gizmo::GizmoButtonsView;
use super::hierarchy::HierarchyView;
use super::inspector::InspectorView;
use super::mixer::MixerView;
use super::particle_inspector::ParticleInspectorView;
use super::preferences::PreferencesView;
use super::status_bar::StatusBarView;
use super::toolbar::ToolbarView;
use super::viewport_grid::ViewportGridView;

pub(crate) struct EditorRootView;

impl Build for EditorRootView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let toolbar = ToolbarView.build(ctx);
        let status_bar = StatusBarView.build(ctx);
        let gizmo = GizmoButtonsView.build(ctx);
        let inspector = InspectorView.build(ctx);
        let viewport_grid = ViewportGridView.build(ctx);
        let hierarchy = HierarchyView.build(ctx);
        let co_creator = CoCreatorView.build(ctx);
        let particle_inspector = ParticleInspectorView.build(ctx);
        let preferences = PreferencesView.build(ctx);
        let asset_browser = AssetBrowserView.build(ctx);
        let console = ConsoleView.build(ctx);
        let mixer = MixerView.build(ctx);

        zstack([
            (Alignment::TopLeading, viewport_grid),
            (Alignment::TopLeading, hierarchy),
            (Alignment::TopLeading, toolbar),
            (Alignment::BottomLeading, status_bar),
            (Alignment::TopLeading, gizmo),
            (Alignment::TopLeading, inspector),
            (Alignment::TopLeading, co_creator),
            (Alignment::TopLeading, particle_inspector),
            (Alignment::TopLeading, preferences),
            (Alignment::TopLeading, asset_browser),
            (Alignment::TopLeading, console),
            (Alignment::TopLeading, mixer),
        ])
    }
}
