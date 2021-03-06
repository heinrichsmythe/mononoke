// Copyright (c) 2004-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

#![deny(warnings)]
#![feature(try_from)]

use actix_web::{http::header, server, App, HttpRequest, HttpResponse, Json, Path, State};
use bytes::Bytes;
use clap::{value_t, Arg};
use failure::Fallible;
use futures::Future;
use http::uri::{Authority, Parts, PathAndQuery, Scheme, Uri};
use std::sync::Arc;
use uuid::Uuid;

use context::CoreContext;
use metaconfig_parser::RepoConfigs;
use panichandler::Fate;
use percent_encoding::percent_decode;
use scuba_ext::ScubaSampleBuilder;
use serde_derive::Deserialize;
use slog::{info, o, Drain, Level, Logger};
use slog_glog_fmt::{kv_categorizer, kv_defaults, GlogFormat};
use slog_logview::LogViewDrain;
use sshrelay::SshEnvVars;
use tokio::runtime::Runtime;
use tracing::TraceContext;

mod actor;
mod errors;
mod from_string;
mod middleware;
mod thrift;

use crate::actor::{
    BatchRequest, Mononoke, MononokeQuery, MononokeRepoQuery, MononokeRepoResponse, Revision,
};
use crate::errors::ErrorKind;
use crate::middleware::ScubaMiddleware;

mod config {
    pub const SCUBA_TABLE: &str = "mononoke_apiserver";
}

// Currently logging and scuba is handled using the middleware service
// so we pass on a fake context
fn prepare_fake_ctx(state: &State<HttpServerState>) -> CoreContext {
    CoreContext::new(
        Uuid::new_v4(),
        state.logger.clone(),
        ScubaSampleBuilder::with_discard(),
        None,
        TraceContext::default(),
        None,
        SshEnvVars::default(),
    )
}

#[derive(Deserialize)]
struct GetRawFileParams {
    repo: String,
    changeset: String,
    path: String,
}

// The argument of this function is because the trait `actix_web::FromRequest` is implemented
// for tuple (A, B, ...) (up to 9 elements) [1]. These arguments must implement
// `actix_web::FromRequest` as well so actix-web will try to extract them from `actix::HttpRequest`
// for us. In this case, the `State<HttpServerState>` and `Path<GetRawFileParams>`.
// [1] https://docs.rs/actix-web/0.6.11/actix_web/trait.FromRequest.html#impl-FromRequest%3CS%3E-3
fn get_raw_file(
    (state, params): (State<HttpServerState>, Path<GetRawFileParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetRawFile {
                revision: Revision::CommitHash(params.changeset),
                path: params.path,
            },
        },
    )
}

#[derive(Deserialize)]
struct GetHgFileParams {
    repo: String,
    filenode: String,
}

fn get_hg_file(
    (state, params): (State<HttpServerState>, Path<GetHgFileParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetHgFile {
                filenode: params.filenode,
            },
        },
    )
}

#[derive(Deserialize)]
struct GetFileHistoryParams {
    repo: String,
    filenode: String,
    path: String,
}

fn get_file_history(
    (state, req, params): (
        State<HttpServerState>,
        HttpRequest<HttpServerState>,
        Path<GetFileHistoryParams>,
    ),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetFileHistory {
                filenode: params.filenode,
                path: params.path,
                depth: req.query().get("depth").and_then(|d| d.parse().ok()),
            },
        },
    )
}

#[derive(Deserialize)]
struct IsAncestorParams {
    repo: String,
    ancestor: String,
    descendant: String,
}

fn is_ancestor(
    (state, params): (State<HttpServerState>, Path<IsAncestorParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    let ancestor_parsed = percent_decode(params.ancestor.as_bytes())
        .decode_utf8_lossy()
        .to_string();
    let descendant_parsed = percent_decode(params.descendant.as_bytes())
        .decode_utf8_lossy()
        .to_string();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::IsAncestor {
                ancestor: Revision::CommitHash(ancestor_parsed),
                descendant: Revision::CommitHash(descendant_parsed),
            },
        },
    )
}

#[derive(Deserialize)]
struct ListDirectoryParams {
    repo: String,
    changeset: String,
    path: String,
}

fn list_directory(
    (state, params): (State<HttpServerState>, Path<ListDirectoryParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::ListDirectory {
                revision: Revision::CommitHash(params.changeset),
                path: params.path,
            },
        },
    )
}

#[derive(Deserialize)]
struct GetBlobParams {
    repo: String,
    hash: String,
}

fn get_blob_content(
    (state, params): (State<HttpServerState>, Path<GetBlobParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetBlobContent { hash: params.hash },
        },
    )
}

#[derive(Deserialize)]
struct GetTreeParams {
    repo: String,
    hash: String,
}

fn get_tree(
    (state, params): (State<HttpServerState>, Path<GetTreeParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetTree { hash: params.hash },
        },
    )
}

#[derive(Deserialize)]
struct GetChangesetParams {
    repo: String,
    hash: String,
}

fn get_changeset(
    (state, params): (State<HttpServerState>, Path<GetChangesetParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::GetChangeset {
                revision: Revision::CommitHash(params.hash),
            },
        },
    )
}

#[derive(Deserialize)]
struct DownloadLargeFileParams {
    repo: String,
    oid: String,
}

fn download_large_file(
    (state, params): (State<HttpServerState>, Path<DownloadLargeFileParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::DownloadLargeFile { oid: params.oid },
        },
    )
}

#[derive(Deserialize)]
struct LfsBatchParams {
    repo: String,
}

fn lfs_batch(
    (state, req_json, params, req): (
        State<HttpServerState>,
        Json<BatchRequest>,
        Path<LfsBatchParams>,
        HttpRequest<HttpServerState>,
    ),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    let host_url = req.headers().get(header::HOST);
    let scheme = if state.use_ssl {
        Scheme::HTTPS
    } else {
        Scheme::HTTP
    };
    let lfs_url = host_url
        .and_then(|url_header_value| url_header_value.to_str().ok())
        .and_then(|url| Authority::from_shared(Bytes::from(url)).ok())
        .and_then(|url| {
            let path_and_query = PathAndQuery::from_shared(Bytes::from("")).ok();

            let mut parts = Parts::default();
            parts.scheme = Some(scheme);
            parts.authority = Some(url);
            parts.path_and_query = path_and_query;

            Uri::from_parts(parts).ok()
        });

    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo.clone(),
            kind: MononokeRepoQuery::LfsBatch {
                req: req_json.into_inner(),
                repo_name: params.repo,
                lfs_url,
            },
        },
    )
}

#[derive(Deserialize)]
struct UploadLargeFileParams {
    repo: String,
    oid: String,
}

// TODO(anastasiyaz): T32937714 Bytes -> Streaming
fn upload_large_file(
    (state, body, params): (State<HttpServerState>, Bytes, Path<UploadLargeFileParams>),
) -> impl Future<Item = MononokeRepoResponse, Error = ErrorKind> {
    let params = params.into_inner();
    state.mononoke.send_query(
        prepare_fake_ctx(&state),
        MononokeQuery {
            repo: params.repo,
            kind: MononokeRepoQuery::UploadLargeFile {
                oid: params.oid,
                body,
            },
        },
    )
}

fn setup_logger(debug: bool) -> Logger {
    let level = if debug { Level::Debug } else { Level::Info };

    let decorator = slog_term::TermDecorator::new().build();
    let stderr_drain = GlogFormat::new(decorator, kv_categorizer::FacebookCategorizer).fuse();
    let stderr_drain = slog_async::Async::new(stderr_drain).build().fuse();
    let logview_drain = LogViewDrain::new("errorlog_mononoke_apiserver");

    let drain = slog::Duplicate::new(stderr_drain, logview_drain);
    let drain = slog_stats::StatsDrain::new(drain);
    let drain = drain.filter_level(level);

    Logger::root(
        drain.fuse(),
        o!(kv_defaults::FacebookKV::new().expect("Failed to initialize logging")),
    )
}

#[derive(Clone)]
struct HttpServerState {
    mononoke: Arc<Mononoke>,
    logger: Logger,
    scuba_builder: ScubaSampleBuilder,
    use_ssl: bool,
}

fn main() -> Fallible<()> {
    panichandler::set_panichandler(Fate::Abort);

    let app = clap::App::new("Mononoke API Server")
        .version("0.0.1")
        .about("An API server serves requests for Mononoke")
        .arg(
            Arg::with_name("http-host")
                .short("H")
                .long("http-host")
                .value_name("HOST")
                .default_value("127.0.0.1")
                .help("HTTP host to listen to"),
        )
        .arg(
            Arg::with_name("http-port")
                .short("p")
                .long("http-port")
                .value_name("PORT")
                .default_value("8000")
                .help("HTTP port to listen to"),
        )
        .arg(
            Arg::with_name("thrift-port")
                .long("thrift-port")
                .value_name("PORT")
                .help("Thrift port"),
        )
        .arg(Arg::with_name("with-scuba").long("with-scuba"))
        .arg(Arg::with_name("debug").short("p").long("debug"))
        .arg(Arg::with_name("without-skiplist").long("without-skiplist"))
        .arg(
            Arg::with_name("stdlog")
                .long("stdlog")
                .help("print logs from third-party crates"),
        )
        .arg(
            Arg::with_name("mononoke-config-path")
                .long("mononoke-config-path")
                .value_name("PATH")
                .required(true)
                .help("directory of the config repository"),
        )
        .arg(
            Arg::with_name("ssl-certificate")
                .long("ssl-certificate")
                .value_name("PATH")
                .help("path to the ssl certificate file"),
        )
        .arg(
            Arg::with_name("ssl-private-key")
                .long("ssl-private-key")
                .value_name("PATH")
                .help("path to the ssl private key file")
                .requires("ssl-ca"),
        )
        .arg(
            Arg::with_name("ssl-ca")
                .long("ssl-ca")
                .value_name("PATH")
                .help("path to the ssl ca file"),
        )
        .arg(
            Arg::with_name("ssl-ticket-seeds")
                .long("ssl-ticket-seeds")
                .value_name("PATH")
                .help("path to the ssl ticket seeds"),
        );

    let app = cmdlib::args::add_myrouter_args(app);
    let matches =
        cmdlib::args::add_cachelib_args(app, false /* hide_advanced_args */).get_matches();
    cmdlib::args::init_cachelib(&matches);

    let host = matches.value_of("http-host").unwrap_or("127.0.0.1");
    let port = matches.value_of("http-port").unwrap_or("8000");
    let thrift_port = value_t!(matches.value_of("thrift-port"), i32);
    let debug = matches.is_present("debug");
    let stdlog = matches.is_present("stdlog");
    let config_path = matches
        .value_of("mononoke-config-path")
        .expect("must set config path");
    let with_scuba = matches.is_present("with-scuba");
    let with_skiplist = !matches.is_present("without-skiplist");
    let myrouter_port = cmdlib::args::parse_myrouter_port(&matches);

    let address = format!("{}:{}", host, port);

    let root_logger = setup_logger(debug);
    let actix_logger = root_logger.clone();
    let mononoke_logger = root_logger.clone();
    let thrift_logger = root_logger.clone();

    // These guards have to be placed in main or they would be destoried once the function ends
    let global_logger = root_logger.clone();

    let (_scope_guard, _log_guard) = if stdlog {
        (
            Some(slog_scope::set_global_logger(global_logger)),
            slog_stdlog::init().ok(),
        )
    } else {
        (None, None)
    };

    let mut runtime = Runtime::new().expect("tokio runtime for blocking jobs");
    let repo_configs = RepoConfigs::read_configs(config_path)?;

    let ssl_acceptor = if let Some(cert) = matches.value_of("ssl-certificate") {
        let cert = cert.to_string();
        let private_key = matches
            .value_of("ssl-private-key")
            .expect("must specify ssl private key")
            .to_string();
        let ca_pem = matches
            .value_of("ssl-ca")
            .expect("must specify CA")
            .to_string();
        let ticket_seed = matches
            .value_of("ssl-ticket-seeds")
            .unwrap_or(secure_utils::fb_tls::SEED_PATH)
            .to_string();

        let ssl = secure_utils::SslConfig {
            cert,
            private_key,
            ca_pem,
        };
        let acceptor = secure_utils::build_tls_acceptor_builder(ssl.clone())?;
        Some(secure_utils::fb_tls::tls_acceptor_builder(
            root_logger.clone(),
            ssl.clone(),
            acceptor,
            ticket_seed,
        )?)
    } else {
        None
    };

    let mut scuba_builder = if with_scuba {
        ScubaSampleBuilder::new(config::SCUBA_TABLE)
    } else {
        ScubaSampleBuilder::with_discard()
    };

    scuba_builder.add_common_server_data();

    let use_ssl = ssl_acceptor.is_some();
    let sys = actix::System::new("mononoke-apiserver");
    let mononoke = runtime.block_on(Mononoke::new(
        mononoke_logger.clone(),
        repo_configs,
        myrouter_port,
        with_skiplist,
    ))?;
    let mononoke = Arc::new(mononoke);

    if let Ok(port) = thrift_port {
        thrift::make_thrift(
            thrift_logger,
            host.to_string(),
            port,
            mononoke.clone(),
            scuba_builder.clone(),
        );
    }

    let state = HttpServerState {
        mononoke,
        logger: actix_logger.clone(),
        scuba_builder: scuba_builder.clone(),
        use_ssl,
    };

    let server = server::new(move || {
        App::with_state(state.clone())
            .middleware(middleware::SLogger::new(actix_logger.clone()))
            .middleware(ScubaMiddleware::new(scuba_builder.clone()))
            .route(
                "/health_check",
                http::Method::GET,
                |req: HttpRequest<HttpServerState>| {
                    // removing ScubaSampleBuilder will disable scuba logging for this request.
                    req.extensions_mut().remove::<ScubaSampleBuilder>();
                    HttpResponse::Ok().body("I_AM_ALIVE")
                },
            )
            .scope("/{repo}", |repo| {
                repo.resource("/raw/{changeset}/{path:.*}", |r| {
                    r.method(http::Method::GET).with_async(get_raw_file)
                })
                .resource("/gethgfile/{filenode}", |r| {
                    r.method(http::Method::GET).with_async(get_hg_file)
                })
                .resource("/getfilehistory/{filenode}/{path:.*}", |r| {
                    r.method(http::Method::GET).with_async(get_file_history)
                })
                .resource("/is_ancestor/{ancestor}/{descendant}", |r| {
                    r.method(http::Method::GET).with_async(is_ancestor)
                })
                .resource("/list/{changeset}/{path:.*}", |r| {
                    r.method(http::Method::GET).with_async(list_directory)
                })
                .resource("/blob/{hash}", |r| {
                    r.method(http::Method::GET).with_async(get_blob_content)
                })
                .resource("/tree/{hash}", |r| {
                    r.method(http::Method::GET).with_async(get_tree)
                })
                .resource("/changeset/{hash}", |r| {
                    r.method(http::Method::GET).with_async(get_changeset)
                })
                .resource("/lfs/download/{oid}", |r| {
                    r.method(http::Method::GET).with_async(download_large_file)
                })
                .resource("/objects/batch", |r| {
                    r.method(http::Method::POST).with_async(lfs_batch)
                })
                .resource("/lfs/upload/{oid}", |r| {
                    r.method(http::Method::PUT).with_async(upload_large_file)
                })
            })
    });

    let server = if let Some(acceptor) = ssl_acceptor {
        server.bind_ssl(address, acceptor)?
    } else {
        server.bind(address)?
    };

    let address = server.addrs()[0];

    server.start();

    if use_ssl {
        info!(root_logger, "Listening to https://{}", address);
    } else {
        info!(root_logger, "Listening to http://{}", address);
    }

    let _ = sys.run();

    Ok(())
}
