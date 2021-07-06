//  this file is used to transform types between seL4 and freertos
use crate::task_global::*;
use crate::kernel::*;
use crate::list::*;
use crate::port::*;
use std::sync::{Arc, RwLock, Weak};
use crate::task_control_cap::*;
use crate::*;
use crate::CNode::*;
use crate::arch_structures_TCB::*;

pub type tcb_t = task_control_block;
pub const seL4_TCBBits: u64 = 11;

//  capability
#[derive(Copy, Clone)]
pub struct cap_t {
    pub words: [u64; 2],
}

#[derive(Copy, Clone)]
pub struct mdb_node_t {
    pub words: [u64; 2],
}

//  Cap Table Entry
#[derive(Copy, Clone)]
pub struct cte_t {
    pub cap: cap_t,
    pub cteMDBNode: mdb_node_t,
}

pub enum irq_state {
    IRQInactive = 0,
    IRQSignal = 1,
    IRQTimer = 2,
    IRQReserved,
}

pub struct dschedule {
    pub domain: dom_t,
    pub length: word_t,
}

pub enum endpoint_state {
    EPState_Idle = 0,
    EPState_Send = 1,
    EPState_Recv = 2,
}
type endpoint_state_t = word_t;

pub enum notification_state {
    NtfnState_Idle = 0,
    NtfnState_Waiting = 1,
    NtfnState_Active = 2,
}
type notification_state_t = word_t;

#[macro_export]
macro_rules! MASK {
    ($x:expr) => {
        (1u64 << ($x)) - 1u64
    };
}

pub const ZombieType_ZombieTCB: u64 = 1u64 << 6;
pub const TCB_CNODE_RADIX: u64 = 4;

#[inline]
pub fn Zombie_new(number: word_t, r#type: word_t, ptr: word_t) -> cap_t {
    let mask: word_t = if r#type == ZombieType_ZombieTCB {
        MASK!(TCB_CNODE_RADIX + 1)
    //(1u64<<(TCB_CNODE_RADIX+1))-1u64
    } else {
        MASK!(r#type + 1)
        //(1u64<<(r#type+1))-1u64
    };
    cap_zombie_cap_new((ptr & !mask) | (number & mask), r#type)
}

#[inline]
pub fn cap_zombie_cap_get_capZombieBits(cap: cap_t) -> word_t {
    let r#type = cap_zombie_cap_get_capZombieType(cap);
    if r#type == ZombieType_ZombieTCB {
        return TCB_CNODE_RADIX;
    }
    r#type & MASK!(6)
}

#[inline]
pub fn cap_zombie_cap_get_capZombieNumber(cap: cap_t) -> word_t {
    let radix: word_t = cap_zombie_cap_get_capZombieBits(cap);
    cap_zombie_cap_get_capZombieID(cap) & MASK!(radix + 1)
}

#[inline]
pub fn cap_zombie_cap_get_capZombiePtr(cap: cap_t) -> word_t {
    let radix: word_t = cap_zombie_cap_get_capZombieBits(cap);
    cap_zombie_cap_get_capZombieID(cap) & !MASK!(radix + 1)
}

#[inline]
pub fn cap_zombie_cap_set_capZombieNumber(cap: cap_t, n: word_t) -> cap_t {
    let radix: word_t = cap_zombie_cap_get_capZombieBits(cap);
    let ptr = cap_zombie_cap_get_capZombieID(cap) & !MASK!(radix + 1);
    cap_zombie_cap_set_capZombieID(cap, ptr | (n & MASK!(radix + 1)))
}


pub enum tcb_cnode_index {
    tcbCTable = 0,
    tcbVTable = 1,
    tcbReply = 2,
    tcbCaller = 3,
    tcbBuffer = 4,
    tcbCNodeEntries,
}
type tcb_cnode_index_t = word_t;

enum vm_rights {
    VMKernelOnly = 1,
    VMReadOnly = 2,
    VMReadWrite = 3,
}
type vm_rights_t = word_t;

struct vm_attributes {
    words: [u64; 1],
}
type vm_attributes_t = vm_attributes;

#[inline]
fn vmAttributesFromWord(w: word_t) -> vm_attributes_t {
    vm_attributes_t { words: [w] }
}

#[derive(Copy, Clone)]
pub struct thread_state_t {
    pub words: [u64; 3],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct notification_t {
    pub words: [u64; 4],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct lookup_fault_t {
    pub words: [u64; 2],
}


pub enum cap_tag_t {
    cap_null_cap = 0,
    cap_untyped_cap = 2,
    cap_endpoint_cap = 4,
    cap_notification_cap = 6,
    cap_reply_cap = 8,
    cap_cnode_cap = 10,
    cap_thread_cap = 12,
    cap_irq_control_cap = 14,
    cap_irq_handler_cap = 16,
    cap_zombie_cap = 18,
    cap_domain_cap = 20,
    cap_frame_cap = 1,
    cap_page_table_cap = 3,
    cap_page_directory_cap = 5,
    cap_pdpt_cap = 7,
    cap_pml4_cap = 9,
    cap_asid_control_cap = 11,
    cap_asid_pool_cap = 13,
    cap_io_port_cap = 19,
    cap_io_port_control_cap = 31,
}

pub const seL4_EndpointBits: u64 = 4;
pub const seL4_NotificationBits: u64 = 5;
pub const seL4_SlotBits: u64 = 5;

#[inline]
pub fn cap_get_capSizeBits(cap: cap_t) -> word_t {
    let ctag = cap_get_capType(cap);
    //rust不允许整数直接转枚举体，所以只能用这种别扭的写法了
    match ctag {
        ctag if ctag == (cap_tag_t::cap_null_cap as u64) => cap_untyped_cap_get_capBlockSize(cap),
        ctag if ctag == (cap_tag_t::cap_endpoint_cap as u64) => seL4_EndpointBits,
        ctag if ctag == (cap_tag_t::cap_notification_cap as u64) => seL4_NotificationBits,
        ctag if ctag == (cap_tag_t::cap_cnode_cap as u64) => {
            cap_cnode_cap_get_capCNodeRadix(cap) + seL4_SlotBits
        }
        ctag if ctag == (cap_tag_t::cap_thread_cap as u64) => seL4_TCBBits,
        ctag if ctag == (cap_tag_t::cap_zombie_cap as u64) => {
            let r#type = cap_zombie_cap_get_capZombieType(cap);
            if r#type == ZombieType_ZombieTCB {
                seL4_TCBBits
            } else {
                (r#type & MASK!(6)) + seL4_SlotBits
            }
        }
        ctag if ctag == (cap_tag_t::cap_null_cap as u64)
            || ctag == (cap_tag_t::cap_domain_cap as u64)
            || ctag == (cap_tag_t::cap_reply_cap as u64)
            || ctag == (cap_tag_t::cap_irq_control_cap as u64)
            || ctag == (cap_tag_t::cap_irq_handler_cap as u64) =>
        {
            0
        }
        _ => cap_get_archCapSizeBits(cap),
    }
}

#[inline]
pub fn cap_get_capIsPhysical(cap: cap_t) -> bool_t {
    let ctag = cap_get_capType(cap);
    match ctag {
        ctag if ctag == (cap_tag_t::cap_untyped_cap as u64)
            || ctag == (cap_tag_t::cap_endpoint_cap as u64)
            || ctag == (cap_tag_t::cap_notification_cap as u64)
            || ctag == (cap_tag_t::cap_cnode_cap as u64)
            || ctag == (cap_tag_t::cap_thread_cap as u64)
            || ctag == (cap_tag_t::cap_zombie_cap as u64) =>
        {
            _bool::r#true as u64
        }
        ctag if ctag == (cap_tag_t::cap_domain_cap as u64)
            || ctag == (cap_tag_t::cap_reply_cap as u64)
            || ctag == (cap_tag_t::cap_irq_control_cap as u64)
            || ctag == (cap_tag_t::cap_irq_handler_cap as u64) =>
        {
            _bool::r#false as u64
        }
        _ => cap_get_archCapIsPhysical(cap),
    }
}

#[inline]
pub unsafe fn cap_get_capPtr(cap: cap_t) -> u64 {
    let ctag = cap_get_capType(cap);
    if ctag == cap_tag_t::cap_untyped_cap as u64 {
        return cap_untyped_cap_get_capPtr(cap);
    } else if ctag == cap_tag_t::cap_endpoint_cap as u64 {
        return cap_endpoint_cap_get_capEPPtr(cap);
    } else if ctag == cap_tag_t::cap_notification_cap as u64 {
        return cap_notification_cap_get_capNtfnPtr(cap);
    } else if ctag == cap_tag_t::cap_cnode_cap as u64 {
        return cap_cnode_cap_get_capCNodePtr(cap);
    } else if ctag == cap_tag_t::cap_thread_cap as u64 {
        return tcb_ptr_cte_ptr(cap_thread_cap_get_capTCBPtr(cap) as *mut tcb_t, 0) as u64;
    } else if ctag == cap_tag_t::cap_zombie_cap as u64 {
        return cap_zombie_cap_get_capZombiePtr(cap);
    } else if ctag == cap_tag_t::cap_domain_cap as u64
        || ctag == cap_tag_t::cap_reply_cap as u64
        || ctag == cap_tag_t::cap_irq_control_cap as u64
        || ctag == cap_tag_t::cap_irq_handler_cap as u64
    {
        return 0u64;
    }
    cap_get_archCapPtr(cap)
}

#[inline]
pub fn isCapRevocable(derivedCap: cap_t, srcCap: cap_t) -> bool_t {
    if isArchCap(derivedCap) != 0 {
        return Arch_isCapRevocable(derivedCap, srcCap);
    }
    let ctag = cap_get_capType(derivedCap);
    match ctag {
        ctag if ctag == (cap_tag_t::cap_endpoint_cap as u64) => {
            (cap_endpoint_cap_get_capEPBadge(derivedCap) != cap_endpoint_cap_get_capEPBadge(srcCap))
                as u64
        }
        ctag if ctag == (cap_tag_t::cap_notification_cap as u64) => {
            (cap_notification_cap_get_capNtfnBadge(derivedCap)
                != cap_notification_cap_get_capNtfnBadge(srcCap)) as u64
        }
        ctag if ctag == (cap_tag_t::cap_irq_handler_cap as u64) => {
            (cap_get_capType(srcCap) == cap_tag_t::cap_irq_control_cap as u64) as u64
        }
        ctag if ctag == (cap_tag_t::cap_untyped_cap as u64) => _bool::r#true as u64,
        _ => _bool::r#false as u64,
    }
}

#[inline]
pub unsafe fn tcb_ptr_cte_ptr(p: *mut tcb_t, i: u64) -> *mut cte_t {
    (((p as u64) & (!MASK!(seL4_TCBBits))) as *mut cte_t).offset(i as isize)
}

// include/object/tcb.h 因为不想翻译tcb.h整个文件所以就放这里了
#[repr(C)]
#[derive(Copy, Clone)]
pub struct tcb_queue {
    pub head: *mut tcb_t,
    pub end: *mut tcb_t,
}
pub type tcb_queue_t = tcb_queue;

//  types for specific architectures and others.

//include/arch/x86/arch/64/mode/types.h
pub const wordRadix: u64 = 6;
pub const wordBits: u64 = 1 << 6;

//include/types.h
pub type word_t = UBaseType;
pub type sword_t = i64;
pub type vptr_t = word_t;
pub type paddr_t = word_t;
pub type pptr_t = word_t;
pub type cptr_t = word_t;
pub type dev_id_t = word_t;
pub type cpu_id_t = word_t;
pub type logical_id_t = u32;
pub type node_id_t = word_t;
pub type dom_t = word_t;

//include/api/types.h
pub type prio_t = word_t;

//include/basic_types.h
#[repr(C)]
pub enum _bool {
    r#false = 0,
    r#true = 1,
}
pub type bool_t = word_t;

//include/compound_types.h
#[repr(C)]
pub struct pde_range {
    base: *mut pde_t,
    length: word_t,
}
pub type pde_range_t = pde_range;

#[repr(C)]
pub struct pte_range {
    base: *mut pte_t,
    length: word_t,
}
pub type pte_range_t = pte_range;
pub type cte_ptr_t = *mut cte_t;

const seL4_MsgExtraCapBits: usize = 2;
pub const seL4_MsgMaxExtraCaps: usize = (1usize << seL4_MsgExtraCapBits) - 1;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct extra_caps {
    pub excaprefs: [cte_ptr_t; seL4_MsgMaxExtraCaps],
}
pub type extra_caps_t = extra_caps;

//generated/mode/api/shared_types_gen.h
#[repr(C)]
#[derive(Copy, Clone)]
pub struct seL4_MessageInfo {
    words: [u64; 1],
}
pub type seL4_MessageInfo_t = seL4_MessageInfo;

#[inline]
pub fn seL4_MessageInfo_get_length(seL4_MessageInfo: seL4_MessageInfo_t) -> u64 {
    seL4_MessageInfo.words[0] & 0x7fu64
}
#[inline]
pub fn seL4_MessageInfo_set_length(
    mut seL4_MessageInfo: seL4_MessageInfo_t,
    v64: u64,
) -> seL4_MessageInfo_t {
    seL4_MessageInfo.words[0] &= !0x7fu64;
    seL4_MessageInfo.words[0] |= v64 & 0x7fu64;
    seL4_MessageInfo
}

#[inline]
pub fn seL4_MessageInfo_get_extraCaps(seL4_MessageInfo: seL4_MessageInfo_t) -> u64 {
    (seL4_MessageInfo.words[0] & 0x180u64) >> 7
}

#[inline]
pub fn seL4_MessageInfo_set_extraCaps(
    mut seL4_MessageInfo: seL4_MessageInfo_t,
    v64: u64,
) -> seL4_MessageInfo_t {
    seL4_MessageInfo.words[0] &= !0x180u64;
    seL4_MessageInfo.words[0] |= (v64 << 7) & 0x180u64;
    seL4_MessageInfo
}

#[inline]
pub fn seL4_MessageInfo_get_capsUnwrapped(seL4_MessageInfo: seL4_MessageInfo_t) -> u64 {
    (seL4_MessageInfo.words[0] & 0xe00u64) >> 9
}

#[inline]
pub fn seL4_MessageInfo_set_capsUnwrapped(
    mut seL4_MessageInfo: seL4_MessageInfo_t,
    v64: u64,
) -> seL4_MessageInfo_t {
    seL4_MessageInfo.words[0] &= !0xe00u64;
    seL4_MessageInfo.words[0] |= (v64 << 9) & 0xe00u64;
    seL4_MessageInfo
}

#[inline]
pub fn seL4_MessageInfo_new(
    label: u64,
    capsUnwrapped: u64,
    extraCaps: u64,
    length: u64,
) -> seL4_MessageInfo_t {
    let ret: u64 = 0
        | (label & 0xfffffffffffffu64) << 12
        | (capsUnwrapped & 0x7u64) << 9
        | (extraCaps & 0x3u64) << 7
        | (length & 0x7fu64) << 0;
    seL4_MessageInfo_t { words: [ret] }
}

#[inline]
pub fn seL4_CapRights_get_capAllowGrant(seL4_CapRights: seL4_CapRights_t) -> u64 {
    (seL4_CapRights.words[0] & 0x4u64) >> 2
}

#[inline]
pub fn seL4_CapRights_get_capAllowRead(seL4_CapRights: seL4_CapRights_t) -> u64 {
    (seL4_CapRights.words[0] & 0x2u64) >> 1
}

#[inline]
pub fn seL4_CapRights_get_capAllowWrite(seL4_CapRights: seL4_CapRights_t) -> u64 {
    seL4_CapRights.words[0] & 0x1u64
}

//include/api/types.h
pub const seL4_MsgMaxLength: u64 = 120;
#[inline]
pub fn messageInfoFromWord(w: word_t) -> seL4_MessageInfo_t {
    let mut mi: seL4_MessageInfo_t = seL4_MessageInfo_t { words: [w] };
    let len: word_t = seL4_MessageInfo_get_length(mi);
    if len > seL4_MsgMaxLength {
        mi = seL4_MessageInfo_set_length(mi, seL4_MsgMaxLength);
    }
    mi
}

#[inline]
pub fn wordFromMessageInfo(mi: seL4_MessageInfo_t) -> word_t {
    mi.words[0]
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct seL4_CapRights_t {
    words: [u64; 1],
}

#[inline]
pub fn rightsFromWord(w: u64) -> seL4_CapRights_t {
    seL4_CapRights_t { words: [w] }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct cap_transfer_t {
    pub ctReceiveRoot: u64,
    pub ctReceiveIndex: u64,
    pub ctReceiveDepth: u64,
}

#[inline]
pub unsafe fn capTransferFromWords(wptr: *mut u64) -> cap_transfer_t {
    cap_transfer_t {
        ctReceiveRoot: *wptr.offset(0),
        ctReceiveIndex: *wptr.offset(1),
        ctReceiveDepth: *wptr.offset(2),
    }
}

// 各个元素
// cap.rs
#[derive(Copy, Clone)]
pub struct deriveCap_ret_t {
    pub status: u64,
    pub cap: cap_t,
}

#[derive(Copy, Clone)]
pub struct finaliseCap_ret_t {
    pub remainder: cap_t,
    pub cleanupInfo: cap_t,
}

// failure
pub enum exception {
    EXCEPTION_NONE = 0,
    EXCEPTION_FAULT = 1,
    EXCEPTION_LOOKUP_FAULT = 2,
    EXCEPTION_SYSCALL_ERROR = 3,
    EXCEPTION_PREEMPTED = 4,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct syscall_error_t {
    pub invalidArgumentNumber: u64,
    pub invalidCapNumber: u64,
    pub rangeErrorMin: u64,
    pub rangeErrorMax: u64,
    pub memoryLeft: u64,
    pub failedLookupWasSource: u64,
    pub type_: u64,
}

// error
pub enum seL4_Error {
    seL4_NoError = 0,
    seL4_InvalidArgument = 1,
    seL4_InvalidCapability = 2,
    seL4_IllegalOperation = 3,
    seL4_RangeError = 4,
    seL4_AlignmentError = 5,
    seL4_FailedLookup = 6,
    seL4_TruncatedMessage = 7,
    seL4_DeleteFirst = 8,
    seL4_RevokeFirst = 9,
    seL4_NotEnoughMemory = 10,
    seL4_NumErrors = 11,
}

#[macro_export]
macro_rules! userError {
    ($($x:expr),*) => {};
}

