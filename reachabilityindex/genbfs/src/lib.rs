// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

use std::collections::HashSet;
use std::sync::Arc;

use cloned::cloned;
use context::CoreContext;
use failure_ext::Error;
use futures::future::{loop_fn, ok, Future, Loop};
use futures::stream::{iter_ok, Stream};
use futures_ext::{BoxFuture, FutureExt};

use changeset_fetcher::ChangesetFetcher;
use mononoke_types::{ChangesetId, Generation};

use common::*;
use reachabilityindex::ReachabilityIndex;

pub struct GenerationNumberBFS {}

impl GenerationNumberBFS {
    pub fn new() -> Self {
        GenerationNumberBFS {}
    }
}

// Take ownership of two sets, the current 'layer' of the bfs, and all nodes seen until then.
// Produce a future which does the following computation:
// - add all nodes in the current layer to the seen set
// - get the set of parents of nodes in the current layer
// - filter out previously seen nodes from the parents
// - filter out nodes whose generation number is too large
// - return the parents as the next bfs layer, and the updated seen as the new seen set
fn process_bfs_layer(
    ctx: CoreContext,
    changeset_fetcher: Arc<ChangesetFetcher>,
    curr_layer: HashSet<ChangesetId>,
    mut curr_seen: HashSet<ChangesetId>,
    dst_gen: Generation,
) -> BoxFuture<(HashSet<ChangesetId>, HashSet<ChangesetId>), Error> {
    let new_changeset_fetcher = changeset_fetcher.clone();
    for next_node in curr_layer.iter() {
        curr_seen.insert(next_node.clone());
    }

    iter_ok::<_, Error>(curr_layer)
        .and_then({
            cloned!(ctx);
            move |hash| new_changeset_fetcher.get_parents(ctx.clone(), hash)
        })
        .map(|parents| iter_ok::<_, Error>(parents.into_iter()))
        .flatten()
        .collect()
        .and_then(|all_parents| {
            changesets_with_generation_numbers(ctx, changeset_fetcher, all_parents)
        })
        .map(move |flattened_node_generation_pairs| {
            let mut next_layer = HashSet::new();
            for (parent_hash, parent_gen) in flattened_node_generation_pairs.into_iter() {
                if !curr_seen.contains(&parent_hash) && parent_gen >= dst_gen {
                    next_layer.insert(parent_hash);
                }
            }
            (next_layer, curr_seen)
        })
        .boxify()
}

impl ReachabilityIndex for GenerationNumberBFS {
    fn query_reachability(
        &self,
        ctx: CoreContext,
        changeset_fetcher: Arc<ChangesetFetcher>,
        src: ChangesetId,
        dst: ChangesetId,
    ) -> BoxFuture<bool, Error> {
        let start_bfs_layer: HashSet<_> = vec![src].into_iter().collect();
        let start_seen: HashSet<_> = HashSet::new();
        check_if_node_exists(ctx.clone(), changeset_fetcher.clone(), src.clone())
            .and_then(move |_| {
                fetch_generation(ctx.clone(), changeset_fetcher.clone(), dst.clone()).and_then(
                    move |dst_gen| {
                        loop_fn(
                            (start_bfs_layer, start_seen),
                            move |(curr_layer, curr_seen)| {
                                if curr_layer.contains(&dst) {
                                    ok(Loop::Break(true)).boxify()
                                } else if curr_layer.is_empty() {
                                    ok(Loop::Break(false)).boxify()
                                } else {
                                    process_bfs_layer(
                                        ctx.clone(),
                                        changeset_fetcher.clone(),
                                        curr_layer,
                                        curr_seen,
                                        dst_gen,
                                    )
                                    .map(move |(next_layer, next_seen)| {
                                        Loop::Continue((next_layer, next_seen))
                                    })
                                    .boxify()
                                }
                            },
                        )
                    },
                )
            })
            .from_err()
            .boxify()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_helpers::test_branch_wide_reachability;
    use test_helpers::test_linear_reachability;
    use test_helpers::test_merge_uneven_reachability;

    #[test]
    fn linear_reachability() {
        let bfs_constructor = || GenerationNumberBFS::new();
        test_linear_reachability(bfs_constructor);
    }

    #[test]
    fn merge_uneven_reachability() {
        let bfs_constructor = || GenerationNumberBFS::new();
        test_merge_uneven_reachability(bfs_constructor);
    }

    #[test]
    fn branch_wide_reachability() {
        let bfs_constructor = || GenerationNumberBFS::new();
        test_branch_wide_reachability(bfs_constructor);
    }
}