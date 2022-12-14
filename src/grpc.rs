use std::{pin::Pin, task::Poll};

use bytes::{BufMut, BytesMut};
use futures::{Stream, StreamExt};
use hyper::{body::HttpBody, Body, HeaderMap};

use crate::{
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
    pub fn into_unary(self) -> Result<hyper::Request<Body>, Status> {
        let encoder = ProstEncoder::new();

        let m = encoder.encode(&self.message)?;
        let payload = Body::from(m);

        Ok(into_http_request(&self.metadata, payload))
    }
}

impl<S, M> Request<S>
where
    S: Stream<Item = M> + Send + 'static,
    M: prost::Message,
{
    pub fn into_stream(self) -> hyper::Request<Body> {
        let s = self.message.map(|m| ProstEncoder::new().encode(&m));

        let payload = Body::wrap_stream(s);

        into_http_request(&self.metadata, payload)
    }
}

fn into_http_request(metadata: &MetadataMap, body: Body) -> hyper::Request<Body> {
    let mut builder = hyper::Request::builder()
        .version(hyper::Version::HTTP_2)
        .method(hyper::Method::POST)
        .header(hyper::http::header::CONTENT_TYPE, headers::APPLICATION_GRPC_PROTO);

    for (k, vv) in metadata {
        for v in vv {
            builder = builder.header(k, v);
        }
    }

    builder.body(body).unwrap()
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
    pub async fn new_unary(resp: hyper::Response<Body>) -> Result<Self, Status> {
        let (mut parts, body) = resp.into_parts();

        if parts.status != hyper::StatusCode::OK {
            let status = Status::from_http_status(parts.status);
            if !status.is_ok() {
                return Err(status);
            }
        }

        let mut streaming = Streaming::new(body);

        let message = streaming
            .recv_message()
            .await?
            .ok_or_else(|| Status::internal("expect first message"))?;

        // when unary, should wait for trailer
        if let Some(trailer) = streaming.recv_trailers().await? {
            parts.headers.extend(trailer);
        }

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
    pub async fn new_streaming(resp: hyper::Response<Body>) -> Result<Self, Status> {
        let (parts, body) = resp.into_parts();

        if parts.status != hyper::StatusCode::OK {
            let status = Status::from_http_status(parts.status);
            if !status.is_ok() {
                return Err(status);
            }
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

    pub async fn recv_trailers(&mut self) -> Result<Option<HeaderMap>, Status> {
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
    body: Body,
    buf: BytesMut,
}

impl<T> Streaming<T>
where
    T: prost::Message + Default + 'static,
{
    pub fn new(body: hyper::Body) -> Self {
        let decoder = Box::new(ProstDecoder::new());

        Streaming {
            decoder,
            body,
            buf: BytesMut::new(),
        }
    }

    pub async fn recv_message(&mut self) -> Result<Option<T>, Status> {
        while let Some(data) = self.body.data().await {
            match data {
                Ok(b) => {
                    self.buf.extend(b);

                    match self.decoder.decode(&mut self.buf)? {
                        Some(m) => {
                            return Ok(Some(m));
                        }
                        None => {
                            continue;
                        }
                    }
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }

        Ok(None)
    }

    fn poll_message(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Option<Result<T, Status>>> {
        match Pin::new(&mut self.body).poll_data(cx) {
            Poll::Ready(b) => {
                if let Some(bs) = b {
                    match bs {
                        Ok(buf) => {
                            self.buf.put(buf);
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

    pub async fn recv_trailers(&mut self) -> Result<Option<HeaderMap>, Status> {
        self.body.trailers().await.map_err(Into::into)
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
