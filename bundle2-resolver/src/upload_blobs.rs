// Copyright (c) 2004-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

use std::collections::HashMap;
use std::sync::Arc;

use futures::Stream;
use futures_ext::{BoxFuture, FutureExt};

use blobrepo::BlobRepo;
use mercurial_types::NodeHash;

use errors::*;

pub trait UploadableBlob {
    type Value: Send + 'static;

    fn upload(self, repo: &BlobRepo) -> Result<(NodeHash, Self::Value)>;
}

pub fn upload_blobs<S, B>(
    repo: Arc<BlobRepo>,
    blobs: S,
) -> BoxFuture<HashMap<NodeHash, B::Value>, Error>
where
    S: Stream<Item = B, Error = Error> + Send + 'static,
    B: UploadableBlob,
{
    blobs
        .fold(HashMap::new(), move |mut map, item| {
            let (key, value) = item.upload(&repo)?;
            ensure_msg!(
                map.insert(key, value).is_none(),
                "Blob already provided before"
            );
            Ok(map)
        })
        .boxify()
}