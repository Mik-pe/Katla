use super::*;
use crate::render_graph::handles::ResourceId;
use crate::render_graph::pass::PassKind;
use crate::render_pass::AttachmentOps;
use crate::texture::ImageFormat;

fn resource(name: &str) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.to_string(),
        resource_type: super::super::resource::GraphResourceType::ColorAttachment {
            clear_value: None,
        },
        format: ImageFormat::R8G8B8A8Unorm,
        width: 64,
        height: 64,
        tracks_swapchain_size: false,
    }
}

fn graphics_pass(name: &str, colors: Vec<ResourceId>) -> PassDesc {
    let mut pass = PassDesc::new(name, PassType::Graphics, Vec::new(), colors.clone());
    pass.color_attachments = colors
        .into_iter()
        .map(|id| (id, AttachmentOps::load()))
        .collect();
    pass.uses_depth = false;
    pass
}

fn entry(pass_index: usize, name: &str, position: usize) -> ResourceExecutionTraceEntry {
    ResourceExecutionTraceEntry {
        pass_index,
        name: name.to_string(),
        pass_type: PassType::Graphics,
        encode_position: position,
        outcome: EmittedPassOutcome::Encoded,
        draw_calls: 1,
        instances: 1,
        color_targets: Vec::new(),
        depth_target: None,
        color_attachment_ops: vec![AttachmentOps::load()],
        depth_attachment_ops: None,
    }
}

#[test]
fn a_matching_trace_has_no_divergences() {
    let resources = vec![resource("color")];
    let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
    let mut trace = ResourceExecutionTrace::new();
    let mut e = entry(0, "only", 0);
    e.color_targets = vec!["color".to_string()];
    trace.push(e);

    assert_eq!(
        compare_with_compiled(&resources, &passes, &[0], &trace),
        vec![]
    );
}

#[test]
fn test_native_attachment_operations_diverge_even_when_targets_match() {
    let resources = vec![resource("color")];
    let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
    let mut emitted = entry(0, "only", 0);
    emitted.color_targets = vec!["color".into()];
    emitted.color_attachment_ops[0].store = crate::render_pass::StoreOp::DontCare;
    let mut trace = ResourceExecutionTrace::new();
    trace.push(emitted);
    assert!(matches!(
        compare_with_compiled(&resources, &passes, &[0], &trace).as_slice(),
        [TraceDivergence::AttachmentOpsMismatch {
            aspect: "color",
            ..
        }]
    ));
}

#[test]
fn test_duplicate_dispatch_is_reported() {
    let resources = vec![resource("color")];
    let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
    let mut emitted = entry(0, "only", 0);
    emitted.color_targets = vec!["color".into()];
    let mut trace = ResourceExecutionTrace::new();
    trace.push(emitted.clone());
    trace.push(emitted);
    assert!(
        compare_with_compiled(&resources, &passes, &[0], &trace)
            .iter()
            .any(|divergence| matches!(
                divergence,
                TraceDivergence::DuplicatePass { pass_index: 0, .. }
            ))
    );
}

#[test]
fn a_live_pass_the_backend_never_encoded_is_reported() {
    let resources = vec![resource("color")];
    let passes = vec![graphics_pass("skipped", vec![ResourceId(0)])];

    let divergences =
        compare_with_compiled(&resources, &passes, &[0], &ResourceExecutionTrace::new());

    assert_eq!(
        divergences,
        vec![TraceDivergence::MissingPass {
            pass_index: 0,
            name: "skipped".to_string()
        }]
    );
}

#[test]
fn an_encoded_pass_outside_the_compiled_plan_is_reported() {
    let resources = vec![resource("color")];
    let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
    let mut trace = ResourceExecutionTrace::new();
    trace.push(entry(7, "ghost", 0));

    let divergences = compare_with_compiled(&resources, &passes, &[0], &trace);

    assert!(divergences.contains(&TraceDivergence::UnknownPass {
        pass_index: 7,
        name: "<out of range>".to_string()
    }));
}

#[test]
fn encoded_passes_in_the_wrong_order_are_reported() {
    let resources = vec![resource("color")];
    let passes = vec![
        graphics_pass("first", vec![ResourceId(0)]),
        graphics_pass("second", vec![ResourceId(0)]),
    ];
    let mut trace = ResourceExecutionTrace::new();
    let mut second = entry(1, "second", 0);
    second.color_targets = vec!["color".to_string()];
    let mut first = entry(0, "first", 1);
    first.color_targets = vec!["color".to_string()];
    trace.push(second);
    trace.push(first);

    let divergences = compare_with_compiled(&resources, &passes, &[0, 1], &trace);

    assert!(divergences.iter().any(|d| matches!(
        d,
        TraceDivergence::OrderMismatch { expected, emitted }
            if expected == &["first".to_string(), "second".to_string()]
                && emitted == &["second".to_string(), "first".to_string()]
    )));
}

#[test]
fn a_color_target_the_encoder_did_not_bind_is_reported() {
    let resources = vec![resource("declared")];
    let passes = vec![graphics_pass("pass", vec![ResourceId(0)])];
    let mut trace = ResourceExecutionTrace::new();
    let mut e = entry(0, "pass", 0);
    e.color_targets = vec!["other".to_string()];
    trace.push(e);

    let divergences = compare_with_compiled(&resources, &passes, &[0], &trace);

    assert!(
        divergences.contains(&TraceDivergence::ColorTargetsMismatch {
            pass_index: 0,
            name: "pass".to_string(),
            compiled: vec!["declared".to_string()],
            emitted: vec!["other".to_string()],
        })
    );
}

#[test]
fn a_depth_binding_the_pass_did_not_declare_is_reported() {
    let resources = vec![resource("color")];
    let mut pass = graphics_pass("pass", vec![ResourceId(0)]);
    pass.uses_depth = false;
    let mut trace = ResourceExecutionTrace::new();
    let mut e = entry(0, "pass", 0);
    e.color_targets = vec!["color".to_string()];
    e.depth_target = Some("depth".to_string());
    trace.push(e);

    let divergences = compare_with_compiled(&resources, &[pass], &[0], &trace);

    assert!(divergences.contains(&TraceDivergence::DepthTargetMismatch {
        pass_index: 0,
        name: "pass".to_string(),
        compiled: None,
        emitted: Some("depth".to_string()),
    }));
}

#[test]
fn a_skipped_pass_is_recorded_but_not_an_ordering_divergence() {
    let resources = vec![resource("color")];
    let passes = vec![
        graphics_pass("empty", vec![ResourceId(0)]),
        graphics_pass("real", vec![ResourceId(0)]),
    ];
    let mut trace = ResourceExecutionTrace::new();
    let mut first = entry(0, "empty", 0);
    first.outcome = EmittedPassOutcome::SkippedNoWork;
    first.color_targets = vec!["color".to_string()];
    let mut second = entry(1, "real", 1);
    second.color_targets = vec!["color".to_string()];
    trace.push(first);
    trace.push(second);

    assert_eq!(
        compare_with_compiled(&resources, &passes, &[0, 1], &trace),
        vec![]
    );
}

#[test]
fn divergence_messages_name_the_pass_and_both_sides() {
    let divergence = TraceDivergence::OrderMismatch {
        expected: vec!["a".to_string()],
        emitted: vec!["b".to_string()],
    };
    let message = divergence.to_string();
    assert!(message.contains("compiled [a]"));
    assert!(message.contains("emitted [b]"));

    let missing = TraceDivergence::MissingPass {
        pass_index: 2,
        name: "shadow".to_string(),
    };
    assert!(missing.to_string().contains("pass 2 ('shadow')"));
}

#[test]
fn text_export_lists_every_encoder_in_order() {
    let mut trace = ResourceExecutionTrace::new();
    let mut e = entry(0, "geometry", 0);
    e.color_targets = vec!["hdr".to_string()];
    trace.push(e);

    let text = trace.to_string();

    assert!(text.contains("1 emitted encoder entries"));
    assert!(text.contains("geometry"));
    assert!(text.contains("color [hdr]"));
    assert_eq!(
        ResourceExecutionTrace::new().to_string(),
        "no encoders emitted\n"
    );
}

#[test]
fn the_kind_field_does_not_affect_comparison() {
    // Only declared attachments and order are compared, so a pass kind that
    // the encoders route differently is not itself a divergence.
    let resources = vec![resource("color")];
    let mut pass = graphics_pass("pass", vec![ResourceId(0)]);
    pass.kind = Some(PassKind::Ui);
    let mut trace = ResourceExecutionTrace::new();
    let mut e = entry(0, "pass", 0);
    e.color_targets = vec!["color".to_string()];
    trace.push(e);

    assert_eq!(
        compare_with_compiled(&resources, &[pass], &[0], &trace),
        vec![]
    );
}
