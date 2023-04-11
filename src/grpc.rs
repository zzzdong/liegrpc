use std::{pin::Pin, task::Poll};

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use http_body::Frame;
use http_body_util::{BodyExt, StreamBody};
use hyper::{
    body::{Body, Incoming},
    HeaderMap, Uri,
};

use crate::{
    channel::{boxed, BoxBody},
    codec::{Decoder, Encoder, ProstDecoder, ProstEncoder},
    metadata::MetadataMap,
    status::Status,
};

pub mod headers {
    pub const GRPC_STATUS: &str = "grpc-status";
    pub const GRPC_MESSAGE: &str = "grpc-message";
    pub const GRPC_STATUS_DETAIL_BIN: &str = "grpc-status-details-bin";
    pub const RESREVER_NAME_PREFIX: &str = "grpc-";

    pub const APPLICATION_GRPC_PROTO: &str = "application/grpc+proto";
}

pub struct Request<T> {
    message: T,
    metadata: MetadataMap,
}

impl<T> Request<T> {
    pub fn new(message: T) -> Self {
        Request {
            message,
            metadata: MetadataMap::new(),
        }
    }

    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut MetadataMap {
        &mut self.metadata
    }
}

impl<T: Clone> Clone for Request<T> {
    fn clone(&self) -> Self {
        Request {
            message: self.message.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<T: prost::Message> Request<T> {
    pub fn into_unary(self, path: &str) -> Result<hyper::Request<BoxBody>, Status> {
        let Request { message, metadata } = self;

        let encoder = ProstEncoder::new();

        let buf = encoder.encode(&message)?;
        let payload = http_body_util::Full::new(buf);

        Ok(into_http_request(path, metadata, boxed(payload)))
    }
}

impl<S, M> Request<S>
where
    S: Stream<Item = M> + Send + 'static,
    M: prost::Message,
{
    pub fn into_stream(self, path: &str) -> hyper::Request<BoxBody> {
        let Request { metadata, message } = self;

        let s = message.map(|m| ProstEncoder::new().encode(&m).map(Frame::data));

        let payload = StreamBody::new(s);

        into_http_request(path, metadata, payload)
    }
}

fn into_http_request(
    path: &str,
    metadata: MetadataMap,
    body: impl Body<Data = Bytes, Error = Status> + Send + 'static,
) -> hyper::Request<BoxBody> {
    let mut builder = hyper::Request::builder()
        .version(hyper::Version::HTTP_2)
        .method(hyper::Method::POST)
        .uri(Uri::try_from(path).unwrap())
        .header(
            hyper::http::header::CONTENT_TYPE,
            headers::APPLICATION_GRPC_PROTO,
        );

    for (k, vv) in &metadata {
        for v in vv {
            builder = builder.header(k, v);
        }
    }

    builder.body(BoxBody::new(body)).unwrap()
}

pub struct Response<T> {
    message: T,
    metadata: MetadataMap,
}

impl<T> Response<T> {
    pub fn new(metadata: MetadataMap, message: T) -> Self {
        Response { message, metadata }
    }

    pub fn get_ref(&self) -> &T {
        &self.message
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.message
    }

    pub fn metadata(&self) -> &MetadataMap {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut MetadataMap {
        &mut self.metadata
    }

    pub fn into_parts(self) -> (MetadataMap, T) {
        (self.metadata, self.message)
    }
}

impl<T> Response<T>
where
    T: prost::Message + Default + 'static,
{
    pub async fn new_unary(resp: hyper::Response<Incoming>) -> Result<Self, Status> {
        let (mut parts, body) = resp.into_parts();

        let status = if parts.status != hyper::StatusCode::OK {
            Status::from_http_status(parts.status)
        } else {
            Status::from_header_map(&parts.headers)?
        };

        if !status.is_ok() {
            return Err(status);
        }

        let mut streaming = Streaming::new(body);

        let message = streaming
            .recv_message()
            .await?
            .ok_or_else(|| Status::internal("expect first message"))?;

        // when unary, should wait for trailer
        streaming.recv_trailers().await?;
        parts.headers.extend(streaming.take_trailers());

        // status from header and trailer.
        let status = Status::from_header_map(&parts.headers)?;
        if !status.is_ok() {
            return Err(status);
        }

        let mut metadata = MetadataMap::new();
        metadata.merge_http_header(&parts.headers);

        Ok(Response { message, metadata })
    }

    pub fn message(&self) -> &T {
        &self.message
    }
}

impl<M> Response<Streaming<M>>
where
    M: prost::Message + Default + 'static,
{
    pub async fn new_streaming(resp: hyper::Response<Incoming>) -> Result<Self, Status> {
        let (parts, body) = resp.into_parts();

        let status = if parts.status != hyper::StatusCode::OK {
            Status::from_http_status(parts.status)
        } else {
            Status::from_header_map(&parts.headers)?
        };

        if !status.is_ok() {
            return Err(status);
        }

        let mut metadata = MetadataMap::new();
        metadata.merge_http_header(&parts.headers);

        let streaming = Streaming::new(body);

        Ok(Response {
            message: streaming,
            metadata,
        })
    }

    pub fn message_stream(&mut self) -> impl Stream<Item = Result<M, Status>> + '_ {
        &mut self.message
    }

    pub async fn recv_message(&mut self) -> Result<Option<M>, Status> {
        self.message.recv_message().await
    }

    pub async fn recv_trailers(&mut self) -> Result<&HeaderMap, Status> {
        self.message.recv_trailers().await
    }
}

impl<T: Clone> Clone for Response<T> {
    fn clone(&self) -> Self {
        Response {
            message: self.message.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

pub struct Streaming<T> {
    decoder: Box<dyn Decoder<Item = T, Error = Status> + Send + 'static>,
    body: Incoming,
    buf: BytesMut,
    trailers: HeaderMap,
}

impl<T> Streaming<T>
where
    T: prost::Message + Default + 'static,
{
    pub fn new(body: Incoming) -> Self {
        let decoder = Box::new(ProstDecoder::new());

        Streaming {
            decoder,
            body,
            buf: BytesMut::new(),
            trailers: HeaderMap::new(),
        }
    }

    pub(crate) fn take_trailers(self) -> HeaderMap {
        self.trailers
    }

    pub async fn recv_message(&mut self) -> Result<Option<T>, Status> {
        while let Some(frame) = self.body.frame().await {
            match frame {
                Ok(f) => {
                    if f.is_data() {
                        let data = f.into_data().expect("frame is not data kind");
                        self.buf.extend(data);

                        match self.decoder.decode(&mut self.buf)? {
                            Some(m) => {
                                return Ok(Some(m));
                            }
                            None => {
                                continue;
                            }
                        }
                    } else {
                        return Err(Status::internal("unknown frame when recv message"));
                    }
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }

        Ok(None)
    }

    pub async fn recv_trailers(&mut self) -> Result<&HeaderMap, Status> {
        while let Some(frame) = self.body.frame().await {
            match frame {
                Ok(f) => {
                    if f.is_trailers() {
                        let trailers = f.into_trailers().expect("frame is not trailers kind");
                        self.trailers.extend(trailers);
                    } else {
                        return Err(Status::internal("unknown frame when recv trailers"));
                    }
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }

        Ok(&self.trailers)
    }

    fn poll_message(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Option<Result<T, Status>>> {
        match Pin::new(&mut self.body).poll_frame(cx) {
            Poll::Ready(b) => {
                if let Some(bs) = b {
                    match bs {
                        Ok(f) => {
                            if f.is_data() {
                                let data = f.into_data().expect("frame is not data kind");
                                self.buf.extend(data);
                            } else if f.is_trailers() {
                                let trailers =
                                    f.into_trailers().expect("frame is not trailer kind");
                                self.trailers.extend(trailers);

                                // when trailers, end of messages
                                return Poll::Ready(None);
                            }
                        }
                        Err(err) => return Poll::Ready(Some(Err(err.into()))),
                    }
                };

                match self.decoder.decode(&mut self.buf) {
                    Ok(ret) => match ret {
                        Some(m) => Poll::Ready(Some(Ok(m))),
                        None => Poll::Pending,
                    },
                    Err(err) => Poll::Ready(Some(Err(err))),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<M> Stream for Streaming<M>
where
    M: prost::Message + Default + 'static,
{
    type Item = Result<M, Status>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::get_mut(self).poll_message(cx)
    }
}
