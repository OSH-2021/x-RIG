use std::ptr::*;
use std::sync::{Arc, RwLock};
use crate::task_control_cap::*;
use crate::types::*;
use crate::arch_structures_TCB::*;
use crate::CNode::*;
use crate::CSpace::*;
use crate::regs::*;
use crate::*;

extern "C" {
    static mut current_extra_caps: extra_caps_t;
    static mut current_fault: seL4_Fault_t;
}

impl TaskHandle {
    pub unsafe fn setupCallerCap(sender: Self, receiver: Self) {
        // setThreadState(sender, _thread_state::BlockedOnReply);
        let sender_ptr = get_ptr_from_handle!(sender);
        let receiver_ptr = get_ptr_from_handle!(receiver);
        sender.set_state(_thread_state::BlockedOnReply);
        let sender_tcb = get_tcb_from_handle_mut!(sender);
        let receiver_tcb = get_tcb_from_handle_mut!(receiver);
        // let sender_ptr = Arc::as_ptr(&sender.0);
        let replySlot = tcb_ptr_cte_ptr(sender_ptr, tcb_cnode_index::tcbReply as u64);
        let callerSlot = tcb_ptr_cte_ptr(receiver_ptr, tcb_cnode_index::tcbCaller as u64);
        cteInsert(
            cap_reply_cap_new(0u64, sender_ptr as u64),
            Arc::new(RwLock::new(*replySlot)),
            Arc::new(RwLock::new(*callerSlot)),
        );
    }

pub unsafe fn deleteCallerCap(receiver: Self) {
    let receiver_ptr = get_ptr_from_handle!(receiver);
    let callerSlot = tcb_ptr_cte_ptr(receiver_ptr, tcb_cnode_index::tcbCaller as u64);
    cteDeleteOne(Arc::new(RwLock::new(*callerSlot)));
}

// pub unsafe fn lookupExtraCaps(
pub unsafe fn lookupExtraCaps(
    thread: &mut Self,
    bufferPtr: *mut u64,
    info: seL4_MessageInfo_t,
) -> u64 {
    let thread_ptr = get_ptr_from_handle!(thread);
    if bufferPtr as u64 == 0u64 {
        current_extra_caps.excaprefs[0] = Box::from_raw(null_mut());
        return 0u64;
    }
    let length = seL4_MessageInfo_get_extraCaps(info);
    let mut i: usize = 0;
    while i < length as usize {
        let cptr = getExtraCPtr(bufferPtr, i as u64);
        let lu_ret = lookupSlot(thread, cptr);
        if lu_ret.status != 0u64 {
            current_fault = seL4_Fault_CapFault_new(cptr, 0u64);
            return lu_ret.status;
        }
        current_extra_caps.excaprefs[i] = lu_ret.slot;
        i += 1;
    }
    if i < seL4_MsgMaxExtraCaps {
        current_extra_caps.excaprefs[i] = Box::from_raw(null_mut());
    }
    0u64
}

// pub unsafe fn copyMRs(
pub unsafe fn copyMRs(
    sender: *mut tcb_t,
    sendBuf: *mut u64,
    receiver: *mut tcb_t,
    recvBuf: *mut u64,
    n: u64,
) -> u64 {
    let mut i: usize = 0;
    while i < n as usize && i < n_msgRegisters {
        setRegister(
            receiver,
            msgRegisters[i],
            getRegister(sender, msgRegisters[i]),
        );
        i += 1;
    }
    if recvBuf as u64 == 0u64 || sendBuf as u64 == 0u64 {
        return i as u64;
    }
    while i < n as usize {
        *recvBuf.offset((i + 1) as isize) = *sendBuf.offset((i + 1) as isize);
        i += 1;
    }
    i as u64
}
}


//  syscall.rs
//  得到当前线程第i个消息寄存器(IPC Buffer)字
#[inline]
pub unsafe fn getSyscallArg(i: u64, ipc_buffer: *mut u64) -> u64 {
    if (i as usize) < n_msgRegisters {
        return getRegister(get_ptr_from_handle!(get_current_task_handle!()), msgRegisters[i as usize]);
    }
    *ipc_buffer.offset((i + 1) as isize)
}


#[no_mangle]
pub unsafe fn getExtraCPtr(bufferPtr: *mut u64, i: u64) -> u64 {
    *bufferPtr.offset((seL4_MsgMaxLength + 2 + i) as isize)
}