use std::any::Any;

#[derive(Default)]
pub struct ActionStream {
    actions: Vec<Box<dyn Any>>,
}

impl ActionStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit<A: 'static>(&mut self, action: A) {
        self.actions.push(Box::new(action));
    }

    pub fn drain<A: 'static>(&mut self) -> Vec<A> {
        let mut matched = Vec::new();
        let mut remaining = Vec::new();

        for action in self.actions.drain(..) {
            match action.downcast::<A>() {
                Ok(a) => matched.push(*a),
                Err(b) => remaining.push(b),
            }
        }

        self.actions = remaining;
        matched
    }
}
