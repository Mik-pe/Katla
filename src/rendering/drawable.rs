pub trait Drawable {
    fn draw(&self);

    //This function is expected to be called after data has been uploaded
    //Thus this entire function is marked unsafe
    unsafe fn rebind_gl(self) -> Self;
}
