pub enum InputMapping {
    MoveForward = 0,
    MoveVertical,
    MoveHorizontal,
}

impl From<InputMapping> for u32 {
    fn from(val: InputMapping) -> Self {
        val as u32
    }
}
