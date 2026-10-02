use super::*;
use crate::render_graph::frame::Frame;
use crate::renderer::types::{DrawList, UIDrawList};

#[test]
fn test_graphics_lists_cannot_be_silently_submitted_to_compute_or_transfer() {
    for pass_type in [PassType::Compute, PassType::Transfer] {
        let mut graph = TestGraph::new();
        let pass = graph
            .add_pass(PassDesc::new("commands", pass_type, vec![], vec![]))
            .unwrap();
        graph.compile().unwrap();
        let mut backend = MockBackend::new();
        let mut frame = Frame::new(&graph, &mut backend, 0, 0);
        frame.submit(pass, std::rc::Rc::new(DrawList::new()));
        assert!(matches!(
            frame.validate_submissions(),
            Err(RenderGraphError::InvalidConfiguration(_))
        ));
    }
}

#[test]
fn test_ui_and_geometry_inputs_require_their_declared_pass_kind() {
    for ui_pass in [false, true] {
        let mut graph = TestGraph::new();
        let mut desc = PassDesc::new("draw", PassType::Graphics, vec![], vec![]);
        desc.kind = Some(if ui_pass {
            super::super::super::PassKind::Ui
        } else {
            super::super::super::PassKind::Geometry
        });
        let pass = graph.add_pass(desc).unwrap();
        graph.compile().unwrap();
        let mut backend = MockBackend::new();
        let mut frame = Frame::new(&graph, &mut backend, 0, 0);
        if ui_pass {
            frame.submit(pass, std::rc::Rc::new(DrawList::new()));
        } else {
            frame.submit_ui(pass, &UIDrawList::default());
        }
        assert!(matches!(
            frame.validate_submissions(),
            Err(RenderGraphError::InvalidConfiguration(_))
        ));
    }
}

#[test]
fn test_ui_lists_are_composed_before_submission_on_every_backend() {
    let mut graph = TestGraph::new();
    let mut desc = PassDesc::new("ui", PassType::Graphics, vec![], vec![]);
    desc.kind = Some(super::super::super::PassKind::Ui);
    let pass = graph.add_pass(desc).unwrap();
    graph.compile().unwrap();
    let mut backend = MockBackend::new();
    let mut frame = Frame::new(&graph, &mut backend, 0, 0);
    frame.submit_ui(pass, &UIDrawList::default());
    frame.validate_submissions().unwrap();
    frame.submit_ui(pass, &UIDrawList::default());
    let error = frame.validate_submissions().unwrap_err();
    assert!(
        error.to_string().contains("submit one composed list"),
        "{error}"
    );
}
