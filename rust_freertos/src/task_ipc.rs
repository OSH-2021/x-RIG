use crate::task_control_cap::*;
use crate::types::*;
use crate::arch_structures_TCB::*;
use crate::CNode::*;
use crate::CSpace::*;

pub unsafe fn setupCallerCap(sender: *mut tcb_t, receiver: *mut tcb_t) {
    setThreadState(sender, _thread_state::BlockedOnReply);
    let replySlot = tcb_ptr_cte_ptr(sender, tcb_cnode_index::tcbReply as u64);
    let callerSlot = tcb_ptr_cte_ptr(receiver, tcb_cnode_index::tcbCaller as u64);
    cteInsert(
        cap_reply_cap_new(0u64, sender as u64),
        replySlot,
        callerSlot,
    );
}

pub unsafe fn deleteCallerCap(receiver: *mut tcb_t) {
    let callerSlot = tcb_ptr_cte_ptr(receiver, tcb_cnode_index::tcbCaller as u64);
    cteDeleteOne(callerSlot);
}

// pub unsafe extern "C" fn lookupExtraCaps(
pub unsafe fn lookupExtraCaps(
    thread: *mut tcb_t,
    bufferPtr: *mut u64,
    info: seL4_MessageInfo_t,
) -> u64 {
    if bufferPtr as u64 == 0u64 {
        current_extra_caps.excaprefs[0] = 0u64 as *mut cte_t;
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
        current_extra_caps.excaprefs[i] = 0u64 as *mut cte_t;
    }
    0u64
}

// pub unsafe extern "C" fn copyMRs(
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


//  syscall.rs
//  得到当前线程第i个消息寄存器(IPC Buffer)字
#[inline]
pub unsafe fn getSyscallArg(i: u64, ipc_buffer: *mut u64) -> u64 {
    if (i as usize) < n_msgRegisters {
        return getRegister(node_state!(ksCurThread), msgRegisters[i as usize]);
    }
    *ipc_buffer.offset((i + 1) as isize)
}

