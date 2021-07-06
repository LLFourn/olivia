use std::sync::Arc;

use olivia_core::{Path, PrefixPath};
use crate::db::*;

pub struct PrefixedDb {
    inner: Arc<dyn DbReadEvent>,
    prefix: Path,
}

impl PrefixedDb {
    pub fn new(db: Arc<dyn DbReadEvent>, prefix: Path) -> Self {
        Self {
            inner: db,
            prefix
        }
    }
}

#[async_trait]
impl DbReadEvent for PrefixedDb {

    async fn get_node(&self, _path: PathRef<'_>) -> anyhow::Result<Option<PathNode>> {
        unimplemented!("this shouldn't be needed");
    }

    async fn latest_child_event(
        &self,
        path: PathRef<'_>,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let path = path.to_path().prefix_path(self.prefix.as_path_ref());
        self.inner.latest_child_event(path.as_path_ref(), kind).await.map(|x| x.map(|x| x.strip_prefix_path(self.prefix.as_path_ref())))
    }

    async fn earliest_unattested_child_event(
        &self,
        path: PathRef<'_>,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let path = path.to_path().prefix_path(self.prefix.as_path_ref());
        self.inner.earliest_unattested_child_event(path.as_path_ref(), kind).await.map(|x| x.map(|x| x.strip_prefix_path(self.prefix.as_path_ref())))
    }
}
