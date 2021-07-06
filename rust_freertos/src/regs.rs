#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
use crate::types::{word_t, tcb_t};

pub unsafe fn setRegister(thread: *mut tcb_t, reg_index: usize, w: word_t) {
    (*thread).registers[reg_index] = w;
}
pub unsafe fn getRegister(thread: *mut tcb_t, reg_index: usize) -> word_t {
    (*thread).registers[reg_index]
}

pub const seL4_FastMessageRegisters: usize = 4;
pub const n_msgRegisters: usize = 4;
pub const n_frameRegisters: usize = 18;
pub const n_gpRegisters: usize = 1;
pub const n_exceptionMessage: usize = 3;
pub const n_syscallMessage: usize = 18;

pub const RDI: usize = 0;
pub const capRegister: usize = 0;
pub const badgeRegister: usize = 0;
pub const RSI: usize = 0;
pub const msgInfoRegister: usize = 1;
pub const RAX: usize = 2;
pub const RBX: usize = 3;
pub const RBP: usize = 4;
pub const R12: usize = 5;
pub const R13: usize = 6;
pub const R14: usize = 7;
pub const RDX: usize = 8;
pub const R10: usize = 9;
pub const R8: usize = 10;
pub const R9: usize = 11;
pub const R15: usize = 12;
pub const FLAGS: usize = 13;
pub const NextIP: usize = 14;
pub const Error: usize = 15;
pub const RSP: usize = 16;
pub const TLS_BASE: usize = 17;
pub const FaultIP: usize = 18;
pub const R11: usize = 19;
pub const RCX: usize = 20;
pub const CS: usize = 21;
pub const SS: usize = 22;
pub const n_contextRegisters: usize = 23;

pub const msgRegisters: [usize; 4] = [R10, R8, R9, R15];
pub const frameRegisters: [usize; 18] = [
    FaultIP, RSP, FLAGS, RAX, RBX, RCX, RDX, RSI, RDI, RBP, R8, R9, R10, R11, R12, R13, R14, R15,
];
pub const gpRegisters: [usize; 1] = [TLS_BASE];
