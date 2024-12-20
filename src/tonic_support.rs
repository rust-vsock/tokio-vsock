use crate::VsockAddr;

/// Connection info for a Vsock Stream.
///
/// See [`Connected`][tonic012::transport::server::Connected] for more details.
///
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VsockConnectInfo {
    peer_addr: Option<VsockAddr>,
}

impl VsockConnectInfo {
    /// Return the remote address the IO resource is connected too.
    pub fn peer_addr(&self) -> Option<VsockAddr> {
        self.peer_addr
    }
}

macro_rules! tonic_connected {
    ($tonic_version:ident $cfg:literal) => {
        /// Allow consumers of VsockStream to check that it is connected and valid before use.
        ///
        #[cfg(feature = $cfg)]
        #[cfg_attr(docsrs, doc(cfg(feature = $cfg)))]
        impl $tonic_version::transport::server::Connected for crate::VsockStream {
            type ConnectInfo = VsockConnectInfo;

            fn connect_info(&self) -> Self::ConnectInfo {
                VsockConnectInfo {
                    peer_addr: self.peer_addr().ok(),
                }
            }
        }
    };
}

tonic_connected!(tonic05 "tonic05");
tonic_connected!(tonic06 "tonic06");
tonic_connected!(tonic07 "tonic07");
tonic_connected!(tonic08 "tonic08");
tonic_connected!(tonic09 "tonic09");
tonic_connected!(tonic010 "tonic010");
tonic_connected!(tonic011 "tonic011");
tonic_connected!(tonic012 "tonic012");
