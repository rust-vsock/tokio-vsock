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

/*
 * Additional material Copyright (c) 2015-2018 Doug Tangren
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
 * NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
 * LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
 * OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
 * WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

//! A basic hyper connector implementation that allows for connecting over Virtio sockets.

use std::borrow::Cow;
use std::io;

use bytes::{Buf, BufMut};
use futures::{Async, Future, Poll};
use hyper::client::connect::Destination;
use hyper::client::connect::{Connect, Connected};
use hyper::Uri as HyperUri;
use nix::sys::socket::{SockAddr, VsockAddr};
use std::io::{Read, Write};
use std::net::Shutdown;
use tokio::prelude::{AsyncRead, AsyncWrite};
use tokio_vsock::ConnectFuture as StreamConnectFuture;
use tokio_vsock::VsockStream;

const VSOCK_SCHEME: &str = "vsock";

#[derive(Debug)]
pub struct Uri<'a> {
    encoded: Cow<'a, str>,
}

impl<'a> Into<HyperUri> for Uri<'a> {
    fn into(self) -> HyperUri {
        self.encoded.as_ref().parse().unwrap()
    }
}

impl<'a> Uri<'a> {
    pub fn new(addr: &SockAddr, path: &str) -> Self {
        let vsock_addr = if let SockAddr::Vsock(addr) = addr {
            addr.0
        } else {
            panic!("requires a virtio socket address");
        };
        Uri {
            encoded: Cow::Owned(format!(
                "{}://{}:{}{}",
                VSOCK_SCHEME, vsock_addr.svm_cid, vsock_addr.svm_port, path
            )),
        }
    }

    fn vsock_addr(dst: &hyper::client::connect::Destination) -> Option<SockAddr> {
        let cid: u32 = match dst.host().parse() {
            Ok(cid) => cid,
            Err(_) => return None,
        };
        let port = match dst.port() {
            Some(port) => u32::from(port),
            None => return None,
        };
        Some(SockAddr::Vsock(VsockAddr::new(cid, port)))
    }
}

#[derive(Clone, Default)]
pub struct VsockConnector;

impl VsockConnector {
    pub fn new() -> Self {
        VsockConnector
    }
}

impl Connect for VsockConnector {
    type Transport = HttpAwareVsockStream;
    type Error = io::Error;
    type Future = ConnectFuture;

    fn connect(&self, destination: Destination) -> Self::Future {
        ConnectFuture::Start(destination)
    }
}

pub enum ConnectFuture {
    Start(Destination),
    Connect(StreamConnectFuture),
}

impl Future for ConnectFuture {
    type Item = (HttpAwareVsockStream, Connected);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self {
                ConnectFuture::Start(destination) => {
                    if destination.scheme() != VSOCK_SCHEME {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid uri {:?}", destination),
                        ));
                    }

                    let addr = match Uri::vsock_addr(&destination) {
                        Some(addr) => addr,
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Invalid uri {:?}", destination),
                            ))
                        }
                    };

                    ConnectFuture::Connect(HttpAwareVsockStream::connect(&addr))
                }

                ConnectFuture::Connect(f) => match f.poll() {
                    Ok(Async::Ready(stream)) => {
                        return Ok(Async::Ready((
                            HttpAwareVsockStream(stream),
                            Connected::new(),
                        )))
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => return Err(err),
                },
            };

            *self = next_state;
        }
    }
}

pub struct HttpAwareVsockStream(VsockStream);

impl HttpAwareVsockStream {
    fn connect(addr: &SockAddr) -> StreamConnectFuture {
        VsockStream::connect(addr)
    }
}

impl Read for HttpAwareVsockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for HttpAwareVsockStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl AsyncRead for HttpAwareVsockStream {
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.read_buf(buf)
    }
}

impl AsyncWrite for HttpAwareVsockStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.0.shutdown(Shutdown::Both).map(|_| Async::Ready(()))
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.write_buf(buf)
    }
}
