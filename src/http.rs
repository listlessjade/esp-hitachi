// use tiny_http::{Method, Response};

use embedded_svc::http::Headers;
use esp_idf_svc::{
    http::{server::EspHttpServer, Method},
    io::{Read, Write},
};

use crate::rpc::RpcRequester;

pub fn run_http(
    http_channel: RpcRequester,
    // ws_channel: RpcRequester,
    port: u16,
) -> anyhow::Result<EspHttpServer<'static>> {
    // let server = tiny_http::Server::http(addr).unwrap();
    let config = esp_idf_svc::http::server::Configuration {
        http_port: port,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&config)?;

    server.fn_handler::<anyhow::Error, _>("/post", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(128) as usize;
        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;

        http_channel.req_tx.send(buf).unwrap();
        let res = http_channel.res_rx.recv().unwrap();
        let mut resp = req.into_ok_response()?;
        resp.write_all(&res)?;

        Ok(())
    })?;

    Ok(server)
    // loop {
    //     let mut request = server.recv()?;
    //     if *request.method() == Method::Get {
    //         request.respond(Response::empty(201))?;
    //         continue;
    //     }

    //     let mut content = Vec::with_capacity(request.body_length().unwrap_or(128));
    //     request.as_reader().read_to_end(&mut content)?;
    //     req_tx.send(content).unwrap();

    //     request.respond(
    //         Response::from_data(res_rx.recv().unwrap())
    //     )?;
    // }
}
