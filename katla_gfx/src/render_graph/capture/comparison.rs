use super::*;

impl RenderGraphCapture {
    /// Finds native resource, encoder-order and synchronization divergences.
    pub fn compare_native(&self) -> Vec<String> {
        let mut failures = Vec::new();
        let native = &self.backend_execution;
        if native.backend.is_empty() {
            failures.push("native execution was not captured".into());
            return failures;
        }
        if !native.encoders.is_empty() && native.frame.is_none() {
            failures.push("native encoder trace has no frame/submission owner".into());
        }
        let expected: Vec<_> = self
            .executed_passes
            .iter()
            .filter(|pass| pass.outcome == "encoded")
            .map(|pass| pass.pass_index)
            .collect();
        let mut observed = Vec::new();
        for (position, encoder) in native.encoders.iter().enumerate() {
            if encoder.ordinal != position {
                failures.push(format!(
                    "native encoder ordinal {} differs from execution position {position}",
                    encoder.ordinal
                ));
            }
            if let Some(pass) = encoder.pass_index {
                if observed.last() != Some(&pass) {
                    observed.push(pass);
                }
                if !expected.contains(&pass) {
                    failures.push(format!(
                        "native encoder {} references unencoded pass {pass}",
                        encoder.ordinal
                    ));
                }
                let Some(declared) = self.graph.passes.iter().find(|record| record.index == pass)
                else {
                    continue;
                };
                let native_kind = match encoder.kind {
                    CapturedEncoderKind::Render => "render",
                    CapturedEncoderKind::Compute => "compute",
                    CapturedEncoderKind::Blit => "blit",
                };
                if declared.encoder.as_deref().is_some_and(|expected| {
                    expected != native_kind
                        && !(expected == "blit"
                            && native_kind == "compute"
                            && native.backend.to_ascii_lowercase().contains("metal"))
                }) {
                    failures.push(format!(
                        "native encoder {} pass {pass} uses {native_kind}, compiled {:?}",
                        encoder.ordinal, declared.encoder
                    ));
                }
                let declared_resources: Vec<_> = declared
                    .image_accesses
                    .iter()
                    .map(|access| access.resource.id)
                    .chain(
                        declared
                            .buffer_accesses
                            .iter()
                            .map(|access| access.resource.id),
                    )
                    .collect();
                if encoder.kind == CapturedEncoderKind::Render
                    && let Some(executed) = self
                        .executed_passes
                        .iter()
                        .find(|record| record.pass_index == pass)
                {
                    for target in executed
                        .color_targets
                        .iter()
                        .chain(executed.depth_target.iter())
                    {
                        if let Some(resource) = self
                            .graph
                            .resources
                            .iter()
                            .find(|resource| &resource.name == target)
                            && !encoder.resources.contains(&resource.id)
                        {
                            failures.push(format!("native encoder {} pass {pass} did not bind attachment r{} ({target})",
                                encoder.ordinal, resource.id));
                        }
                    }
                }
                for resource in &encoder.resources {
                    if !declared_resources.contains(resource) {
                        failures.push(format!(
                            "native encoder {} pass {pass} bound undeclared resource r{resource}",
                            encoder.ordinal
                        ));
                    }
                }
            }
        }
        if observed != expected {
            failures.push(format!(
                "native pass order differs: compiled {expected:?}, emitted {observed:?}"
            ));
        }
        let mut matched = vec![false; native.synchronization.len()];
        for operation in &self.planned_synchronization {
            if let Some(index) =
                native
                    .synchronization
                    .iter()
                    .enumerate()
                    .position(|(index, actual)| {
                        !matched[index]
                            && actual.origin == "compiled"
                            && operation.contract() == actual.contract()
                    })
            {
                matched[index] = true;
            } else {
                failures.push(format!(
                    "missing native synchronization coverage: {} r{} {:?} -> {:?} {}",
                    operation.resource_kind,
                    operation.resource,
                    operation.producer,
                    operation.consumer,
                    operation.range
                ));
            }
        }
        for (index, actual) in native.synchronization.iter().enumerate() {
            if actual.origin == "compiled" && !matched[index] {
                failures.push(format!(
                    "unexpected native synchronization: {} r{} {:?} -> {:?} {}",
                    actual.resource_kind,
                    actual.resource,
                    actual.producer,
                    actual.consumer,
                    actual.range
                ));
            }
            if actual.emitted
                && (actual.native_scope.is_empty()
                    || actual.required_native_scope != actual.native_scope)
            {
                failures.push(format!(
                    "SyncScopeMismatch: {} r{} {:?} -> {:?}: required {:?}, emitted {:?}",
                    actual.resource_kind,
                    actual.resource,
                    actual.producer,
                    actual.consumer,
                    actual.required_native_scope,
                    actual.native_scope
                ));
            }
            if !actual.emitted && actual.omission_reason.as_deref().is_none_or(str::is_empty) {
                failures.push(format!(
                    "r{} synchronization omitted without a reason",
                    actual.resource
                ));
            }
        }
        failures
    }
}
