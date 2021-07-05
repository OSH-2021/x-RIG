//  this file is used to transform types between seL4 and freertos
use crate::task_global::*;
use crate::kernel::*;
use crate::list::*;
use crate::port::*;
use std::sync::{Arc, RwLock, Weak};
use crate::seL4::*;

pub type word_t = UBaseType;
pub type prio_t = UBaseType;

#[derive(Copy, Clone)]
pub struct cap_t {
    pub words: [u64; 2],
}

#[derive(Copy, Clone)]
pub struct cte_t {
    pub cap: cap_t,
    pub cteMDBNode: mdb_node_t,
}

