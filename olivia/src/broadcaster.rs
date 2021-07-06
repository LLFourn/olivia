use olivia_core::{Path, PathRef, PrefixPath};
use std::collections::HashMap;
use tokio::sync::broadcast::*;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

#[derive(Debug)]
pub struct Broadcaster<T> {
    paths: HashMap<Path, Sender<T>>,
}

impl<T> Default for Broadcaster<T> {
    fn default() -> Self {
        Self {
            paths: HashMap::default(),
        }
    }
}

impl<T: Clone + PrefixPath + Send + 'static> Broadcaster<T> {
    pub fn subscribe_to(&mut self, path: Path) -> impl Stream<Item = T> + Send {
        let tx = self.paths.entry(path).or_insert_with(|| {
            let (tx, _) = channel(128);
            tx
        });

        BroadcastStream::new(tx.subscribe()).map(|res| res.unwrap())
    }

    pub fn process(&self, item_path: PathRef<'_>, item: T) {
        for (path, tx) in &self.paths {
            if path.as_path_ref().is_parent_of(item_path) {
                let _ = tx.send(item.clone().strip_prefix_path(path.as_path_ref()));
            }
        }
    }
}
