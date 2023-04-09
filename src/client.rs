use futures::Stream;

use crate::{
    channel::Channel,
    grpc::{Request, Response, Streaming},
    status::Status,
};

#[async_trait::async_trait]
pub trait GrpcClient {
    async fn unary_unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<M2>, Status>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static;

    async fn unary_streaming<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<Streaming<M2>>, Status>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static;

    async fn streaming_unary<M1, M2, S1>(
        &mut self,
        path: &str,
        request: Request<S1>,
    ) -> Result<Response<M2>, Status>
    where
        S1: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static;

    async fn streaming_streaming<M1, M2, S1>(
        &mut self,
        path: &str,
        request: Request<S1>,
    ) -> Result<Response<Streaming<M2>>, Status>
    where
        S1: Stream<Item = M1> + Send + 'static,
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
    async fn unary_unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<M2>, Status>
    where
        M1: prost::Message + Send,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_unary(path)?;

        let resp = self.channel.call(req).await?;

        Response::new_unary(resp).await
    }

    async fn unary_streaming<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> Result<Response<Streaming<M2>>, Status>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_unary(path)?;

        let resp = self.channel.call(req).await?;

        Response::new_streaming(resp).await
    }

    async fn streaming_unary<M1, M2, S1>(
        &mut self,
        path: &str,
        request: Request<S1>,
    ) -> Result<Response<M2>, Status>
    where
        S1: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_stream(path);

        let resp = self.channel.call(req).await?;

        Response::new_unary(resp).await
    }

    async fn streaming_streaming<M1, M2, S1>(
        &mut self,
        path: &str,
        request: Request<S1>,
    ) -> Result<Response<Streaming<M2>>, Status>
    where
        S1: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let req = request.into_stream(path);

        let resp = self.channel.call(req).await?;

        Response::new_streaming(resp).await
    }
}
