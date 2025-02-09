#[cfg(feature = "axum08")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum08")))]
impl axum08::serve::Listener for crate::VsockListener {
    type Io = crate::VsockStream;

    type Addr = vsock::VsockAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match std::future::poll_fn(|cx| self.poll_accept(cx)).await {
                Ok(tuple) => return tuple,
                Err(err) => handle_accept_error(err).await,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.local_addr()
    }
}

#[cfg(feature = "axum08")]
async fn handle_accept_error(err: std::io::Error) {
    if matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    ) {
        return;
    }

    // [From `hyper::Server` in 0.14](https://github.com/hyperium/hyper/blob/v0.14.27/src/server/tcp.rs#L186)
    //
    // > A possible scenario is that the process has hit the max open files
    // > allowed, and so trying to accept a new connection will fail with
    // > `EMFILE`. In some cases, it's preferable to just wait for some time, if
    // > the application will likely close some files (or connections), and try
    // > to accept the connection again. If this option is `true`, the error
    // > will be logged at the `error` level, since it is still a big deal,
    // > and then the listener will sleep for 1 second.
    //
    // hyper allowed customizing this but axum does not.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
