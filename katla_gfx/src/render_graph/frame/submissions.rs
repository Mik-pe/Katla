//! Shared validation of frame inputs before either backend encodes commands.

use super::PassExecutionData;
use crate::render_graph::{PassKind, PassType, RenderGraphError};

impl PassExecutionData {
    pub(crate) fn validate(
        &self,
        name: &str,
        pass_type: PassType,
        kind: PassKind,
    ) -> Result<(), RenderGraphError> {
        let invalid = |reason| RenderGraphError::InvalidConfiguration(reason);
        if pass_type != PassType::Graphics
            && (!self.draw_lists.is_empty() || !self.ui_draw_lists.is_empty())
        {
            return Err(invalid(format!(
                "Pass '{name}' cannot accept graphics submissions"
            )));
        }
        if !self.draw_lists.is_empty() && kind == PassKind::Ui {
            return Err(invalid(format!(
                "Mesh draw lists cannot be submitted to UI pass '{name}'"
            )));
        }
        if !self.ui_draw_lists.is_empty() && kind != PassKind::Ui {
            return Err(invalid(format!(
                "UI draw lists require a UI pass ('{name}')"
            )));
        }
        if self.ui_draw_lists.len() > 1 {
            return Err(invalid(format!(
                "UI pass '{name}' received {} UI draw lists; submit one composed list per pass",
                self.ui_draw_lists.len()
            )));
        }
        Ok(())
    }
}
