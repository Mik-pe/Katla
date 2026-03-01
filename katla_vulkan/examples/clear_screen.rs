use ash::vk;
use katla_vulkan::render_graph::*;

fn main() {
    // Simple test that types are accessible
    println!("Render pass types available");

    // Create a simple attachment to verify API
    let _attachment = Attachment::clear_color(vk::ImageView::null(), [0.0, 0.0, 0.0, 1.0]);

    println!("clear_screen example compiled successfully");
}
