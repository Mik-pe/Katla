use std::path::PathBuf;

/// A cache for loaded files that avoids reloading the same file multiple times.
///
/// Uses a loader function instead of `From<PathBuf>` to support fallible loading.
use std::{collections::HashMap, rc::Rc};

pub struct FileCache<T, F>
where
    F: Fn(&PathBuf) -> T,
{
    objects: HashMap<PathBuf, Rc<T>>,
    loader: F,
}

impl<T, F> FileCache<T, F>
where
    F: Fn(&PathBuf) -> T,
{
    /// Create a new cache with the given loader function.
    pub fn new(loader: F) -> Self {
        Self {
            objects: HashMap::new(),
            loader,
        }
    }

    /// Get a cached object or load it using the loader function.
    pub fn read(&mut self, path: PathBuf) -> Rc<T> {
        match self.objects.get(&path) {
            Some(file) => file.clone(),
            None => {
                let cached_object = Rc::new((self.loader)(&path));
                self.objects.insert(path, cached_object.clone());
                cached_object
            }
        }
    }
}
