use std::path::PathBuf;

/// A cache for loaded files that avoids reloading the same file multiple times.
///
/// Uses a loader function that returns `Result<T, E>` to support fallible loading.
use std::{collections::HashMap, rc::Rc};

pub struct FileCache<T, E, F>
where
    F: Fn(&PathBuf) -> Result<T, E>,
{
    objects: HashMap<PathBuf, Rc<T>>,
    loader: F,
}

impl<T, E, F> FileCache<T, E, F>
where
    F: Fn(&PathBuf) -> Result<T, E>,
{
    /// Create a new cache with the given loader function.
    pub fn new(loader: F) -> Self {
        Self {
            objects: HashMap::new(),
            loader,
        }
    }

    /// Get a cached object or load it using the loader function.
    pub fn read(&mut self, path: PathBuf) -> Result<Rc<T>, E> {
        match self.objects.get(&path) {
            Some(file) => Ok(file.clone()),
            None => {
                let cached_object = Rc::new((self.loader)(&path)?);
                self.objects.insert(path, cached_object.clone());
                Ok(cached_object)
            }
        }
    }
}
