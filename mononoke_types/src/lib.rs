// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

//! Base types used throughout Mononoke.

#![deny(warnings)]
// The allow(dead_code) is temporary until Thrift serialization is done.
#![allow(dead_code)]
#![feature(const_fn, try_from)]

extern crate abomonation;
#[macro_use]
extern crate abomonation_derive;
extern crate ascii;
extern crate asyncmemo;
extern crate bincode;
extern crate blake2;
extern crate bytes;
extern crate chrono;
#[macro_use]
extern crate failure_ext as failure;
extern crate heapsize;
#[macro_use]
extern crate heapsize_derive;
#[macro_use]
extern crate lazy_static;
#[cfg(test)]
#[macro_use]
extern crate maplit;
#[cfg_attr(test, macro_use)]
extern crate quickcheck;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate blobstore;
extern crate sql;

extern crate rust_thrift;

extern crate mononoke_types_thrift;

pub mod blob;
pub mod bonsai_changeset;
pub mod datetime;
pub mod errors;
pub mod file_change;
pub mod file_contents;
pub mod generation;
pub mod hash;
pub mod path;
pub mod rawbundle2;
pub mod repo;
pub mod sql_types;
pub mod typed_hash;

pub use blob::{Blob, BlobstoreBytes, BlobstoreValue, ChangesetBlob, ContentBlob, RawBundle2Blob};
pub use bonsai_changeset::{BonsaiChangeset, BonsaiChangesetMut};
pub use datetime::{DateTime, Timestamp};
pub use file_change::{FileChange, FileType};
pub use file_contents::FileContents;
pub use generation::Generation;
pub use path::{check_case_conflicts, MPath, MPathElement, RepoPath};
pub use rawbundle2::RawBundle2;
pub use repo::RepositoryId;
pub use typed_hash::{ChangesetId, ContentId, MononokeId, RawBundle2Id};

mod thrift {
    pub use mononoke_types_thrift::*;
}
