use super::*;
use std::fmt::{self, Write as _};

impl RenderGraphCapture {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Writes the joined bundle and separate plan/execution artifacts for a failing check.
    pub fn write_artifacts(&self, directory: &std::path::Path, label: &str) -> std::io::Result<()> {
        let label: String = label
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        std::fs::create_dir_all(directory)?;
        let write_json = |suffix: &str, value: serde_json::Value| -> std::io::Result<()> {
            let json = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
            std::fs::write(
                directory.join(format!("{label}.{suffix}.json")),
                format!("{json}\n"),
            )
        };
        write_json(
            "capture",
            serde_json::to_value(self).map_err(std::io::Error::other)?,
        )?;
        write_json(
            "plan",
            serde_json::to_value(&self.graph).map_err(std::io::Error::other)?,
        )?;
        write_json(
            "execution",
            serde_json::to_value(&self.backend_execution).map_err(std::io::Error::other)?,
        )?;
        std::fs::write(directory.join(format!("{label}.text")), self.to_string())?;
        std::fs::write(directory.join(format!("{label}.dot")), self.to_dot())
    }

    /// Extends the logical/physical DOT graph with separate native execution nodes.
    pub fn to_dot(&self) -> String {
        let mut output = self.graph.to_dot();
        if let Some(end) = output.rfind('}') {
            output.truncate(end);
        }
        for encoder in &self.backend_execution.encoders {
            let label = encoder
                .label
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            let _ = writeln!(
                output,
                "  backend_encoder_{} [shape=hexagon, color=purple, label=\"native {:?} {}\\n{}\"];",
                encoder.ordinal, encoder.kind, encoder.ordinal, label
            );
            if let Some(pass) = encoder.pass_index {
                let _ = writeln!(
                    output,
                    "  p{pass} -> backend_encoder_{} [style=dashed, color=purple, label=\"emitted\"];",
                    encoder.ordinal
                );
            }
            for resource in &encoder.resources {
                let _ = writeln!(
                    output,
                    "  r{resource} -> backend_encoder_{} [color=purple, label=\"bound\"];",
                    encoder.ordinal
                );
            }
        }
        output.push_str("}\n");
        output
    }
}

impl fmt::Display for RenderGraphCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.graph)?;
        writeln!(f, "\nBackend execution: {}", self.backend_execution.backend)?;
        if let Some(owner) = &self.backend_execution.frame {
            writeln!(
                f,
                "  slot {} generation {} allocator {} feedback {} {:?}",
                owner.frame_slot,
                owner.generation,
                owner.command_allocator,
                owner.feedback_identity,
                owner.feedback
            )?;
        }
        for encoder in &self.backend_execution.encoders {
            writeln!(
                f,
                "  native encoder {} {:?} pass {:?} ({}) resources {:?}",
                encoder.ordinal, encoder.kind, encoder.pass_index, encoder.label, encoder.resources
            )?;
        }
        for operation in &self.backend_execution.synchronization {
            writeln!(
                f,
                "  native sync {} {} {} {:?} -> {:?} {}: {} -> {} ({}, emitted={}, omission={:?})",
                operation.boundary,
                operation.version,
                operation.range,
                operation.producer,
                operation.consumer,
                operation.resource_kind,
                operation.source,
                operation.destination,
                operation.reason,
                operation.emitted,
                operation.omission_reason
            )?;
            for scope in &operation.native_scope {
                writeln!(f, "      native scope: {scope:?}")?;
            }
        }
        for binding in &self.backend_execution.bindings {
            writeln!(
                f,
                "  native binding {} layout {} snapshot {:?} residency {:?}",
                binding.identity,
                binding.layout_identity,
                binding.snapshot_generation,
                binding.residency_members
            )?;
        }
        if self.comparison.is_empty() {
            writeln!(f, "  compiled/native comparison: matched")
        } else {
            for failure in &self.comparison {
                writeln!(f, "  divergence: {failure}")?;
            }
            Ok(())
        }
    }
}
