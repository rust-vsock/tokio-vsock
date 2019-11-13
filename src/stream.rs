/*
 * Tokio Reference TCP Implementation
 * Copyright (c) 2019 Tokio Contributors
 *
 * Permission is hereby granted, free of charge, to any
 * person obtaining a copy of this software and associated
 * documentation files (the "Software"), to deal in the
 * Software without restriction, including without
 * limitation the rights to use, copy, modify, merge,
 * publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software
 * is furnished to do so, subject to the following
 * conditions:
 *
 * The above copyright notice and this permission notice
 * shall be included in all copies or substantial portions
 * of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
 * ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
 * TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
 * PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
 * SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
 * OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
 * IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
 * DEALINGS IN THE SOFTWARE.
 */

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

use std::io::{Error, ErrorKind, Read, Result, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};

use bytes::{Buf, BufMut};
use futures::{Async, Future, Poll};
use mio::Ready;
use nix::sys::socket::SockAddr;
use std::mem;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::reactor::{Handle, PollEvented2};

use super::iovec::IoVec;

/// An I/O object representing a Virtio socket connected to a remote endpoint.
#[derive(Debug)]
pub struct VsockStream {
    io: PollEvented2<super::mio::VsockStream>,
}

impl VsockStream {
    pub(crate) fn new(connected: super::mio::VsockStream) -> Self {
        let io = PollEvented2::new(connected);
        Self { io }
    }

    /// Create a new socket connected to the specified address.
    pub fn connect(addr: &SockAddr) -> ConnectFuture {
        use self::ConnectFutureState::*;
        let inner = match super::mio::VsockStream::connect(addr) {
            Ok(vsock) => Waiting(Self::new(vsock)),
            Err(e) => Error(e),
        };
        ConnectFuture { inner }
    }

    /// Create a new socket from an existing blocking socket.
    pub fn from_std(stream: vsock::VsockStream, handle: &Handle) -> Result<Self> {
        let io = super::mio::VsockStream::from_std(stream)?;
        let io = PollEvented2::new_with_handle(io, handle)?;
        Ok(VsockStream { io })
    }

    /// Check the socket's read readiness state.
    pub fn poll_read_ready(&self, mask: Ready) -> Result<Async<Ready>> {
        self.io.poll_read_ready(mask)
    }

    /// Check the socket's write readiness state.
    pub fn poll_write_ready(&self) -> Result<Async<Ready>> {
        self.io.poll_write_ready()
    }

    /// The local address that this socket is bound to.
    pub fn local_addr(&self) -> Result<SockAddr> {
        self.io.get_ref().local_addr()
    }

    /// The remote address that this socket is connected to.
    pub fn peer_addr(&self) -> Result<SockAddr> {
        self.io.get_ref().peer_addr()
    }

    /// Shuts down the read, write, or both halves of this connection.
    pub fn shutdown(&self, how: Shutdown) -> Result<()> {
        self.io.get_ref().shutdown(how)
    }
}

impl AsRawFd for VsockStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}

impl Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        <&Self>::write(&mut &*self, buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        <&Self>::read(&mut &*self, buf)
    }
}

impl AsyncWrite for VsockStream {
    fn shutdown(&mut self) -> Result<Async<()>> {
        Ok(Async::Ready(()))
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, Error> {
        <&Self>::write_buf(&mut &*self, buf)
    }
}

impl AsyncRead for VsockStream {
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, Error> {
        <&Self>::read_buf(&mut &*self, buf)
    }
}

impl Write for &VsockStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.io.get_ref().write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Read for &VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.io.get_ref().read(buf)
    }
}

impl AsyncWrite for &VsockStream {
    fn shutdown(&mut self) -> Result<Async<()>> {
        Ok(Async::Ready(()))
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, Error> {
        if let Async::NotReady = self.io.poll_write_ready()? {
            return Ok(Async::NotReady);
        }

        let r = {
            // The `IoVec` type can't have a zero-length size, so create a dummy
            // version from a 1-length slice which we'll overwrite with the
            // `bytes_vec` method.
            static DUMMY: &[u8] = &[0];
            let iovec = <&IoVec>::from(DUMMY);
            let mut bufs = [iovec; 64];
            let n = buf.bytes_vec(&mut bufs);
            self.io.get_ref().write_bufs(&bufs[..n])
        };
        match r {
            Ok(n) => {
                buf.advance(n);
                Ok(Async::Ready(n))
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

impl AsyncRead for &VsockStream {
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, Error> {
        if let Async::NotReady = self.io.poll_read_ready(mio::Ready::readable())? {
            return Ok(Async::NotReady);
        }

        let r = unsafe {
            // The `IoVec` type can't have a 0-length size, so we create a bunch
            // of dummy versions on the stack with 1 length which we'll quickly
            // overwrite.
            let b1: &mut [u8] = &mut [0];
            let b2: &mut [u8] = &mut [0];
            let b3: &mut [u8] = &mut [0];
            let b4: &mut [u8] = &mut [0];
            let b5: &mut [u8] = &mut [0];
            let b6: &mut [u8] = &mut [0];
            let b7: &mut [u8] = &mut [0];
            let b8: &mut [u8] = &mut [0];
            let b9: &mut [u8] = &mut [0];
            let b10: &mut [u8] = &mut [0];
            let b11: &mut [u8] = &mut [0];
            let b12: &mut [u8] = &mut [0];
            let b13: &mut [u8] = &mut [0];
            let b14: &mut [u8] = &mut [0];
            let b15: &mut [u8] = &mut [0];
            let b16: &mut [u8] = &mut [0];
            let mut bufs: [&mut IoVec; 16] = [
                b1.into(),
                b2.into(),
                b3.into(),
                b4.into(),
                b5.into(),
                b6.into(),
                b7.into(),
                b8.into(),
                b9.into(),
                b10.into(),
                b11.into(),
                b12.into(),
                b13.into(),
                b14.into(),
                b15.into(),
                b16.into(),
            ];
            let n = buf.bytes_vec_mut(&mut bufs);
            self.io.get_ref().read_bufs(&mut bufs[..n])
        };

        match r {
            Ok(n) => {
                unsafe {
                    buf.advance_mut(n);
                }
                Ok(Async::Ready(n))
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                self.io.clear_read_ready(mio::Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

/// Future returned by `VsockStream::connect` which will resolve to a `VsockStream`
/// when the stream is connected.
#[derive(Debug)]
pub struct ConnectFuture {
    inner: ConnectFutureState,
}

impl Future for ConnectFuture {
    type Item = VsockStream;
    type Error = Error;

    fn poll(&mut self) -> Poll<VsockStream, Error> {
        self.inner.poll()
    }
}

#[derive(Debug)]
enum ConnectFutureState {
    Waiting(VsockStream),
    Error(Error),
    Empty,
}

impl ConnectFutureState {
    fn poll_inner<F>(&mut self, f: F) -> Poll<VsockStream, Error>
    where
        F: FnOnce(&mut PollEvented2<super::mio::VsockStream>) -> Poll<mio::Ready, Error>,
    {
        {
            let stream = match *self {
                ConnectFutureState::Waiting(ref mut s) => s,
                ConnectFutureState::Error(_) => {
                    let e = match mem::replace(self, ConnectFutureState::Empty) {
                        ConnectFutureState::Error(e) => e,
                        _ => panic!(),
                    };
                    return Err(e);
                }
                ConnectFutureState::Empty => panic!("can't poll vsock stream twice"),
            };

            // Once we've connected, wait for the stream to be writable as
            // that's when the actual connection has been initiated. Once we're
            // writable we check for `take_socket_error` to see if the connect
            // actually hit an error or not.
            //
            // If all that succeeded then we ship everything on up.
            if let Async::NotReady = f(&mut stream.io)? {
                return Ok(Async::NotReady);
            }

            if let Some(e) = stream.io.get_ref().take_error()? {
                return Err(e);
            }
        }

        match mem::replace(self, ConnectFutureState::Empty) {
            ConnectFutureState::Waiting(stream) => Ok(Async::Ready(stream)),
            _ => panic!(),
        }
    }
}

impl Future for ConnectFutureState {
    type Item = VsockStream;
    type Error = Error;

    fn poll(&mut self) -> Poll<VsockStream, Error> {
        self.poll_inner(|io| io.poll_write_ready())
    }
}
