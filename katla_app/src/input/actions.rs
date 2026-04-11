#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    MoveForward = 0,
    MoveBackward = 1,
    MoveLeft = 2,
    MoveRight = 3,
    MoveUp = 4,
    MoveDown = 5,
    Jump = 6,
    Interact = 7,
    Inventory = 8,
    Pause = 9,
    Exit = 10,

    LookEnable = 11,
    Sprint = 12,
    PanEnable = 13,
    Slow = 14,
}

impl Action {
    /// Total number of action slots. Padded to 16 (1 spare) so the
    /// `keyboard_keys` array in `InputState` stays power-of-two sized.
    pub const COUNT: usize = 16;
}
