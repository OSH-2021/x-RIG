#[macro_use]
extern crate log;
extern crate rust_freertos;

use rust_freertos::*;
use simplelog::*;
use stream_buffer::StreamBuffer;
use std::sync::Arc;

fn main() { // test streambuffer
    // 两个任务共享所有权，所以需Arc包装。
    let buffer_recv = Arc::new(StreamBuffer::new(10));
    let buffer_sender = Arc::clone(&buffer_recv);
    let _ = TermLogger::init(LevelFilter::Trace, Config::default());
    // 发送数据的任务代码。
    let sender = move || {
        for i in 1..11 {
            // send方法的参数包括要发送的数据、最大发送值和 ticks_to_wait
            buffer_sender.send(i, 1024，pdMS_TO_TICKS!(50)).unwrap();
        }
        loop {
        }
    };
    // 接收数据的任务代码。
    let receiver = move || {
        let mut sum = 0;
        loop {
            // receive方法的参数只有ticks_to_wait和 最大接受值
            if let Ok(x) = buffer_recv.receive(1, pdMS_TO_TICKS!(10)) {
                println!("{}", x);
                sum += x;
            } else {
                trace!("receive END");
                // 若等待30ms仍未收到数据，则认为发送结束。
                assert_eq!(sum, 55);
                kernel::task_end_scheduler();
            }
        }
    };
    // 创建这两个任务。
    let _sender_task = task_control::TCB::new()
        .name("Sender")
        .priority(3)
        .initialise(sender);
    let _receiver_task = task_control::TCB::new()
        .name("Receiver")
        .priority(3)
        .initialise(receiver);     
    kernel::task_start_scheduler();
}