use crate::{
    channel::Channel,
    grpc::{Request, Response, SRequest},
    status::Status,
};
use futures::Stream;

#[async_trait::async_trait]
pub trait GrpcClient {
    async fn unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<M2>, Status>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static;

    async fn streaming<M1, M2, S>(
        &mut self,
        path: &str,
        request: SRequest<S>,
    ) -> Result<Response<M2>, Status>
    where
        S: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static;
}

pub struct Client {
    channel: Channel,
}

impl Client {
    pub fn new(target: impl Iterator<Item = impl AsRef<str>>) -> Result<Self, Status> {
        let channel = Channel::new(target)?;

        Ok(Client { channel })
    }
}

#[async_trait::async_trait]
impl GrpcClient for Client {
    async fn unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<M2>, Status>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_http();

        let resp = self.channel.call(path, req).await?;

        let resp = Response::from_http(resp);

        Ok(resp)
    }

    async fn streaming<M1, M2, S>(
        &mut self,
        path: &str,
        request: SRequest<S>,
    ) -> Result<Response<M2>, Status>
    where
        S: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_http();

        let resp = self.channel.call(path, req).await?;

        let resp = Response::from_http(resp);

        Ok(resp)
    }
}
