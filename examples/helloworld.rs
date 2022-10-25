use liegrpc::{
    client::GrpcClient,
    grpc::{Request, Response},
};

/// The request message containing the user's name.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelloRequest {
    #[prost(string, tag = "1")]
    pub name: std::string::String,
}
/// The response message containing the greetings
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelloReply {
    #[prost(string, tag = "1")]
    pub message: std::string::String,
}

#[tokio::main]
async fn main() {
    let uri = "http://127.0.0.1:50001".parse().unwrap();
    let mut client = liegrpc::client::Client::new(uri);

    let req = Request::new(HelloRequest {
        name: "tom".to_owned(),
    });

    let mut rsp: Response<HelloReply> = client
        .unary("/helloworld.Greeter/SayHello", req)
        .await
        .unwrap();

    let msg = rsp.message().await.unwrap();

    println!("=> {:?}", msg);
}
