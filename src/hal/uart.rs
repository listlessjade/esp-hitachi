use core::str;
use std::{collections::VecDeque, str::FromStr, sync::Arc};

use arrayvec::ArrayString;
use esp_idf_hal::{delay::BLOCK, io::Write, task::queue::Queue, uart::UartDriver};
use memchr::memchr_iter;
use thingbuf::{
    mpsc::blocking::{StaticChannel, StaticSender},
    recycling::DefaultRecycle,
};

use crate::rpc::{MessageSource, RpcRequester};

pub static UART_QUEUE: StaticChannel<String, 32, DefaultRecycle> =
    StaticChannel::<String, 32, DefaultRecycle>::new();

pub fn spawn_uart_thread(
    engine: RpcRequester,
    uart: UartDriver<'static>,
    uart_bus: Arc<Queue<ArrayString<32>>>
) -> (
    std::thread::JoinHandle<()>,
    std::thread::JoinHandle<()>,
    StaticSender<String>,
) {
    let (uart_tx_channel, uart_rx_channel) = UART_QUEUE.split();
    let (mut uart_tx, uart_rx) = uart.into_split();
    let mut buf = VecDeque::with_capacity(256);

    let receiver_thread = std::thread::spawn(move || {
        loop {
            let mut temp_buf: [u8; 8] = [0; 8];
            let bytes_read = uart_rx.read(&mut temp_buf, BLOCK).unwrap();
            buf.extend(&temp_buf[..bytes_read]);

            let rem = buf.make_contiguous();
            let mut cursor = 0;

            for pos in memchr_iter(b'\n', rem) {
                let mut slot = engine.req_tx.send_ref().unwrap();
                slot.buffer.extend_from_slice(&rem[cursor..pos]);
                slot.src = MessageSource::Uart;
                if let Ok(s) = str::from_utf8(&rem[cursor..pos]) {
                    let _ = uart_bus.send_back(ArrayString::from_str(s).unwrap(), 0);
                }
                // tx.send(String::from_utf8_lossy(&rem[cursor..pos]).into_owned()).unwrap();
                cursor = pos;
            }

            buf.drain(..cursor);
        }
    });

    let sender_thread = std::thread::spawn(move || {
        for line in &uart_rx_channel {
            uart_tx.write_all(line.as_bytes()).unwrap();
            uart_tx.write_all(b"\r\n").unwrap();
            // uart_tx.write_all(format!("{}\r\n", line).as_bytes()).unwrap();
        }
    });

    (receiver_thread, sender_thread, uart_tx_channel)
    // loop {
    //     if let Some(pos) = memchr::memchr(b'\n', buf)
    // }
    // res.push_str(std::str::from_utf8(&temp_buf[..bytes_read]).unwrap());
}
