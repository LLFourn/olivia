use std::sync::Arc;

use crate::db::*;
use olivia_core::{Path, PrefixPath};

#[derive(Clone)]
pub struct PrefixedDb {
    inner: Arc<dyn DbReadEvent>,
    prefix: Path,
}

impl PrefixedDb {
    pub fn new(db: Arc<dyn DbReadEvent>, prefix: Path) -> Self {
        Self { inner: db, prefix }
    }
}

#[async_trait]
impl DbReadEvent for PrefixedDb {
    async fn get_node(&self, _path: PathRef<'_>) -> anyhow::Result<Option<GetPath>> {
        unimplemented!("this shouldn't be needed");
    }

    async fn query_event(&self, mut query: EventQuery<'_, '_>) -> anyhow::Result<Option<Event>> {
        let path = query.path.unwrap_or(PathRef::root()).to_path();
        let prefixed_path = path.prefix_path(self.prefix.as_path_ref());
        query.path = Some(prefixed_path.as_path_ref());
        self.inner
            .query_event(query)
            .await
            .map(|x| x.map(|x| x.strip_prefix_path(self.prefix.as_path_ref())))
    }

    async fn query_events(&self, mut query: EventQuery<'_, '_>) -> anyhow::Result<Vec<Event>> {
        let path = query.path.unwrap_or(PathRef::root()).to_path();
        let prefixed_path = path.prefix_path(self.prefix.as_path_ref());
        query.path = Some(prefixed_path.as_path_ref());
        self.inner.query_events(query).await.map(|x| {
            x.into_iter()
                .map(|x| x.strip_prefix_path(self.prefix.as_path_ref()))
                .collect()
        })
    }
}
