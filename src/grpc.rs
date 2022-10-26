use bytes::BytesMut;
use futures::{Stream, StreamExt};
use hyper::{body::HttpBody, Body};

use crate::{
    codec::{Decoder, Encoder, ProstDecoder, ProstEncoder},
    metadata::Metadata,
    status::Status,
};

pub mod headers {
    pub const GRPC_STATUS: &str = "grpc-status";
    pub const GRPC_MESSAGE: &str = "grpc-message";
    pub const GRPC_STATUS_DETAIL_BIN: &str = "grpc-status-details-bin";
    pub const RESREVER_NAME_PREFIX: &str = "grpc-";
}

pub struct Request<T> {
    message: T,
    metadata: Metadata,
}

impl<T: prost::Message> Request<T> {
    pub fn new(message: T) -> Self {
        Request {
            message,
            metadata: Metadata::new(),
        }
    }

    pub fn into_http(self) -> Result<hyper::Request<Body>, Status> {
        let encoder = ProstEncoder::new();

        let m = encoder.encode(&self.message)?;
        let payload = Body::from(m);

        Ok(into_http(&self.metadata, payload))
    }
}

pub struct SRequest<S> {
    message: S,
    metadata: Metadata,
}

impl<S, M> SRequest<S>
where
    S: Stream<Item = M> + Send + 'static,
    M: prost::Message,
{
    pub fn new(message: S) -> Self {
        SRequest {
            message,
            metadata: Metadata::new(),
        }
    }

    pub fn into_http(self) -> hyper::Request<Body> {
        let s = self.message.map(|m| ProstEncoder::new().encode(&m));

        let payload = Body::wrap_stream(s);

        into_http(&self.metadata, payload)
    }
}

fn into_http(metadata: &Metadata, body: Body) -> hyper::Request<Body> {
    let mut builder = hyper::Request::builder()
        .version(hyper::Version::HTTP_2)
        .method(hyper::Method::POST)
        .header("Content-Type", "application/grpc+proto");

    for (k, v) in metadata {
        for vv in v {
            builder = builder.header(k, vv);
        }
    }

    builder.body(body).unwrap()
}

pub struct Response<T> {
    decoder: Box<dyn Decoder<Item = T, Error = Status> + 'static>,
    body: Body,
    buf: BytesMut,
    metadata: Metadata,
}

impl<T> Response<T>
where
    T: prost::Message + Default + 'static,
{
    pub fn from_http(resp: hyper::Response<Body>) -> Result<Self, Status> {
        let (parts, body) = resp.into_parts();

        if parts.status != hyper::StatusCode::OK {
            let status = Status::from_http_status(parts.status);
            if !status.is_ok() {
                return Err(status);
            }
        }

        let status = Status::from_header_map(&parts.headers)?;

        if !status.is_ok() {
            return Err(status);
        }

        let mut metadata = Metadata::new();

        for (k, v) in parts.headers {
            // ignore invalid header
            if let Some(name) = k {
                let _ = metadata.insert(
                    name.as_str(),
                    String::from_utf8_lossy(v.as_bytes()).as_ref(),
                );
            }
        }

        let decoder = Box::new(ProstDecoder::new());

        Ok(Response {
            decoder,
            body,
            buf: BytesMut::new(),
            metadata,
        })
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub async fn message(&mut self) -> Result<Option<T>, Status> {
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
}
