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

        let draw_ctx = ctx.env::<MixerDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        // Sync preference values into state when the mixer becomes active.
        // get_or_create initialized these to 0.0; overwrite with actual prefs.
        let pref_master = draw_ctx.preferences.audio.master_volume;
        let pref_sfx = draw_ctx.preferences.audio.sfx_volume;
        let pref_music = draw_ctx.preferences.audio.music_volume;
        let pref_ambient = draw_ctx.preferences.audio.ambient_volume;
        if (ctx.get_state::<f32>(master_id).unwrap_or(0.0) - pref_master).abs() > 1e-4
            && ctx.get_state::<f32>(master_id).unwrap_or(-1.0).abs() < 1e-6
        {
            ctx.set_state(master_id, pref_master);
        }
        if (ctx.get_state::<f32>(sfx_id).unwrap_or(0.0) - pref_sfx).abs() > 1e-4
            && ctx.get_state::<f32>(sfx_id).unwrap_or(-1.0).abs() < 1e-6
        {
            ctx.set_state(sfx_id, pref_sfx);
        }
        if (ctx.get_state::<f32>(music_id).unwrap_or(0.0) - pref_music).abs() > 1e-4
            && ctx.get_state::<f32>(music_id).unwrap_or(-1.0).abs() < 1e-6
        {
            ctx.set_state(music_id, pref_music);
        }
        if (ctx.get_state::<f32>(ambient_id).unwrap_or(0.0) - pref_ambient).abs() > 1e-4
            && ctx.get_state::<f32>(ambient_id).unwrap_or(-1.0).abs() < 1e-6
        {
            ctx.set_state(ambient_id, pref_ambient);
        }

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

        let master_db_peak = clamp_db(linear_to_db(levels.master.peak));
        let master_db_rms = clamp_db(linear_to_db(levels.master.rms));
        let current_master: f32 = ctx.get_state(master_id).unwrap();
        if (current_master - draw_ctx.preferences.audio.master_volume).abs() > 1e-4 {
            ctx.emit(PreferencesAction::SetMasterVolume(current_master));
        }
        let master_fader = labeled_slider("Master", master_id, 0.0..=1.0).show_value(true);
        let master_meter = vu_meter(master_db_peak, master_db_rms);

        let sfx_db_peak = clamp_db(linear_to_db(levels.sfx.peak));
        let sfx_db_rms = clamp_db(linear_to_db(levels.sfx.rms));
        let current_sfx: f32 = ctx.get_state(sfx_id).unwrap();
        if (current_sfx - draw_ctx.preferences.audio.sfx_volume).abs() > 1e-4 {
            ctx.emit(PreferencesAction::SetSfxVolume(current_sfx));
        }
        let sfx_fader = labeled_slider("SFX", sfx_id, 0.0..=1.0).show_value(true);
        let sfx_meter = vu_meter(sfx_db_peak, sfx_db_rms);

        let music_db_peak = clamp_db(linear_to_db(levels.music.peak));
        let music_db_rms = clamp_db(linear_to_db(levels.music.rms));
        let current_music: f32 = ctx.get_state(music_id).unwrap();
        if (current_music - draw_ctx.preferences.audio.music_volume).abs() > 1e-4 {
            ctx.emit(PreferencesAction::SetMusicVolume(current_music));
        }
        let music_fader = labeled_slider("Music", music_id, 0.0..=1.0).show_value(true);
        let music_meter = vu_meter(music_db_peak, music_db_rms);

        let ambient_db_peak = clamp_db(linear_to_db(levels.ambient.peak));
        let ambient_db_rms = clamp_db(linear_to_db(levels.ambient.rms));
        let current_ambient: f32 = ctx.get_state(ambient_id).unwrap();
        if (current_ambient - draw_ctx.preferences.audio.ambient_volume).abs() > 1e-4 {
            ctx.emit(PreferencesAction::SetAmbientVolume(current_ambient));
        }
        let ambient_fader = labeled_slider("Ambient", ambient_id, 0.0..=1.0).show_value(true);
        let ambient_meter = vu_meter(ambient_db_peak, ambient_db_rms);

        let bus_row = hstack([
            vstack([master_fader.boxed(), master_meter.boxed()])
                .spacing(2.0)
                .align(Alignment::Center)
                .boxed(),
            vstack([sfx_fader.boxed(), sfx_meter.boxed()])
                .spacing(2.0)
                .align(Alignment::Center)
                .boxed(),
            vstack([music_fader.boxed(), music_meter.boxed()])
                .spacing(2.0)
                .align(Alignment::Center)
                .boxed(),
            vstack([ambient_fader.boxed(), ambient_meter.boxed()])
                .spacing(2.0)
                .align(Alignment::Center)
                .boxed(),
        ])
        .spacing(16.0)
        .padding_all(12.0)
        .align(Alignment::Center);

        vstack([voice_status, bus_row.boxed()])
            .spacing(4.0)
            .padding_all(8.0)
            .align(Alignment::Leading)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}
