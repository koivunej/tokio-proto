use BindClient;
use super::{Multiplex, RequestIdSource, RequestId};
use super::lift::{LiftBind, LiftTransport};
use simple::LiftProto;

use std::io;

use streaming::{self, Message};
use streaming::multiplex::StreamingMultiplex;
use tokio_core::reactor::Handle;
use tokio_service::Service;
use futures::{stream, Stream, Sink, Future, IntoFuture, Poll};

type MyStream<E> = stream::Empty<(), E>;

/// An multiplexed client protocol.
///
/// The `T` parameter is used for the I/O object used to communicate, which is
/// supplied in `bind_transport`.
///
/// For simple protocols, the `Self` type is often a unit struct. In more
/// advanced cases, `Self` may contain configuration information that is used
/// for setting up the transport in `bind_transport`.
pub trait ClientProto<T: 'static>: 'static {
    /// Request messages.
    type Request: 'static;

    /// Response messages.
    type Response: 'static;

    /// The type of request ids to used to correlate requests to responses
    type RequestId: RequestId;

    /// The message transport, which usually take `T` as a parameter.
    ///
    /// An easy way to build a transport is to use `tokio_core::io::Framed`
    /// together with a `Codec`; in that case, the transport type is
    /// `Framed<T, YourCodec>`. See the crate docs for an example.
    type Transport: 'static +
        Stream<Item = (Self::RequestId, Self::Response), Error = io::Error> +
        Sink<SinkItem = (Self::RequestId, Self::Request), SinkError = io::Error>;

    /// A future for initializing a transport from an I/O object.
    ///
    /// In simple cases, `Result<Self::Transport, Self::Error>` often suffices.
    type BindTransport: IntoFuture<Item = Self::Transport, Error = io::Error>;

    /// The `RequestIdSource` to use.
    type RequestIds: RequestIdSource<Self::RequestId, Self::Request>;

    /// Create a `RequestIdSource` to generate ids for requests, used both on the wire and
    /// internally to correlate responses to requests.
    fn requestid_source(&self) -> Self::RequestIds;

    /// Build a transport from the given I/O object, using `self` for any
    /// configuration.
    ///
    /// An easy way to build a transport is to use `tokio_core::io::Framed`
    /// together with a `Codec`; in that case, `bind_transport` is just
    /// `io.framed(YourCodec)`. See the crate docs for an example.
    fn bind_transport(&self, io: T) -> Self::BindTransport;
}

impl<T: 'static, P: ClientProto<T>> BindClient<Multiplex, T> for P {
    type ServiceRequest = P::Request;
    type ServiceResponse = P::Response;
    type ServiceError = io::Error;

    type BindClient = ClientService<T, P>;

    fn bind_client(&self, handle: &Handle, io: T) -> Self::BindClient {
        ClientService {
            inner: BindClient::<StreamingMultiplex<MyStream<io::Error>>, T>::bind_client(
                LiftProto::from_ref(self), handle, io
            )
        }
    }
}

impl<T, P> streaming::multiplex::ClientProto<T> for LiftProto<P> where
    T: 'static, P: ClientProto<T>
{
    type Request = P::Request;
    type RequestBody = ();

    type Response = P::Response;
    type ResponseBody = ();
    type RequestId = P::RequestId;

    type Error = io::Error;

    type Transport = LiftTransport<P::Transport, io::Error>;
    type BindTransport = LiftBind<T, <P::BindTransport as IntoFuture>::Future, io::Error>;
    type RequestIds = P::RequestIds;

    fn requestid_source(&self) -> Self::RequestIds {
        P::requestid_source(self.lower())
    }

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        LiftBind::lift(ClientProto::bind_transport(self.lower(), io).into_future())
    }
}

/// Client `Service` for simple multiplex protocols
pub struct ClientService<T, P> where T: 'static, P: ClientProto<T> {
    inner: <LiftProto<P> as BindClient<StreamingMultiplex<MyStream<io::Error>>, T>>::BindClient
}

impl<T, P> Service for ClientService<T, P> where T: 'static, P: ClientProto<T> {
    type Request = P::Request;
    type Response = P::Response;
    type Error = io::Error;
    type Future = ClientFuture<T, P>;

    fn call(&self, req: P::Request) -> Self::Future {
        ClientFuture {
            inner: self.inner.call(Message::WithoutBody(req))
        }
    }
}

impl<T, P> Clone for ClientService<T, P> where T: 'static, P: ClientProto<T> {
    fn clone(&self) -> Self {
        ClientService {
            inner: self.inner.clone(),
        }
    }
}

pub struct ClientFuture<T, P> where T: 'static, P: ClientProto<T> {
    inner: <<LiftProto<P> as BindClient<StreamingMultiplex<MyStream<io::Error>>, T>>::BindClient
            as Service>::Future
}

impl<T, P> Future for ClientFuture<T, P>  where T: 'static, P: ClientProto<T> {
    type Item = P::Response;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match try_ready!(self.inner.poll()) {
            Message::WithoutBody(msg) => Ok(msg.into()),
            Message::WithBody(..) => panic!("bodies not supported"),
        }
    }
}
