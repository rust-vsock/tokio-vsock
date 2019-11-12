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

use futures::future::err;
use futures::{Future, Stream};
use hyper::client::Client;
use hyper::{Body, Method, Request, Response, Uri as HyperUri};
use hyper_vsock::{Uri, VsockConnector};
use nix::sys::socket::{SockAddr, VsockAddr};
use std::io;
use tokio::runtime::Runtime;

/// Make a simple GET request over vsock to the test server.
/// Further testing is required, however this test, combined with the server
/// hits a surprising number of code paths.
#[test]
fn test_vsock_server() {
    let url: HyperUri = Uri::new(&SockAddr::Vsock(VsockAddr::new(3, 8000)), "/").into();
    let req = Request::builder()
        .uri(url)
        .method(Method::GET)
        .body(Body::empty())
        .expect("Unable to build request");

    let client = Client::builder()
        .keep_alive(false)
        .build::<_, Body>(VsockConnector::new());

    let task = client
        .request(req)
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "bad request"))
        .and_then(
            |res: Response<Body>| -> Box<dyn Future<Item = String, Error = io::Error> + Send> {
                if !res.status().is_success() {
                    return Box::new(err(io::Error::new(io::ErrorKind::Other, "bad status")));
                }
                Box::new(
                    res.into_body()
                        .map_err(|_| ())
                        .fold(vec![], |mut acc, chunk| {
                            acc.extend_from_slice(&chunk);
                            Ok(acc)
                        })
                        .and_then(|v| String::from_utf8(v).map_err(|_| ()))
                        .map_err(|_| io::Error::new(io::ErrorKind::Other, "bad response")),
                )
            },
        );

    let runtime = Runtime::new().expect("unable to create runtime");
    let response = runtime.block_on_all(task).expect("request failed");

    assert_eq!(response, "Hello World!");
}
