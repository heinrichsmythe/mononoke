// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

use std::sync::Arc;

use actix::Arbiter;
use cloned::cloned;
use slog::{info, Logger};

use apiserver_thrift::server::make_MononokeAPIService_server;
use fb303::server::make_FacebookService_server;
use fb303_core::server::make_BaseService_server;
use srserver::ThriftServerBuilder;

use self::dispatcher::ThriftDispatcher;
use self::facebook::FacebookServiceImpl;
use self::mononoke::MononokeAPIServiceImpl;
use super::actor::Mononoke;
use scuba_ext::ScubaSampleBuilder;

mod dispatcher;
mod facebook;
mod mononoke;

pub fn make_thrift(
    logger: Logger,
    host: String,
    port: i32,
    addr: Arc<Mononoke>,
    scuba_builder: ScubaSampleBuilder,
) {
    let dispatcher = ThriftDispatcher(Arbiter::new("thrift-worker"));

    dispatcher.start({
        move |dispatcher| {
            info!(logger, "Starting thrift service at {}:{}", host, port);
            ThriftServerBuilder::new()
                .with_address(&host, port, false)
                .expect(&format!("cannot bind to {}:{}", host, port))
                .with_tls()
                .expect("cannot bind to tls")
                .with_factory(dispatcher, {
                    move || {
                        move |proto| {
                            cloned!(addr, logger, scuba_builder);
                            make_MononokeAPIService_server(
                                proto,
                                MononokeAPIServiceImpl::new(addr, logger, scuba_builder),
                                |proto| {
                                    make_FacebookService_server(
                                        proto,
                                        FacebookServiceImpl {},
                                        |proto| {
                                            make_BaseService_server(proto, FacebookServiceImpl {})
                                        },
                                    )
                                },
                            )
                        }
                    }
                })
                .build()
        }
    });
}
