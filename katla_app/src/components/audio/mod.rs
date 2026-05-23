use katla_ecs::Component;

#[derive(Component, Clone)]
pub struct AudioSource {
    pub path: String,
}

impl AudioSource {
    pub fn new(path: impl Into<String>) -> Self {
        AudioSource { path: path.into() }
    }
}

#[derive(Component, Clone, Default)]
pub struct AudioListener;

#[derive(Component, Clone)]
pub struct AudioEmitter {
    pub source_path: String,
    pub volume: f32,
    pub looping: bool,
    pub playing: bool,
}

impl AudioEmitter {
    pub fn new(source_path: impl Into<String>) -> Self {
        AudioEmitter {
            source_path: source_path.into(),
            volume: 1.0,
            looping: false,
            playing: true,
        }
    }
}
