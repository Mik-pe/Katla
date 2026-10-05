//! Compact audio bus controls synchronized with editor preferences.
use std::boxed::Box;

use katla_audio::{LevelsSnapshot, linear_to_db};
use katla_math::Rect2D;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Widget, WidgetBox, empty, hstack, labeled_slider, text, vstack,
    vu_meter,
};

use crate::Preferences;

use super::super::types::PreferencesAction;

#[derive(Clone)]
pub(crate) struct MixerDrawCtx {
    pub bounds: Rect2D,
    pub levels: LevelsSnapshot,
    pub active_voices: usize,
    pub peak_voices: usize,
    pub preferences: Preferences,
    pub theme: katla_ui::ColorScheme,
}

fn clamp_db(db: f32) -> f32 {
    db.max(-60.0)
}

pub(crate) struct MixerView;

impl Build for MixerView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        // Always reserve state slots in the same order regardless of whether
        // the env is set, so that subsequent sibling views don't get their
        // StateId slots shifted when this view becomes active/inactive.
        let master_id = ctx.state(0.0f32);
        let sfx_id = ctx.state(0.0f32);
        let music_id = ctx.state(0.0f32);
        let ambient_id = ctx.state(0.0f32);

        let baseline_id = ctx.state(None::<[f32; 4]>);
        let scroll_id = ctx.state(0.0f32);
        let Some(draw_ctx) = ctx.env::<MixerDrawCtx>().cloned() else {
            return empty().boxed();
        };
        let actual = [
            draw_ctx.preferences.audio.master_volume,
            draw_ctx.preferences.audio.sfx_volume,
            draw_ctx.preferences.audio.music_volume,
            draw_ctx.preferences.audio.ambient_volume,
        ];
        let ids = [master_id, sfx_id, music_id, ambient_id];
        if let Some(previous) = ctx.get_state::<Option<[f32; 4]>>(baseline_id).flatten() {
            for (index, id) in ids.iter().enumerate() {
                let edited = ctx.get_state::<f32>(*id).unwrap_or(actual[index]);
                if edited != previous[index] {
                    log::debug!("Mixer bus {index} changed: {} -> {edited}", previous[index]);
                    ctx.emit(match index {
                        0 => PreferencesAction::SetMasterVolume(edited),
                        1 => PreferencesAction::SetSfxVolume(edited),
                        2 => PreferencesAction::SetMusicVolume(edited),
                        _ => PreferencesAction::SetAmbientVolume(edited),
                    });
                }
            }
        }
        for (id, value) in ids.iter().zip(actual) {
            ctx.set_state(*id, value);
        }
        ctx.set_state(baseline_id, Some(actual));
        let theme = &draw_ctx.theme;
        let voice_status = text(format!(
            "Voices: {}/{} (peak: {})",
            draw_ctx.active_voices,
            katla_audio::MAX_VOICES,
            draw_ctx.peak_voices
        ))
        .font_size(katla_ui::FontSize::Small)
        .color(theme.text_secondary)
        .boxed();
        let levels = [
            &draw_ctx.levels.master,
            &draw_ctx.levels.sfx,
            &draw_ctx.levels.music,
            &draw_ctx.levels.ambient,
        ];
        let width = ((draw_ctx.bounds.width() - 40.0 - 48.0) / 4.0).max(110.0);
        let height = (draw_ctx.bounds.height() - 76.0).clamp(40.0, 80.0);
        let buses = ["Master", "SFX", "Music", "Ambient"]
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                vstack([
                    labeled_slider(label, ids[i], 0.0..=1.0)
                        .label_width(56.0)
                        .flex_width(width)
                        .show_value(true)
                        .boxed(),
                    vu_meter(
                        clamp_db(linear_to_db(levels[i].peak)),
                        clamp_db(linear_to_db(levels[i].rms)),
                    )
                    .flex_height(height)
                    .boxed(),
                ])
                .spacing(4.0)
                .align(Alignment::Center)
                .flex_width(width)
                .boxed()
            });
        let bus_row = hstack(buses)
            .spacing(16.0)
            .padding_all(12.0)
            .align(Alignment::Center);
        katla_ui::declarative::scroll(
            vstack([voice_status, bus_row.boxed()])
                .spacing(4.0)
                .padding_all(8.0)
                .align(Alignment::Leading)
                .flex_width(draw_ctx.bounds.width())
                .boxed(),
            scroll_id,
        )
        .flex_width(draw_ctx.bounds.width())
        .flex_height(draw_ctx.bounds.height())
        .boxed()
    }
}
