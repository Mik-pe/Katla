use std::{collections::HashMap, path::PathBuf, rc::Rc};

pub struct FileCache<T> {
    objects: HashMap<PathBuf, Rc<T>>,
}

impl<T> FileCache<T>
where
    T: From<PathBuf>,
{
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
        }
    }

    pub fn read(&mut self, path: PathBuf) -> Rc<T> {
        match self.objects.get(&path) {
            Some(model) => model.clone(),
            None => {
                let cached_object = Rc::new(T::from(path.clone()));
                self.objects.insert(path, cached_object.clone());
                cached_object
            }
        }
    }
}
