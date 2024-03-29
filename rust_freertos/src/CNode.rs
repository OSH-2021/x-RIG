#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(non_upper_case_globals)]
#![allow(unused_attributes)]
use crate::projdefs::FreeRtosError;
use crate::arch_structures_TCB::*;
use crate::task_control_cap::*;
use crate::task_ipc::*;
use crate::types::*;
use crate::CSpace::*;
use crate::*;
use std::sync::{Arc, RwLock};
use std::ptr::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CTable {
    pub caps: [cte_t; MAX_CSlots],
}

#[derive(Debug, Clone)]
pub struct slot_range_t {
    pub cnode: Arc<RwLock<cte_t>>,
    pub offset: u64,
    pub length: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct finaliseSlot_ret_t {
    pub status: u64,
    pub success: bool_t,
    pub cleanupInfo: cap_t,
}

extern "C" {
    pub static mut current_syscall_error: syscall_error_t;
    pub static mut current_lookup_fault: lookup_fault_t;
    fn preemptionPoint() -> u64;
    fn finaliseCap(cap: cap_t, final_: bool_t, exposed: bool_t) -> finaliseCap_ret_t;
    fn sameRegionAs(cap_a: cap_t, cap_b: cap_t) -> bool_t;
    fn sameObjectAs(cap_a: cap_t, cap_b: cap_t) -> bool_t;
    fn cancelBadgedSends(epptr: *mut endpoint_t, badge: u64);
    fn maskCapRights(cap_rights: seL4_CapRights_t, cap: cap_t) -> cap_t;
    fn kprintf(format: *const u8, ...) -> u64;
    fn puts(str: *const u8) -> u64;
    fn deletedIRQHandler(irq: u8); //  where to place? TODO
    fn Arch_postCapDeletion(cap: cap_t); //  where to place? TODO
}

pub unsafe fn decodeCNodeInvocation(
    thread: &mut TaskHandle,
    invLabel: u64,
    length: u64,
    cap: cap_t,
    excaps: extra_caps_t,
    buffer: *mut u64,
) -> u64 {
    if invLabel < invocation_label::CNodeRevoke as u64
        || invLabel > invocation_label::CNodeSaveCaller as u64
    {
        userError!("CNodeCap: IllegalOperation attemped.");
        current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
        return exception::EXCEPTION_SYSCALL_ERROR as u64;
    }
    if length < 2u64 {
        userError!("CNode operation: Truncated message.");
        current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
        return exception::EXCEPTION_SYSCALL_ERROR as u64;
    }
    let index = getSyscallArg(0, buffer);
    let w_bits = getSyscallArg(1, buffer);
    let mut lu_ret = lookupTargetSlot(thread, cap, index, w_bits);
    if lu_ret.status != 0u64 {
        userError!("CNode operation: Target slot invalid.");
        return lu_ret.status;
    }
    let destSlot = lu_ret.slot;
    let dest_arc_lock = Arc::new(RwLock::new(*destSlot.clone()));
    if invLabel >= invocation_label::CNodeCopy as u64
        && invLabel <= invocation_label::CNodeMutate as u64
    {
        if length < 4 || Box::into_raw(excaps.excaprefs[0].clone()) as u64 == 0u64 {
            userError!("CNode Copy/Mint/Move/Mutate: Truncated message.");
            current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        let srcIndex = getSyscallArg(2, buffer);
        let srcDepth = getSyscallArg(3, buffer);
        let srcRoot = (*excaps.excaprefs[0]).cap;
        let status = ensureEmptySlot(Arc::new(RwLock::new(*destSlot)));
        let newCap: cap_t;
        let isMove: bool;
        if status != 0u64 {
            userError!("CNode Copy/Mint/Move/Mutate: Destination not empty.");
            return status;
        }
        lu_ret = lookupSourceSlot(thread, srcRoot, srcIndex, srcDepth);
        if lu_ret.status != 0u64 {
            userError!("CNode Copy/Mint/Move/Mutate: Invalid source slot.");
            return lu_ret.status;
        }
        let srcSlot = lu_ret.slot;
        let src_arc_lock = Arc::new(RwLock::new(*srcSlot));
        if cap_get_capType((*srcSlot).cap) == cap_tag_t::cap_null_cap as u64 {
            userError!("CNode Copy/Mint/Move/Mutate: Source slot invalid or empty.");
            current_syscall_error.type_ = seL4_Error::seL4_FailedLookup as u64;
            current_syscall_error.failedLookupWasSource = 1u64;
            current_lookup_fault = lookup_fault_missing_capability_new(srcDepth);
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        if invLabel == invocation_label::CNodeCopy as u64 {
            if length < 5 {
                userError!("Truncated message for CNode Copy operation.");
                current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
                return exception::EXCEPTION_SYSCALL_ERROR as u64;
            }
            let cap_rights = rightsFromWord(getSyscallArg(4, buffer));
            let srcCap = maskCapRights(cap_rights, (*srcSlot).cap);
            let dc_ret = deriveCap(srcSlot, srcCap);
            if dc_ret.status != 0u64 {
                userError!("Error deriving cap for CNode Copy operation.");
                return dc_ret.status;
            }
            newCap = dc_ret.cap;
            isMove = false;
        } else if invLabel == invocation_label::CNodeMint as u64 {
            if length < 6 {
                userError!("CNode Mint: Truncated message.");
                current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
                return exception::EXCEPTION_SYSCALL_ERROR as u64;
            }
            let cap_rights = rightsFromWord(getSyscallArg(4, buffer));
            let capData = getSyscallArg(5, buffer);
            let srcCap = maskCapRights(cap_rights, (*srcSlot).cap);
            let dc_ret = deriveCap(srcSlot, updateCapData(0u64, capData, srcCap));
            if dc_ret.status != 0u64 {
                userError!("Erro deriving cap for CNode Mint operation.");
                return dc_ret.status;
            }
            newCap = dc_ret.cap;
            isMove = false;
        } else if invLabel == invocation_label::CNodeMove as u64 {
            newCap = (*srcSlot).cap;
            isMove = true;
        } else if invLabel == invocation_label::CNodeMutate as u64 {
            if length < 5 {
                userError!("CNode Mutate: Truncated message.");
                current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
                return exception::EXCEPTION_SYSCALL_ERROR as u64;
            }
            let capData = getSyscallArg(4, buffer);
            newCap = updateCapData(1u64, capData, (*srcSlot).cap);
            isMove = true;
        } else {
            panic!();
        }
        if cap_get_capType(newCap) == cap_tag_t::cap_null_cap as u64 {
            userError!("CNode Copy/Mint/Move/Mutate: Mutated cap would be invalid.");
            current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }

        //    setThreadState(
        //        node_state!(get_ptr_from_handle!(get_current_task_handle!())),
        //        _thread_state::Restart,
        //    );
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        if isMove { // Arc::new(RwLock::new(srcSlot))
            invokeCNodeMove(newCap, src_arc_lock.clone(), dest_arc_lock.clone());
        } else {
            invokeCNodeInsert(newCap, src_arc_lock, dest_arc_lock.clone());
        }
    }
    if invLabel == invocation_label::CNodeRevoke as u64 {
        // setThreadState(node_state!(get_ptr_from_handle!(get_current_task_handle!())), _thread_state::Restart);
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        return invokeCNodeRevoke(dest_arc_lock.clone());
    } else if invLabel == invocation_label::CNodeDelete as u64 {
        // setThreadState(node_state!(get_ptr_from_handle!(get_current_task_handle!())), _thread_state::Restart);
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        return invokeCNodeDelete(dest_arc_lock);
    } else if invLabel == invocation_label::CNodeSaveCaller as u64 {
        let status = ensureEmptySlot(dest_arc_lock.clone());
        if status != 0u64 {
            userError!("CNode SaveCaller: Destination slot not empty.");
            return status;
        }
        // setThreadState(node_state!(get_ptr_from_handle!(get_current_task_handle!())), _thread_state::Restart);
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        return invokeCNodeSaveCaller(dest_arc_lock.clone());
    } else if invLabel == invocation_label::CNodeCancelBadgedSends as u64 {
        let destCap = (*destSlot).cap;
        if hasCancelSendRights(destCap) == 0u64 {
            userError!("CNode CancelBadgedSends: Target cap invalid.");
            current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        // setThreadState(node_state!(get_ptr_from_handle!(get_current_task_handle!())), _thread_state::Restart);
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        return invokeCNodeCancelBadgedSends(destCap);
    } else if invLabel == invocation_label::CNodeRotate as u64 {
        if length < 8 || Box::into_raw(excaps.excaprefs[0].clone()) as u64 == 0u64 || Box::into_raw(excaps.excaprefs[1].clone()) as u64 == 0u64 {
            current_syscall_error.type_ = seL4_Error::seL4_TruncatedMessage as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        let pivotNewData = getSyscallArg(2, buffer);
        let pivotIndex = getSyscallArg(3, buffer);
        let pivotDepth = getSyscallArg(4, buffer);
        let srcNewData = getSyscallArg(5, buffer);
        let srcIndex = getSyscallArg(6, buffer);
        let srcDepth = getSyscallArg(7, buffer);
        let pivotRoot = (*excaps.excaprefs[0]).cap;
        let srcRoot = (*excaps.excaprefs[1]).cap;
        let mut lu_ret = lookupSourceSlot(thread, srcRoot, srcIndex, srcDepth);
        if lu_ret.status != 0u64 {
            return lu_ret.status;
        }
        let srcSlot = lu_ret.slot;
        let src_arc_lock = Arc::new(RwLock::new(*srcSlot));
        lu_ret = lookupPivotSlot(thread, pivotRoot, pivotIndex, pivotDepth);
        if lu_ret.status != 0u64 {
            return lu_ret.status;
        }
        let pivotSlot = lu_ret.slot;
        if pivotSlot == srcSlot || pivotSlot == destSlot {
            userError!("CNode Rotate: Pivot slot the same as source or dest slot");
            current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        if srcSlot != destSlot {
            let status = ensureEmptySlot(dest_arc_lock.clone());
            if status != 0u64 {
                return status;
            }
        }
        if cap_get_capType((*srcSlot).cap) == cap_tag_t::cap_null_cap as u64 {
            current_syscall_error.type_ = seL4_Error::seL4_FailedLookup as u64;
            current_syscall_error.failedLookupWasSource = 1;
            current_lookup_fault = lookup_fault_missing_capability_new(srcDepth);
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        if cap_get_capType((*pivotSlot).cap) == cap_tag_t::cap_null_cap as u64 {
            current_syscall_error.type_ = seL4_Error::seL4_FailedLookup as u64;
            current_syscall_error.failedLookupWasSource = 1;
            current_lookup_fault = lookup_fault_missing_capability_new(pivotDepth);
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        let newSrcCap = updateCapData(1u64, srcNewData, (*srcSlot).cap);
        let newPivotCap = updateCapData(1u64, pivotNewData, (*pivotSlot).cap);
        if cap_get_capType(newSrcCap) == cap_tag_t::cap_null_cap as u64 {
            userError!("CNode Rotate: Source cap invalid.");
            current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        if cap_get_capType(newPivotCap) == cap_tag_t::cap_null_cap as u64 {
            userError!("CNode Rotate: Pivot cap invalid.");
            current_syscall_error.type_ = seL4_Error::seL4_IllegalOperation as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
        // setThreadState(node_state!(get_ptr_from_handle!(get_current_task_handle!())), _thread_state::Restart);
        let current_task = get_current_task_handle!();
        current_task.set_state(_thread_state::Restart);
        return invokeCNodeRotate(newSrcCap, newPivotCap, src_arc_lock, Arc::new(RwLock::new(*pivotSlot.clone())), dest_arc_lock.clone());
    }
    0u64
}

#[no_mangle]
pub unsafe fn invokeCNodeRevoke(destSlot: Arc<RwLock<cte_t>>) -> u64 {
    cteRevoke(destSlot)
}

#[no_mangle]
pub unsafe fn invokeCNodeDelete(destSlot: Arc<RwLock<cte_t>>) -> u64 {
    cteDelete(destSlot, 1u64)
}

#[no_mangle]
pub unsafe fn invokeCNodeCancelBadgedSends(cap: cap_t) -> u64 {
    let badge = cap_endpoint_cap_get_capEPBadge(cap);
    if badge != 0u64 {
        // let ep = Arc::from_raw(cap_endpoint_cap_get_capEPPtr(cap) as *mut endpoint_t);
        cancelBadgedSends(cap_endpoint_cap_get_capEPPtr(cap) as *mut endpoint_t, badge);
    }
    0u64
}

#[no_mangle]
pub unsafe fn invokeCNodeInsert(
    cap: cap_t,
    srcSlot: Arc<RwLock<cte_t>>,
    destSlot: Arc<RwLock<cte_t>>,
) -> u64 {
    cteInsert(cap, srcSlot, destSlot);
    0u64
}

#[no_mangle]
pub unsafe fn invokeCNodeMove(
    cap: cap_t,
    srcSlot: Arc<RwLock<cte_t>>,
    destSlot: Arc<RwLock<cte_t>>,
) -> u64 {
    cteMove(cap, srcSlot, destSlot);
    0u64
}

#[no_mangle]
pub unsafe fn invokeCNodeRotate(
    cap1: cap_t,
    cap2: cap_t,
    slot1: Arc<RwLock<cte_t>>,
    slot2: Arc<RwLock<cte_t>>,
    slot3: Arc<RwLock<cte_t>>,
) -> u64 {
    if Arc::ptr_eq(&slot1, &slot3) {
        cteSwap(cap1, slot1.clone(), cap2, slot2.clone());
    } else {
        cteMove(cap2, slot2.clone(), slot3.clone());
        cteMove(cap1, slot1.clone(), slot2.clone());
    }
    0u64
}

#[no_mangle]
pub unsafe fn invokeCNodeSaveCaller(destSlot: Arc<RwLock<cte_t>>) -> u64 {
    let srcSlot = tcb_ptr_cte_ptr(get_ptr_from_handle!(get_current_task_handle!()), tcb_cnode_index::tcbCaller as u64);
    let cap = (*srcSlot).cap;
    let srcSlot = Arc::from_raw(srcSlot);
    let cap_type = cap_get_capType(cap);
    if cap_type == cap_tag_t::cap_null_cap as u64 {
        //userError!("CNode SaveCaller: Reply cap not present.")
    } else if cap_type == cap_tag_t::cap_reply_cap as u64 {
        if cap_reply_cap_get_capReplyMaster(cap) == 0u64 {
            cteMove(cap, Arc::new(RwLock::new(*srcSlot.clone())), destSlot.clone());
        }
    } else {
        panic!("caller capability must be null or reply");
    }
    0u64
}

unsafe fn setUntypedCapAsFull(srcCap: cap_t, newCap: cap_t, srcSlot: Arc<RwLock<cte_t>>) {
    if cap_get_capType(srcCap) == cap_tag_t::cap_untyped_cap as u64
        && cap_get_capType(newCap) == cap_tag_t::cap_untyped_cap as u64
    {
        if cap_untyped_cap_get_capPtr(srcCap) == cap_untyped_cap_get_capPtr(newCap)
            && cap_untyped_cap_get_capBlockSize(newCap) == cap_untyped_cap_get_capBlockSize(srcCap)
        {
            cap_untyped_cap_ptr_set_capFreeIndex(
                &mut srcSlot.write().unwrap().cap,
                (1 << cap_untyped_cap_get_capBlockSize(srcCap)) - 4,
            );
        }
    }
}

#[no_mangle]
pub unsafe fn cteInsert(newCap: cap_t, srcSlot: Arc<RwLock<cte_t>>, destSlot: Arc<RwLock<cte_t>>) {
    // owned
    let srcMDB: mdb_node_t = srcSlot.clone().read().unwrap().cteMDBNode; // (*srcSlot).cteMDBNode;
    let srcCap: cap_t = srcSlot.clone().read().unwrap().cap; // (*srcSlot).cap;
    let newCapIsRevocable: u64 = isCapRevocable(newCap, srcCap);
    let mut newMDB = mdb_node_set_mdbPrev(srcMDB, Arc::as_ptr(&srcSlot) as u64);
    newMDB = mdb_node_set_mdbRevocable(newMDB, newCapIsRevocable);
    newMDB = mdb_node_set_mdbFirstBadged(newMDB, newCapIsRevocable);
    setUntypedCapAsFull(srcCap, newCap, srcSlot.clone());
    // (*destSlot).cap = newCap;
    destSlot.write().unwrap().cap = newCap;
    // (*destSlot) = newMDB;
    destSlot.write().unwrap().cteMDBNode = newMDB;
    mdb_node_ptr_set_mdbNext(&mut srcSlot.write().unwrap().cteMDBNode, Arc::as_ptr(&destSlot) as u64);
    if mdb_node_get_mdbNext(newMDB) != 0u64 {
        mdb_node_ptr_set_mdbPrev(
            &mut (*(mdb_node_get_mdbNext(newMDB) as *mut cte_t)).cteMDBNode,
            Arc::as_ptr(&destSlot) as u64
        );
    }
}

#[no_mangle]
pub unsafe fn cteMove(newCap: cap_t, srcSlot: Arc<RwLock<cte_t>>, destSlot: Arc<RwLock<cte_t>>) {
    let mdb: mdb_node_t = srcSlot.clone().read().unwrap().cteMDBNode; // (*srcSlot).cteMDBNode;
    destSlot.write().unwrap().cap = newCap;
    srcSlot.write().unwrap().cap = cap_null_cap_new();
    destSlot.write().unwrap().cteMDBNode = mdb;
    srcSlot.write().unwrap().cteMDBNode = mdb_node_new(0, 0, 0, 0);
    let prev_ptr: u64 = mdb_node_get_mdbPrev(mdb);
    if prev_ptr != 0u64 {
        mdb_node_ptr_set_mdbNext(&mut (*(prev_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&destSlot) as u64);
    }
    let next_ptr: u64 = mdb_node_get_mdbNext(mdb);
    if next_ptr != 0u64 {
        mdb_node_ptr_set_mdbPrev(&mut (*(next_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&destSlot) as u64);
    }
}

#[no_mangle]
pub unsafe fn capSwapForDelete(slot1: Arc<RwLock<cte_t>>, slot2: Arc<RwLock<cte_t>>) {
    if Arc::ptr_eq(&slot1, &slot2) {
        return;
    }
    let cap1 = slot1.clone().read().unwrap().cap;
    let cap2 = slot2.clone().read().unwrap().cap;
    cteSwap(cap1, slot1, cap2, slot2);
}

#[no_mangle]
pub unsafe fn cteSwap(cap1: cap_t, slot1: Arc<RwLock<cte_t>>, cap2: cap_t, slot2: Arc<RwLock<cte_t>>) {
    slot1.write().unwrap().cap = cap2;
    slot2.write().unwrap().cap = cap1;
    let mdb1: mdb_node_t = slot1.read().unwrap().cteMDBNode;
    let mut prev_ptr: u64 = mdb_node_get_mdbPrev(mdb1);
    if prev_ptr != 0u64 {
        mdb_node_ptr_set_mdbNext(&mut (*(prev_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&slot2) as u64);
    }
    let mut next_ptr: u64 = mdb_node_get_mdbNext(mdb1);
    if next_ptr != 0u64 {
        mdb_node_ptr_set_mdbPrev(&mut (*(next_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&slot2) as u64);
    }
    let mdb2: mdb_node_t = slot2.read().unwrap().cteMDBNode;
    slot1.write().unwrap().cteMDBNode = mdb2;
    slot2.write().unwrap().cteMDBNode = mdb1;

    prev_ptr = mdb_node_get_mdbPrev(mdb2);
    if prev_ptr != 0u64 {
        mdb_node_ptr_set_mdbNext(&mut (*(prev_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&slot1) as u64);
    }
    next_ptr = mdb_node_get_mdbNext(mdb2);
    if next_ptr != 0u64 {
        mdb_node_ptr_set_mdbPrev(&mut (*(next_ptr as *mut cte_t)).cteMDBNode, Arc::as_ptr(&slot1) as u64);
    }
}

#[no_mangle]
pub unsafe fn cteRevoke(slot: Arc<RwLock<cte_t>>) -> u64 {
    let mut ret = mdb_node_get_mdbNext(slot.read().unwrap().cteMDBNode) as *mut cte_t;
    let mut nextPtr: Arc<RwLock<cte_t>> = Arc::new(RwLock::new(*ret));
    while Arc::as_ptr(&nextPtr.clone()) as u64 != 0u64 && isMDBParentOf(slot.clone(), nextPtr.clone()) != 0u64 {
        let mut status: u64 = cteDelete(nextPtr.clone(), true as u64);
        if status != 0u64 {
            return status;
        }
        status = preemptionPoint();
        if status != 0u64 {
            return status;
        }
        ret = mdb_node_get_mdbNext(slot.write().unwrap().cteMDBNode) as *mut cte_t;
        // nextPtr = Arc::from_raw();
        *nextPtr.write().unwrap() = *ret;
    }
    0u64
}

#[no_mangle]
pub unsafe fn cteDelete(slot: Arc<RwLock<cte_t>>, exposed: bool_t) -> u64 {
    let fs_ret: finaliseSlot_ret_t = finaliseSlot(slot.clone(), exposed);
    if fs_ret.status != 0u64 {
        return fs_ret.status;
    }
    if exposed != 0u64 || fs_ret.success != 0u64 {
        emptySlot(slot.clone(), fs_ret.cleanupInfo);
    }
    0u64
}

#[no_mangle]
pub unsafe fn emptySlot(slot: Arc<RwLock<cte_t>>, cleanupInfo: cap_t) {
    if cap_get_capType(slot.read().unwrap().cap) != cap_tag_t::cap_null_cap as u64 {
        let mdbNode: mdb_node_t = slot.read().unwrap().cteMDBNode; // (*slot).cteMDBNode;
        let prev = Arc::new(RwLock::new(*(mdb_node_get_mdbPrev(mdbNode) as *mut cte_t)));
        let next = Arc::new(RwLock::new(*(mdb_node_get_mdbNext(mdbNode) as *mut cte_t)));
        if Arc::as_ptr(&prev) as u64 != 0u64 {
            mdb_node_ptr_set_mdbNext(&mut prev.write().unwrap().cteMDBNode, Arc::as_ptr(&next) as u64);
        }
        if Arc::as_ptr(&next) as u64 != 0u64 {
            mdb_node_ptr_set_mdbPrev(&mut next.write().unwrap().cteMDBNode, Arc::as_ptr(&prev) as u64);
        }
        if Arc::as_ptr(&next) as u64 != 0u64 {
            mdb_node_ptr_set_mdbFirstBadged(
                &mut next.write().unwrap().cteMDBNode,
                mdb_node_get_mdbFirstBadged(next.write().unwrap().cteMDBNode)
                    | mdb_node_get_mdbFirstBadged(mdbNode),
            );
        }
        slot.write().unwrap().cteMDBNode = mdb_node_new(0, 0, 0, 0);
        slot.write().unwrap().cap = cap_null_cap_new();
        postCapDeletion(cleanupInfo);
    }
}

#[inline]
unsafe fn capRemovable(cap: cap_t, slot: Arc<RwLock<cte_t>>) -> bool {
    let cap_type = cap_get_capType(cap);
    if cap_type == cap_tag_t::cap_null_cap as u64 {
        return true;
    } else if cap_type == cap_tag_t::cap_zombie_cap as u64 {
        let n = cap_zombie_cap_get_capZombieNumber(cap);
        let ret = cap_zombie_cap_get_capZombiePtr(cap) as *mut cte_t;
        let z_slot = Arc::new(RwLock::new(*ret));
        return n == 0 || (n == 1 && Arc::ptr_eq(&slot, &z_slot));
    }
    panic!("finaliseCap should only return Zombie or NullCap")
}

#[inline]
unsafe fn capCyclicZombie(cap: cap_t, slot: Arc<RwLock<cte_t>>) -> bool {
    let ret = cap_zombie_cap_get_capZombiePtr(cap) as *mut cte_t;

    cap_get_capType(cap) == cap_tag_t::cap_zombie_cap as u64
        && Arc::ptr_eq(&Arc::new(RwLock::new(*ret)), &slot)
}

unsafe fn finaliseSlot(slot: Arc<RwLock<cte_t>>, immediate: bool_t) -> finaliseSlot_ret_t {
    while cap_get_capType(slot.clone().read().unwrap().cap) != cap_tag_t::cap_null_cap as u64 {
        let final_: u64 = isFinalCapability(slot.clone());
        let fc_ret = finaliseCap(slot.clone().read().unwrap().cap, final_, 0u64);
        if capRemovable(fc_ret.remainder, slot.clone()) {
            return finaliseSlot_ret_t {
                status: 0u64,
                success: 1u64,
                cleanupInfo: fc_ret.cleanupInfo,
            };
        }
        slot.clone().write().unwrap().cap = fc_ret.remainder;
        if immediate == 0u64 && capCyclicZombie(fc_ret.remainder, slot.clone()) {
            return finaliseSlot_ret_t {
                status: 0u64,
                success: 0u64,
                cleanupInfo: fc_ret.cleanupInfo,
            };
        }
        let mut status = reduceZombie(slot.clone(), immediate);
        if status != 0u64 {
            return finaliseSlot_ret_t {
                status: status,
                success: 0u64,
                cleanupInfo: cap_null_cap_new(),
            };
        }
        status = preemptionPoint();
        if status != 0u64 {
            return finaliseSlot_ret_t {
                status: status,
                success: 0u64,
                cleanupInfo: cap_null_cap_new(),
            };
        }
    }
    finaliseSlot_ret_t {
        status: 0u64,
        success: 1u64,
        cleanupInfo: cap_null_cap_new(),
    }
}

unsafe fn reduceZombie(slot: Arc<RwLock<cte_t>>, immediate: bool_t) -> u64 {
    let ptr = Arc::from_raw(cap_zombie_cap_get_capZombiePtr(slot.read().unwrap().cap) as *mut cte_t);
    let n = cap_zombie_cap_get_capZombieNumber(slot.read().unwrap().cap);
    let type_ = cap_zombie_cap_get_capZombieType(slot.read().unwrap().cap);
    if immediate == 1u64 {
        let endSlot = Arc::from_raw(Arc::as_ptr(&ptr).offset((n - 1) as isize));
        let status = cteDelete(Arc::new(RwLock::new(*endSlot)), 0u64);
        if status != 0u64 {
            return status;
        }
        let cap_type = cap_get_capType(slot.read().unwrap().cap);
        if cap_type == cap_tag_t::cap_null_cap as u64 {
        } else if cap_type == cap_tag_t::cap_zombie_cap as u64 {
            let ptr2 = Arc::from_raw(cap_zombie_cap_get_capZombiePtr(slot.read().unwrap().cap) as *mut cte_t);
            if ptr == ptr2
                && cap_zombie_cap_get_capZombieNumber(slot.read().unwrap().cap) == n
                && cap_zombie_cap_get_capZombieType(slot.read().unwrap().cap) == type_
            {
                slot.write().unwrap().cap = cap_zombie_cap_set_capZombieNumber(slot.read().unwrap().cap, n - 1);
            }
        } else {
            panic!("Expected recursion to result in Zombie.");
        }
    } else {
        capSwapForDelete(Arc::new(RwLock::new(*ptr)), slot);
    }
    0u64
}

// #[allow(unused_variables)]
#[no_mangle]
pub unsafe fn cteDeleteOne(slot: Arc<RwLock<cte_t>>) {
    let cap_type = cap_get_capType(slot.clone().read().unwrap().cap);
    if cap_type != cap_tag_t::cap_null_cap as u64 {
        let final_ = isFinalCapability(slot.clone());
        let fc_ret = finaliseCap(slot.clone().read().unwrap().cap, final_, 1u64);
        emptySlot(slot, cap_null_cap_new());
    }
}

#[no_mangle]
pub unsafe fn insertNewCap(parent: Arc<RwLock<cte_t>>, slot: Arc<RwLock<cte_t>>, cap: cap_t) {
    let next = Arc::new(RwLock::new(*(mdb_node_get_mdbNext(parent.read().unwrap().cteMDBNode) as *mut cte_t)));
    slot.clone().write().unwrap().cap = cap;
    slot.clone().write().unwrap().cteMDBNode = mdb_node_new(Arc::as_ptr(&next) as u64, 1u64, 1u64, Arc::as_ptr(&parent) as u64);
    if Arc::as_ptr(&next) as u64 != 0u64 {
        mdb_node_ptr_set_mdbPrev(&mut next.write().unwrap().cteMDBNode, Arc::as_ptr(&slot) as u64);
    }
    mdb_node_ptr_set_mdbNext(&mut parent.write().unwrap().cteMDBNode, Arc::as_ptr(&slot) as u64);
}

#[no_mangle]
pub unsafe fn setupReplyMaster(thread: &mut TaskHandle) {
    let thread_ptr = get_ptr_from_handle!(thread);
    let slot = tcb_ptr_cte_ptr(thread_ptr, tcb_cnode_index::tcbReply as u64);
    if cap_get_capType((*slot).cap) == cap_tag_t::cap_null_cap as u64 {
        (*slot).cap = cap_reply_cap_new(1u64, thread_ptr as u64);
        (*slot).cteMDBNode = mdb_node_new(0, 0, 0, 0);
        mdb_node_ptr_set_mdbRevocable(&mut (*slot).cteMDBNode, 1u64);
        mdb_node_ptr_set_mdbFirstBadged(&mut (*slot).cteMDBNode, 1u64);
    }
}

#[no_mangle]
pub unsafe fn isMDBParentOf(cte_a: Arc<RwLock<cte_t>>, cte_b: Arc<RwLock<cte_t>>) -> bool_t {
    if mdb_node_get_mdbRevocable(cte_a.read().unwrap().cteMDBNode) == 0u64 {
        return 0u64;
    }
    if sameRegionAs(cte_a.read().unwrap().cap, cte_b.read().unwrap().cap) == 0u64 {
        return 0u64;
    }
    let cap_type = cap_get_capType(cte_a.read().unwrap().cap);
    if cap_type == cap_tag_t::cap_endpoint_cap as u64 {
        let badge = cap_endpoint_cap_get_capEPBadge(cte_a.read().unwrap().cap);
        if badge == 0u64 {
            return 1u64;
        }
        return ((badge == cap_endpoint_cap_get_capEPBadge(cte_a.read().unwrap().cap))
            && mdb_node_get_mdbFirstBadged(cte_b.read().unwrap().cteMDBNode) == 0u64) as u64;
    } else if cap_type == cap_tag_t::cap_notification_cap as u64 {
        let badge = cap_notification_cap_get_capNtfnBadge(cte_a.read().unwrap().cap);
        if badge == 0u64 {
            return 1u64;
        }
        return ((badge == cap_notification_cap_get_capNtfnBadge(cte_b.read().unwrap().cap))
            && mdb_node_get_mdbFirstBadged(cte_b.read().unwrap().cteMDBNode) == 0u64) as u64;
    }
    1u64
}

#[no_mangle]
pub unsafe fn ensureNoChildren(slot: Arc<RwLock<cte_t>>) -> u64 {
    if mdb_node_get_mdbNext(slot.read().unwrap().cteMDBNode) != 0u64 {
        let next = Arc::from_raw(mdb_node_get_mdbNext(slot.read().unwrap().cteMDBNode) as *mut cte_t);
        if isMDBParentOf(slot, Arc::new(RwLock::new(*next))) != 0u64 {
            current_syscall_error.type_ = seL4_Error::seL4_RevokeFirst as u64;
            return exception::EXCEPTION_SYSCALL_ERROR as u64;
        }
    }
    return 0u64;
}

#[no_mangle]
pub unsafe fn ensureEmptySlot(slot: Arc<RwLock<cte_t>>) -> u64 {
    if cap_get_capType(slot.read().unwrap().cap) != cap_tag_t::cap_null_cap as u64 {
        current_syscall_error.type_ = seL4_Error::seL4_DeleteFirst as u64;
        return exception::EXCEPTION_SYSCALL_ERROR as u64;
    }
    return 0u64;
}

#[no_mangle]
pub unsafe fn isFinalCapability(cte: Arc<RwLock<cte_t>>) -> bool_t {
    let mdb = cte.read().unwrap().cteMDBNode;
    let prevIsSameObject: bool = if mdb_node_get_mdbPrev(mdb) == 0u64 {
        false
    } else {
        let prev = Arc::from_raw(mdb_node_get_mdbPrev(mdb) as *mut cte_t);
        sameObjectAs((*prev).cap, cte.read().unwrap().cap) == 1u64 // 奇行种呜呜呜
    };
    if prevIsSameObject {
        return 0u64;
    } else {
        if mdb_node_get_mdbNext(mdb) == 0u64 {
            return 1u64;
        } else {
            let next = Arc::from_raw(mdb_node_get_mdbNext(mdb) as *mut cte_t);
            return sameObjectAs(cte.read().unwrap().cap, (*next).cap);
        }
    }
}

#[no_mangle]
pub unsafe fn slotCapLongRunningDelete(slot: Arc<RwLock<cte_t>>) -> bool_t {
    let cap_type = cap_get_capType(slot.read().unwrap().cap);
    if cap_type == cap_tag_t::cap_null_cap as u64 || isFinalCapability(slot) == 0u64 {
        return 0u64;
    }
    if cap_type == cap_tag_t::cap_thread_cap as u64
        || cap_type == cap_tag_t::cap_zombie_cap as u64
        || cap_type == cap_tag_t::cap_cnode_cap as u64
    {
        return 1u64;
    }
    0u64
}

#[no_mangle]
pub unsafe fn getReceiveSlots(thread: &mut TaskHandle, buffer: *mut u64) -> Result<Arc<RwLock<cte_t>>, FreeRtosError> {
    let thread_ptr = get_ptr_from_handle!(thread);
    if buffer as u64 == 0u64 {
        return Err(FreeRtosError::Ajkaierdja);
    }
    let ct = loadCapTransfer(buffer);
    let cptr = ct.ctReceiveRoot;
    let luc_ret = lookupCap(thread, cptr);
    if luc_ret.status != 0u64 {
        return Err(FreeRtosError::Ajkaierdja);
    }
    let cnode = luc_ret.cap;
    let lus_ret = lookupTargetSlot(thread, cnode, ct.ctReceiveIndex, ct.ctReceiveDepth);
    if lus_ret.status != 0u64 {
        return Err(FreeRtosError::Ajkaierdja);
    }
    let slot = lus_ret.slot;
    if cap_get_capType((*slot).cap) != cap_tag_t::cap_null_cap as u64 {
        return Err(FreeRtosError::Ajkaierdja);
    }
    Ok(Arc::new(RwLock::new(*slot)))
}

#[no_mangle]
pub unsafe fn loadCapTransfer(buffer: *mut u64) -> cap_transfer_t {
    const offset: isize = (seL4_MsgMaxLength + seL4_MsgMaxExtraCaps as u64 + 2) as isize;
    capTransferFromWords(buffer.offset(offset))
}

// objecttype.rs
#[inline]
pub unsafe fn postCapDeletion(cap: cap_t) {
    if cap_get_capType(cap) == cap_tag_t::cap_irq_handler_cap as u64 {
        let irq: u8 = cap_irq_handler_cap_get_capIRQ(cap) as u8;
        deletedIRQHandler(irq);
    } else if isArchCap(cap) != 0u64 {
        Arch_postCapDeletion(cap);
    }
}
