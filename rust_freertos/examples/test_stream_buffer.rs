#[macro_use]
extern crate log;
extern crate rust_freertos;

use rust_freertos::*;
use simplelog::*;


fn main() { // test streambuffer
   

    let mut sender_buffer = stream_buffer::StreamBufferHandle::
         StreamBufferGenericCreate(5,1,true ,5);

    let mut receiver_buffer = sender_buffer.clone();
    let _ = TermLogger::init(LevelFilter::Trace, Config::default());
    // 发送数据的任务代码。
    let sender = move || {
        for i in 1..11 {
            // send方法的参数包括要发送的数据、最大发送值和 ticks_to_wait
            sender_buffer.StreamBufferSend(i, 5, pdMS_TO_TICKS!(5));
        }
        loop {
            
        }
    };
    // 接收数据的任务代码。
    let receiver = move || {
        let mut x :u8 = 0;
        let mut sum = 0;
         {
            // receive方法的参数只有ticks_to_wait和 最大接受值
            let num = receiver_buffer.StreamBufferReceive(&mut x, 1,pdMS_TO_TICKS!(1000));
            if num > 0{

                trace!("The number received:{}", x as u64);

            } else {
                trace!("receive END");
                // 若等待30ms仍未收到数据，则认为发送结束。
                assert_eq!(x, 1);
                kernel::task_end_scheduler();
            }

        }
    };
    //创建这两个任务。
    let _sender_task = task_control::TCB::new()
        .name("Sender")
        .priority(3)
        .initialise(sender);
    let _receiver_task = task_control::TCB::new()
        .name("Receiver")
        .priority(4)
        .initialise(receiver);
        
    kernel::task_start_scheduler();



}