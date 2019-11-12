/*
 * Copyright 2019 fsyncd, Berlin, Germany.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use clap::{crate_authors, crate_version, App, Arg};
use futures::future::{err, ok};
use futures::{Future, Stream};
use hyper::server::conn::Http;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response};
use nix::sys::socket::{SockAddr, VsockAddr};
use std::io;
use tokio_vsock::VsockListener;

/// A simple Virtio socket server that uses Hyper to response to requests.
fn main() {
    let matches = App::new("test_server")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Tokio Virtio socket test server")
        .arg(
            Arg::with_name("listen")
                .long("listen")
                .short("l")
                .help("Port to listen for Virtio connections")
                .required(true)
                .takes_value(true),
        )
        .get_matches();

    let listen_port = matches
        .value_of("listen")
        .expect("port is required")
        .parse::<u32>()
        .expect("port must be a valid integer");

    let listener = VsockListener::bind(&SockAddr::Vsock(VsockAddr::new(
        libc::VMADDR_CID_ANY,
        listen_port,
    )))
    .expect("unable to bind virtio listener");

    println!("Listening for connections on port: {}", listen_port);

    let http = Http::new();

    let task = listener
        .incoming()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "accept connection failed"))
        .for_each(move |stream| {
            let peer_addr = match stream.peer_addr() {
                Ok(peer_addr) => peer_addr,
                Err(e) => return err(e),
            };
            println!("Received connection from: {:?}", peer_addr);
            let service =
                service_fn_ok(|_: Request<Body>| Response::new(Body::from("Hello World!")));
            tokio::spawn(http.serve_connection(stream, service).map_err(|e| {
                panic!("server connection error: {}", e);
            }));
            ok(())
        });

    tokio::run(task.map_err(|_| ()));
}
