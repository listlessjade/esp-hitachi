// use tiny_http::{Method, Response};

use std::{cell::RefCell, fs::File, io::Write};

use embedded_svc::http::Headers;
use esp_idf_svc::{
    http::{
        server::{EspHttpConnection, EspHttpServer, Handler, Request},
        Method,
    },
    io::{Read as _, Write as _},
    ota::{EspFirmwareInfoLoader, EspOta, FirmwareInfo},
    sys::EspError,
};
use log::Level;

use crate::rpc::{MessageSource, RpcRequester};

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

    // let req_tx = http_channel.req_tx.clone();

    server.fn_handler::<anyhow::Error, _>("/post", Method::Post, move |mut req| {
        let mut slot = http_channel.req_tx.send_ref().unwrap();
        slot.src = MessageSource::HttpRpc;

        slot.buffer
            .resize(req.content_len().unwrap_or(64) as usize, 0);
        req.read_exact(&mut slot.buffer)?;
        drop(slot);

        let res = http_channel.res_rx.recv_ref().unwrap();
        let mut resp = req.into_ok_response()?;
        resp.write_all(&res.buffer)?;

        Ok(())
    })?;

    server.fn_handler::<anyhow::Error, _>("/ota/script", Method::Post, move |mut req| {
        let mut file = File::create("/spiffs/script.rhai")?;

        let mut buf = vec![0; 256];
        let mut total_bytes_read = 0;
        let file_size = req.content_len().unwrap_or_default() as usize;

        while total_bytes_read < file_size {
            let bytes_read = req.read(&mut buf)?;
            file.write_all(&buf[..bytes_read])?;
            total_bytes_read += bytes_read;
        }

        let mut res = req.into_ok_response()?;
        res.write_all(b"script updated!")?;

        Ok(())
    })?;

    server.handler(
        "/ota/firmware",
        Method::Post,
        FirmwareUpdateHandler {
            ota: RefCell::new(EspOta::new().unwrap()),
        },
    )?;

    Ok(server)
}

const FIRMWARE_DOWNLOAD_CHUNK_SIZE: usize = 1024 * 8; // 8kb
const FIRMWARE_MAX_SIZE: usize = 1024 * 1024 * 3; // 3MB
const FIRMWARE_MIN_SIZE: usize = size_of::<FirmwareInfo>() + 1024;

pub struct FirmwareUpdateHandler {
    ota: RefCell<EspOta>,
}

impl Handler<EspHttpConnection<'_>> for FirmwareUpdateHandler {
    type Error = anyhow::Error;

    fn handle(&self, connection: &mut EspHttpConnection) -> Result<(), Self::Error> {
        let mut req = Request::wrap(connection);

        let file_size = req.content_len().unwrap_or(0) as usize;
        if file_size < FIRMWARE_MIN_SIZE {
            respond_and_log(
                req,
                Level::Info,
                400,
                format!("File size {file_size} too small - not proceeding!"),
            )?;
            return Ok(());
        }

        if file_size > FIRMWARE_MAX_SIZE {
            respond_and_log(
                req,
                Level::Info,
                400,
                format!("File size {file_size} too big - not proceeding!"),
            )?;
            return Ok(());
        }

        if !req
            .content_type()
            .is_some_and(|c| c == "application/octet-stream")
        {
            respond_and_log(
                req,
                Level::Info,
                400,
                "File Content-Type incorrect - not proceeding!".to_string(),
            )?;
            return Ok(());
        }

        let mut ota = self.ota.borrow_mut();

        let mut work = ota.initiate_update()?;
        let mut buffer = vec![0; FIRMWARE_DOWNLOAD_CHUNK_SIZE];
        let mut missing_firmware_info = true;
        let mut total_bytes_read = 0;

        let dl_result = loop {
            let Ok(bytes_read) = req.read(&mut buffer) else {
                break Err((500, "IO Error".to_string()));
            };

            log::info!(
                "firmware DL: {:.2}%",
                (total_bytes_read as f64 / file_size as f64) * 100.0
            );

            total_bytes_read += bytes_read;
            if missing_firmware_info {
                let Ok(_info) = get_firmware_info(&buffer[..bytes_read]) else {
                    break Err((
                        400,
                        "Failed to get firmware info from sent bytes".to_string(),
                    ));
                };

                // log::info!("Received firmware info: {info:?}");
                missing_firmware_info = false;
            }

            if bytes_read > 0 {
                if let Err(e) = work.write(&buffer[..bytes_read]) {
                    break Err((500, format!("Failed to write to the OTA: {e}")));
                }
            }

            if total_bytes_read >= file_size {
                break Ok(());
            }
        };

        if let Err((status, err_msg)) = dl_result {
            work.abort().unwrap();
            respond_and_log(req, Level::Error, status, err_msg)?;
            return Ok(());
        }

        if total_bytes_read < file_size {
            work.abort().unwrap();
            respond_and_log(req, Level::Error, 500, format!("was supposed to get {file_size} bytes, but only got {total_bytes_read}. aborting update"))?;
            return Ok(());
        }

        work.complete()?;

        respond_and_log(req, Level::Info, 200, "OTA update completed!".to_owned())?;

        Ok(())
    }
}

fn respond_and_log(
    r: Request<&mut EspHttpConnection>,
    log_level: log::Level,
    status: u16,
    msg: String,
) -> anyhow::Result<()> {
    log::log!(log_level, "{}", msg);
    let mut res = r.into_status_response(status)?;
    res.write_all(msg.as_bytes())?;
    Ok(())
}

fn get_firmware_info(buff: &[u8]) -> Result<(), EspError> {
    let mut loader = EspFirmwareInfoLoader::new();
    loader.load(buff)?;
    Ok(())
    // loader.get_info()
}
