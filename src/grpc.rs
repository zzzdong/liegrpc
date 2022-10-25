use bytes::BytesMut;
use futures::{Stream, StreamExt};
use hyper::{body::HttpBody, Body};

use crate::{
    codec::{Decoder, Encoder, ProstDecoder, ProstEncoder},
    metadata::Metadata,
};

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

    pub fn into_http(self) -> hyper::Request<Body> {
        let mut builder = hyper::Request::builder()
            .version(hyper::Version::HTTP_2)
            .method("POST")
            .header("Content-Type", "application/grpc+proto");

        for (k, v) in self.metadata.map {
            builder = builder.header(k, v);
        }

        let encoder = ProstEncoder::new();

        let payload = encoder.encode(&self.message).unwrap();

        builder.body(Body::from(payload)).unwrap()
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
        let mut builder = hyper::Request::builder()
            .version(hyper::Version::HTTP_2)
            .method("POST")
            .header("Content-Type", "application/grpc+proto");

        for (k, v) in self.metadata.map {
            builder = builder.header(k, v);
        }

        let s = self.message.map(|m| ProstEncoder::new().encode(&m));

        let payload = Body::wrap_stream(s);

        builder.body(payload).unwrap()
    }
}

pub struct Response<T> {
    decoder: Box<dyn Decoder<Item = T, Error = anyhow::Error> + 'static>,
    body: Body,
    buf: BytesMut,
}

impl<T> Response<T>
where
    T: prost::Message + Default + 'static,
{
    pub fn from_http(resp: hyper::Response<Body>) -> Self {
        let (parts, body) = resp.into_parts();

        let decoder = Box::new(ProstDecoder::new());

        Response {
            decoder,
            body,
            buf: BytesMut::new(),
        }
    }

    pub async fn message(&mut self) -> Result<Option<T>, anyhow::Error> {
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
