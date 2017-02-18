//! Pipelined, multiplexed protocols.
//!
//! See the crate-level docs for an overview.

use std::io;
use std::hash::Hash;
use std::fmt::Debug;
use futures::{Stream, Sink, Async};
use tokio_core::io::{Io, Framed, Codec};

mod frame_buf;

mod client;
pub use self::client::ClientProto;

mod server;
pub use self::server::ServerProto;

mod frame;
pub use self::frame::Frame;


pub mod advanced;

/// Identifies a request / response thread
pub type RequestId = u64;

/// WIP RequestId abstraction
pub trait RId: Copy + Hash + Eq + Debug + 'static {}

impl<T: Copy + Hash + Eq + Debug + 'static> RId for T {}

/// `RequestIdSource` is used to generate at minimum session-wide unique identifiers of type `RId`.
/// Uniqueness needs depend on the application and can be wider than single session.
///
/// Depending on the protocol the identifier can be generated or embedded in the message `T`.
pub trait RequestIdSource<Id, T>: 'static {
    /// Generate the next request id or look it up from the message
    fn next(&mut self, msg: &T) -> Id;
}

/// `RequestIdSource` generated from by an u64 counter
pub struct Counter(u64);

impl Counter {
    /// Initialize the counter with value 0
    pub fn new() -> Self {
        Counter(0)
    }
}

impl<T> RequestIdSource<u64, T> for Counter {
    fn next(&mut self, _: &T) -> u64 {
        let ret = self.0;
        self.0 += 1;
        ret
    }
}


/// A marker used to flag protocols as being streaming and multiplexed.
///
/// This is an implementation detail; to actually implement a protocol,
/// implement the `ClientProto` or `ServerProto` traits in this module.
pub struct StreamingMultiplex<B>(B);

/// Additional transport details relevant to streaming, multiplexed protocols.
///
/// All methods added in this trait have default implementations.
pub trait Transport<RID, ReadBody>: 'static +
    Stream<Error = io::Error> +
    Sink<SinkError = io::Error>
{
    /// Allow the transport to do miscellaneous work (e.g., sending ping-pong
    /// messages) that is not directly connected to sending or receiving frames.
    ///
    /// This method should be called every time the task using the transport is
    /// executing.
    fn tick(&mut self) {}

    /// Cancel interest in the exchange identified by RequestId
    fn cancel(&mut self, request_id: RID) -> io::Result<()> {
        drop(request_id);
        Ok(())
    }

    /// Tests to see if this I/O object may accept a body frame for the given
    /// request ID
    fn poll_write_body(&mut self, id: RID) -> Async<()> {
        drop(id);
        Async::Ready(())
    }

    /// Invoked before the multiplexer dispatches the body chunk to the body
    /// stream.
    fn dispatching_body(&mut self, id: RID, body: &ReadBody) {
        drop(id);
        drop(body);
    }
}

impl<T:Io + 'static, C: Codec + 'static, RID, ReadBody> Transport<RID, ReadBody> for Framed<T,C> {}
