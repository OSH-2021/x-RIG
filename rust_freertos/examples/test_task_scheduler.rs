extern crate rust_freertos;

use rust_freertos::*;
#[cfg(feature = "configUSE_CAPS")]
use rust_freertos::task_control_cap::*;
#[cfg(not(feature = "configUSE_CAPS"))]
use rust_freertos::task_control::*;

fn main() {
    let t0 = move || {
        loop {
            println!("Task 0 running!");
        }
    };

    let t1 = move || {
        loop {
            println!("Task 1 running!");
        }
    };

    let t2 = move || {
        loop {
            println!("Task 2 running!");
        }
    };

    let _task0 = TCB::new()
        .name("Task0")
        .priority(3)
        .initialise(t0);
    let _task1 = TCB::new()
        .name("Task1")
        .priority(3)
        .initialise(t1);
    let _task2 = TCB::new()
        .name("Task2")
        .priority(3)
        .initialise(t2);
    kernel::task_start_scheduler();
}
