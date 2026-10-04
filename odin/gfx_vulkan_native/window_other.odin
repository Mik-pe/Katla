#+build !darwin
//! The window fixture requires a real Cocoa session; headless native acceptance stays portable.
package main

import gpu "../gfx/vulkan"

run_window :: proc(renderer:^gpu.Renderer) { panic("Cocoa window acceptance is unavailable on this platform") }
