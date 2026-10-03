use std::boxed::Box;

use katla_audio::{LevelsSnapshot, linear_to_db};
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Widget, WidgetBox, empty, grid, hstack, labeled_slider,
    panel_body, scroll, text, vstack, vu_meter,
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
        let scroll_id = ctx.state(0.0f32);
        let baseline_id = ctx.state(None::<[f32; 4]>);

        let draw_ctx = ctx.env::<MixerDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let preferences = [
            draw_ctx.preferences.audio.master_volume,
            draw_ctx.preferences.audio.sfx_volume,
            draw_ctx.preferences.audio.music_volume,
            draw_ctx.preferences.audio.ambient_volume,
        ];
        let ids = [master_id, sfx_id, music_id, ambient_id];
        if ctx.get_state(baseline_id) != Some(Some(preferences)) {
            for (id, value) in ids.into_iter().zip(preferences) {
                ctx.set_state(id, value);
            }
        }
        ctx.set_state(baseline_id, Some(preferences));
        let [pref_master, pref_sfx, pref_music, pref_ambient] = preferences;

        let theme = &draw_ctx.theme;
        let levels = &draw_ctx.levels;

        let voice_status = text(format!(
            "Voices: {}/{} (peak: {})",
            draw_ctx.active_voices,
            katla_audio::MAX_VOICES,
            draw_ctx.peak_voices
        ))
        .color(theme.text_secondary)
        .boxed();

        let columns = (((draw_ctx.bounds.width() - 24.0 + 12.0) / 260.0) as usize).clamp(1, 4);
        let cell_width = ((draw_ctx.bounds.width() - 24.0 - 12.0 * (columns - 1) as f32)
            / columns as f32)
            .max(1.0);
        let strips = [
            ("Master", master_id, &levels.master, pref_master),
            ("Sound effects", sfx_id, &levels.sfx, pref_sfx),
            ("Music", music_id, &levels.music, pref_music),
            ("Ambient", ambient_id, &levels.ambient, pref_ambient),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (label, id, level, preference))| {
            let current = ctx.get_state::<f32>(id).unwrap_or(preference);
            if (current - preference).abs() > 1e-4 {
                ctx.emit(match index {
                    0 => PreferencesAction::SetMasterVolume(current),
                    1 => PreferencesAction::SetSfxVolume(current),
                    2 => PreferencesAction::SetMusicVolume(current),
                    _ => PreferencesAction::SetAmbientVolume(current),
                });
            }
            hstack([
                vstack([
                    text(label).color(theme.text_secondary).boxed(),
                    labeled_slider("", id, 0.0..=1.0)
                        .label_width(0.0)
                        .show_value(true)
                        .precision(0)
                        .value_display(100.0, "%")
                        .boxed(),
                ])
                .spacing(12.0)
                .flex_grow(1.0)
                .boxed(),
                vu_meter(
                    clamp_db(linear_to_db(level.peak)),
                    clamp_db(linear_to_db(level.rms)),
                )
                .boxed(),
            ])
            .spacing(16.0)
            .padding_all(12.0)
            .align(Alignment::Middle)
            .flex_width(cell_width)
            .boxed()
        })
        .collect::<Vec<_>>();

        let content = vstack([
            voice_status,
            grid(columns, Vec2::new(cell_width, 144.0), strips)
                .grid_spacing(12.0)
                .boxed(),
        ])
        .spacing(12.0)
        .padding_all(12.0)
        .flex_shrink(0.0)
        .boxed();
        panel_body(scroll(content, scroll_id).flex_grow(1.0).boxed())
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}
