// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

// This file should only contain functions that accept a String and returns an internal type

use std::{convert::TryFrom, str::FromStr};

use mercurial_types::{HgChangesetId, HgFileNodeId, HgNodeHash};
use mononoke_types::{hash::Sha256, MPath};

use crate::errors::ErrorKind;

pub fn get_mpath(path: String) -> Result<MPath, ErrorKind> {
    MPath::try_from(&*path).map_err(|e| ErrorKind::InvalidInput(path, Some(e)))
}

pub fn get_changeset_id(changesetid: String) -> Result<HgChangesetId, ErrorKind> {
    HgChangesetId::from_str(&changesetid).map_err(|e| ErrorKind::InvalidInput(changesetid, Some(e)))
}

pub fn get_nodehash(hash: &str) -> Result<HgNodeHash, ErrorKind> {
    HgNodeHash::from_str(hash).map_err(|e| ErrorKind::InvalidInput(hash.to_string(), Some(e)))
}

pub fn get_filenode_id(hash: &str) -> Result<HgFileNodeId, ErrorKind> {
    Ok(HgFileNodeId::new(get_nodehash(hash)?))
}

pub fn get_sha256_oid(oid: String) -> Result<Sha256, ErrorKind> {
    Sha256::from_str(&oid).map_err(|e| ErrorKind::InvalidInput(oid.to_string(), Some(e.into())))
}
