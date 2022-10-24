use std::marker::PhantomData;

use bytes::BytesMut;
use hyper::{client::HttpConnector, Body};


pub struct Request<T> {
    message: T,
}

impl<T> Request<T> 
    where T: prost::Message
{
    fn into_http(self) -> hyper::Request<Body> {
        hyper::Request::builder().body(Body::empty()).unwrap()
    }
}

pub struct Response<T> {
    decoder: Box<dyn Decoder<Item = T, Error = anyhow::Error>>,
    body: Body,
    buf: BytesMut,
}

impl<T> Response<T>
    where T: prost::Message
{
    fn from_http(resp: hyper::Response<Body>) -> Self {
        let (parts, body) = resp.into_parts();


        Response { decoder:, body, buf: BytesMut::new() }
    }
}


struct ProstDecoder<T> {
    message: PhantomData<T>,
}


impl<T: prost::Message + Default> Decoder for ProstDecoder<T> {
    type Item = T;
    type Error = anyhow::Error;

    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        T::decode(buf);
        unimplemented!()
    }
}


#[async_trait::async_trait]
pub trait GrpcClient {
    async fn unary<M1: prost::Message, M2: prost::Message>(
        &mut self,
        method: &str,
        request: Request<M1>,
    ) -> anyhow::Result<Response<M2>>;
}


trait Decoder {
    type Item: prost::Message;
    type Error: From<std::io::Error>;

    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;
}

pub struct Client {
    inner: hyper::Client<hyper_rustls::HttpsConnector<HttpConnector>>,
}

impl Client {
    pub fn new() -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http2()
            .build();

        Client {
            inner: hyper::Client::builder().build(https),
        }
    }
}

#[async_trait::async_trait]
impl GrpcClient for Client {
    async fn unary<M1: prost::Message, M2: prost::Message>(
        &mut self,
        method: &str,
        request: Request<M1>,
    ) -> anyhow::Result<Response<M2>> {
        let req = request.into_http();
        let resp = self.inner.request(req).await?;

        let resp = Response::from_http(resp);

        Ok(resp)
    }
}