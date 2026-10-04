//! One display transform follows the complete linear HDR scene.
package render

import gfx "../../gfx"
import "core:math"

Tonemap_Operator :: enum { ACES, Reinhard, Tony_McMapface, Linear }
Postprocess_Settings :: struct { exposure:f32, operator:Tonemap_Operator }
postprocess_default :: proc()->Postprocess_Settings { return {1,.ACES} }
postprocess_valid :: proc(settings:Postprocess_Settings)->bool { return !math.is_nan(settings.exposure) && !math.is_inf(settings.exposure) && settings.exposure>=0 && settings.operator>=.ACES && settings.operator<=.Linear }
/// All scene raster binaries are prepared before native scene publication.
Scene_Pipelines :: struct { surface,postprocess:gfx.Graphics_Desc, output_format:gfx.Texture_Format, features:Feature_Descriptors }

/// Editor features are authored app settings, independent of backend policy.
Feature_Settings :: struct {
    sky,grid,shadows,outline,wallhack:bool,
    ground_height:f32,
    shadow_size:u32,
    postprocess:Postprocess_Settings,
}
FEATURE_SETTINGS_DEFAULT :: Feature_Settings{sky=true,grid=true,shadows=true,outline=true,wallhack=true,shadow_size=2048,postprocess={1,.ACES}}
feature_settings_default :: proc()->Feature_Settings { return FEATURE_SETTINGS_DEFAULT }
