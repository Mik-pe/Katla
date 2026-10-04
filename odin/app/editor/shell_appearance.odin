//! Application appearance presets change retained paint, shaped text and control geometry together.
package editor_app
import ui "../../ui"
import "core:fmt"

@(private="package")
appearance_color :: proc(rgb:u32)->ui.Color { return {f32(rgb>>16&255)/255,f32(rgb>>8&255)/255,f32(rgb&255)/255,1} }
@(private="package")
shell_appearance :: proc(shell:^Shell) {
    theme:=ui.theme_default(shell.ctx.theme.font)
    if shell.preferences==nil { shell.ctx.theme=theme; return }
    palette:=[10]u32{0x1C1C1E,0x2A2A2A,0x3A3A3A,0x48484A,0x58585A,0xFFFFFF,0x8E8E93,0x6E6E73,0xF79545,0xF79545}
    switch shell.preferences.theme {
    case "dark":palette={0x2B2B2B,0x2B2B2B,0x4A4A4A,0x5A5A5A,0x6A6A6A,0xEEEEEE,0xBBBBBB,0x777777,0x4EC9B0,0x3465A4}
    case "light":palette={0xF0F0F0,0xFFFFFF,0xE0E0E0,0xD0D0D0,0xC0C0C0,0x222222,0x555555,0x808080,0x2070D0,0x2070D0}
    case "nord": palette={0x2E3440,0x2E3440,0x3B4252,0x434C5E,0x81A1C1,0xECEFF4,0xE5E9F0,0xD8DEE9,0xA3BE8C,0x81A1C1}
    case "tokyo_night": palette={0x1A1B26,0x1A1B26,0x242533,0x3B3E4D,0x7AA2F7,0xC0CAF5,0xA9B1D6,0x565F89,0x9ECE6A,0x364A8E}
    case "dracula": palette={0x282A36,0x282A36,0x44475A,0x52576C,0xBD93F9,0xF8F8F2,0xE5E5E5,0x6272A4,0x50FA7B,0xBD93F9}
    case "gruvbox": palette={0x282828,0x282828,0x3C3836,0x504A45,0xD79921,0xEBDBB2,0xD5C4A1,0x928374,0xB8BB26,0xD79921}
    case "one_dark": palette={0x282C34,0x282C34,0x3E4451,0x4B5263,0x61AFEF,0xABB2BF,0x9DA5B4,0x5C6370,0x98C379,0x3E4451}
    case "material_palenight": palette={0x292D3E,0x292D3E,0x3A3F5B,0x414763,0x82AAFF,0xA6ACCD,0x8A93B5,0x676E95,0xC3E88D,0x676E95}
    case "ayu_dark": palette={0x0D1017,0x0D1017,0x1A1F29,0x2D3440,0x39BAE6,0xBFBDB6,0xA8A49D,0x5C6773,0xBED9F5,0x1A1F29}
    case "github_dark": palette={0x0D1117,0x0D1117,0x21262D,0x30363D,0x1F6FEB,0xE6EDF3,0xC9D1D9,0x7D8590,0x3FB950,0x1F6FEB}
    case "monokai": palette={0x272822,0x272822,0x3E3D32,0x49483E,0x66D9EF,0xF8F8F2,0xCFCFC2,0x75715E,0xA6E22E,0x49483E}
    case "rose_pine": palette={0x191724,0x191724,0x1F1D2E,0x26233A,0xC4A7E7,0xE0DEF4,0xC9C8D3,0x6E6A86,0x9CCFD8,0x403D52}
    case "kanagawa": palette={0x1F1F28,0x1F1F28,0x2A2A3C,0x363646,0x7E9CD8,0xDCD7BA,0xC8C093,0x727169,0x76946A,0x2D4F67}
    case "solarized_dark": palette={0x002B36,0x002B36,0x073642,0x094959,0x268BD2,0x839496,0x657B83,0x586E75,0x859900,0x073642}
    }
    theme.canvas=appearance_color(palette[0]); theme.panel=appearance_color(palette[1]); theme.control=appearance_color(palette[2]); theme.hover=appearance_color(palette[3]); theme.active=appearance_color(palette[4]); theme.text=appearance_color(palette[5]); theme.muted=appearance_color(palette[6]); theme.disabled=appearance_color(palette[7]); theme.accent=appearance_color(palette[8]); theme.selection=appearance_color(palette[9])
    scale:=shell.preferences.font_scale
    theme.font_size*=scale; theme.row_height*=scale; theme.padding*=scale; theme.radius*=scale
    shell.ctx.theme=theme
}
@(private="package")
shell_scale :: proc(shell:^Shell)->f32 { return shell.preferences.font_scale if shell.preferences!=nil else 1 }
@(private="package")
scale_descriptor :: proc(descriptor:^ui.Descriptor,scale:f32) {
    if descriptor.layout.height.kind==.Pixels { descriptor.layout.no_shrink=true }
    if descriptor.layout.width.kind==.Pixels { descriptor.layout.width.value*=scale }
    if descriptor.layout.height.kind==.Pixels { descriptor.layout.height.value*=scale }
    descriptor.layout.padding.top*=scale; descriptor.layout.padding.right*=scale; descriptor.layout.padding.bottom*=scale; descriptor.layout.padding.left*=scale
    descriptor.layout.gap*=scale
    if descriptor.font_size>0 { descriptor.font_size*=scale }
    for &child in descriptor.children { scale_descriptor(&child,scale) }
}
/// Performance values come from the actual accepted owner frame, independently of the active dock panel.
shell_frame_statistics :: proc(shell:^Shell,seconds:f32,passes:int,serial:u64) { shell.frame_seconds=seconds; shell.frame_passes=passes; shell.frame_serial=serial }
@(private="package")
shell_status :: proc(shell:^Shell,mode:string)->string {
    if shell.state.last_error!=.None { label:=fmt.aprintf("%s · Last action: %v",mode,shell.state.last_error,allocator=shell.allocator); append(&shell.texts,label); return label }
    if shell.preferences==nil || !shell.preferences.show_stats || shell.frame_serial==0 { return mode }
    label:=fmt.aprintf("%s · %.1f ms · %.0f fps · %d passes · frame %d",mode,shell.frame_seconds*1000,1/max(.000001,shell.frame_seconds),shell.frame_passes,shell.frame_serial,allocator=shell.allocator)
    append(&shell.texts,label); return label
}
