/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <assert.h>
#include <types.h>
#include <api/failures.h>
#include <api/invocation.h>
#include <api/syscall.h>
#include <api/types.h>
#include <machine/io.h>
#include <object/structures.h>
#include <object/objecttype.h>
#include <object/cnode.h>
#include <object/interrupt.h>
#include <object/untyped.h>
#include <kernel/cspace.h>
#include <kernel/thread.h>
#include <model/preemption.h>
#include <model/statedata.h>
#include <util.h>

struct finaliseSlot_ret {
    exception_t status;
    bool_t success;
    cap_t cleanupInfo;
};
typedef struct finaliseSlot_ret finaliseSlot_ret_t;

static finaliseSlot_ret_t finaliseSlot(cte_t *slot, bool_t exposed);
static void emptySlot(cte_t *slot, cap_t cleanupInfo);
static exception_t reduceZombie(cte_t *slot, bool_t exposed);

#ifdef CONFIG_KERNEL_MCS
#define CNODE_LAST_INVOCATION CNodeRotate
#else
#define CNODE_LAST_INVOCATION CNodeSaveCaller   /*Z CNode有关系统调用的最后一个子功能编号 */
#endif
/*Z 引用cap_cnode_cap能力的系统调用 */
exception_t decodeCNodeInvocation(word_t invLabel, word_t length, cap_t cap,/*Z 消息标签、长度、能力 */
                                  extra_caps_t excaps, word_t *buffer)/*Z 额外能力、IPC buffer */
{
    lookupSlot_ret_t lu_ret;
    cte_t *destSlot;
    word_t index, w_bits;
    exception_t status;

    /* Haskell error: "decodeCNodeInvocation: invalid cap" */
    assert(cap_get_capType(cap) == cap_cnode_cap);

    if (invLabel < CNodeRevoke || invLabel > CNODE_LAST_INVOCATION) {
        userError("CNodeCap: Illegal Operation attempted.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (length < 2) {
        userError("CNode operation: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }
    index = getSyscallArg(0, buffer);/*Z ---------------------------------------消息传参(公共部分)：0-要操作的目标能力句柄 */
    w_bits = getSyscallArg(1, buffer);                                                          /*Z 1-句柄深度。不能为0 */
    /*Z 查找要操作能力句柄指代的能力 */
    lu_ret = lookupTargetSlot(cap, index, w_bits);
    if (lu_ret.status != EXCEPTION_NONE) {
        userError("CNode operation: Target slot invalid.");
        return lu_ret.status;
    }
    destSlot = lu_ret.slot;

    if (invLabel >= CNodeCopy && invLabel <= CNodeMutate) {
        cte_t *srcSlot;
        word_t srcIndex, srcDepth, capData;
        bool_t isMove;
        seL4_CapRights_t cap_rights;
        cap_t srcRoot, newCap;
        deriveCap_ret_t dc_ret;
        cap_t srcCap;

        if (length < 4 || excaps.excaprefs[0] == NULL) {
            userError("CNode Copy/Mint/Move/Mutate: Truncated message.");
            current_syscall_error.type = seL4_TruncatedMessage;
            return EXCEPTION_SYSCALL_ERROR;
        }
        srcIndex = getSyscallArg(2, buffer);/*Z --------------------------------消息传参(个例部分)：2-源能力句柄 */
        srcDepth = getSyscallArg(3, buffer);                                                    /*Z 3-句柄深度 */

        srcRoot = excaps.excaprefs[0]->cap;                                                     /*Z extraCaps0-源CNode */

        status = ensureEmptySlot(destSlot);
        if (status != EXCEPTION_NONE) {/*Z 验证目标CSlot要为空 */
            userError("CNode Copy/Mint/Move/Mutate: Destination not empty.");
            return status;
        }

        lu_ret = lookupSourceSlot(srcRoot, srcIndex, srcDepth);
        if (lu_ret.status != EXCEPTION_NONE) {/*Z 验证源能力要存在 */
            userError("CNode Copy/Mint/Move/Mutate: Invalid source slot.");
            return lu_ret.status;
        }
        srcSlot = lu_ret.slot;

        if (cap_get_capType(srcSlot->cap) == cap_null_cap) {/*Z 验证源能力不为空 */
            userError("CNode Copy/Mint/Move/Mutate: Source slot invalid or empty.");
            current_syscall_error.type = seL4_FailedLookup;
            current_syscall_error.failedLookupWasSource = 1;
            current_lookup_fault =
                lookup_fault_missing_capability_new(srcDepth);
            return EXCEPTION_SYSCALL_ERROR;
        }

        switch (invLabel) {
        case CNodeCopy:/*Z ----------------------------------------------------------子功能：拷贝能力 */

            if (length < 5) {
                userError("Truncated message for CNode Copy operation.");
                current_syscall_error.type = seL4_TruncatedMessage;
                return EXCEPTION_SYSCALL_ERROR;
            }

            cap_rights = rightsFromWord(getSyscallArg(4, buffer));/*Z---------------消息传参：4-新能力权限 */
            srcCap = maskCapRights(cap_rights, srcSlot->cap);/*Z 根据新权限设置能力最后的权限，只减不增 */
            dc_ret = deriveCap(srcSlot, srcCap);/*Z 返回拷贝(导出)的能力。基本是原能力，要作一些排错、复位等处理 */
            if (dc_ret.status != EXCEPTION_NONE) {
                userError("Error deriving cap for CNode Copy operation.");
                return dc_ret.status;
            }
            newCap = dc_ret.cap;
            isMove = false;

            break;

        case CNodeMint:/*Z ----------------------------------------------------------子功能：制作(拷贝并修改)能力 */
            if (length < 6) {
                userError("CNode Mint: Truncated message.");
                current_syscall_error.type = seL4_TruncatedMessage;
                return EXCEPTION_SYSCALL_ERROR;
            }

            cap_rights = rightsFromWord(getSyscallArg(4, buffer));/*Z-----------------消息传参：4-新能力权限 */
            capData = getSyscallArg(5, buffer);                                             /*Z 5-新能力的可更新参数 */
            srcCap = maskCapRights(cap_rights, srcSlot->cap);
            dc_ret = deriveCap(srcSlot,
                               updateCapData(false, capData, srcCap));
            if (dc_ret.status != EXCEPTION_NONE) {
                userError("Error deriving cap for CNode Mint operation.");
                return dc_ret.status;
            }
            newCap = dc_ret.cap;
            isMove = false;

            break;

        case CNodeMove:/*Z ----------------------------------------------------------子功能：移动能力 */
            newCap = srcSlot->cap;
            isMove = true;

            break;

        case CNodeMutate:
            if (length < 5) {/*Z ----------------------------------------------------------子功能：修改能力 */
                userError("CNode Mutate: Truncated message.");
                current_syscall_error.type = seL4_TruncatedMessage;
                return EXCEPTION_SYSCALL_ERROR;
            }

            capData = getSyscallArg(4, buffer);/*Z ----------------------------------消息传参：4-新能力的可更新参数 */
            newCap = updateCapData(true, capData, srcSlot->cap);
            isMove = true;

            break;

        default:
            assert(0);
            return EXCEPTION_NONE;
        }

        if (cap_get_capType(newCap) == cap_null_cap) {
            userError("CNode Copy/Mint/Move/Mutate: Mutated cap would be invalid.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }

        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        if (isMove) {/*Z 移动CSlot(目标使用新能力) */
            return invokeCNodeMove(newCap, srcSlot, destSlot);
        } else {/*Z 源、目的CSlot建立关联，并将新能力拷贝到目的CSlot。目的CSlot必须为空能力 */
            return invokeCNodeInsert(newCap, srcSlot, destSlot);
        }
    }

    if (invLabel == CNodeRevoke) {/*Z ----------------------------------------------------------子功能：回收能力(不包括本身) */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeCNodeRevoke(destSlot);/*Z 回收能力：递归删除子能力 */
    }

    if (invLabel == CNodeDelete) {/*Z ----------------------------------------------------------子功能：删除能力 */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeCNodeDelete(destSlot);/*Z 删除能力，摘出CSlot关联链条 */
    }

#ifndef CONFIG_KERNEL_MCS
    if (invLabel == CNodeSaveCaller) {/*Z ----------------------------------------------------------子功能：保存回复Caller的能力 */
        status = ensureEmptySlot(destSlot);
        if (status != EXCEPTION_NONE) {
            userError("CNode SaveCaller: Destination slot not empty.");
            return status;
        }

        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeCNodeSaveCaller(destSlot);/*Z 将当前线程(主动方)设置的tcbCaller回复能力，移至目标(被要求方)CSlot */
    }
#endif

    if (invLabel == CNodeCancelBadgedSends) {/*Z ----------------------------------------------------------子功能：取消于某一EP的线程阻塞 */
        cap_t destCap;

        destCap = destSlot->cap;
        /*Z 只有具备全部4个权限的EP能力可以取消发送 */
        if (!hasCancelSendRights(destCap)) {
            userError("CNode CancelBadgedSends: Target cap invalid.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeCNodeCancelBadgedSends(destCap);/*Z 取消EP队列中阻塞于该EP的线程阻塞 */
    }

    if (invLabel == CNodeRotate) {/*Z ----------------------------------------------------------子功能：源、中间方到目标的能力依次移动 */
        word_t pivotNewData, pivotIndex, pivotDepth;
        word_t srcNewData, srcIndex, srcDepth;
        cte_t *pivotSlot, *srcSlot;
        cap_t pivotRoot, srcRoot, newSrcCap, newPivotCap;

        if (length < 8 || excaps.excaprefs[0] == NULL
            || excaps.excaprefs[1] == NULL) {
            current_syscall_error.type = seL4_TruncatedMessage;
            return EXCEPTION_SYSCALL_ERROR;
        }
        pivotNewData = getSyscallArg(2, buffer);/*Z ----------------------------------消息传参：2-中间方能力新数据 */
        pivotIndex   = getSyscallArg(3, buffer);                                            /*Z 3-中间方CSlot句柄 */
        pivotDepth   = getSyscallArg(4, buffer);                                            /*Z 4-中间方CSlot深度 */
        srcNewData   = getSyscallArg(5, buffer);                                            /*Z 5-源方能力新数据 */
        srcIndex     = getSyscallArg(6, buffer);                                            /*Z 6-源方CSlot句柄 */
        srcDepth     = getSyscallArg(7, buffer);                                            /*Z 7-源方CSlot深度 */

        pivotRoot = excaps.excaprefs[0]->cap;                                               /*Z extraCaps0-中间CNode */
        srcRoot   = excaps.excaprefs[1]->cap;                                               /*Z extraCaps1-源CNode */
        /*Z 查找源方CSlot */
        lu_ret = lookupSourceSlot(srcRoot, srcIndex, srcDepth);
        if (lu_ret.status != EXCEPTION_NONE) {
            return lu_ret.status;
        }
        srcSlot = lu_ret.slot;
        /*Z 查找目标方CSlot */
        lu_ret = lookupPivotSlot(pivotRoot, pivotIndex, pivotDepth);
        if (lu_ret.status != EXCEPTION_NONE) {
            return lu_ret.status;
        }
        pivotSlot = lu_ret.slot;

        if (pivotSlot == srcSlot || pivotSlot == destSlot) {/*Z 验证中间方要是不同的 */
            userError("CNode Rotate: Pivot slot the same as source or dest slot.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }

        if (srcSlot != destSlot) {/*Z 源与目标不同，则目标必须为空 */
            status = ensureEmptySlot(destSlot);
            if (status != EXCEPTION_NONE) {
                return status;
            }
        }

        if (cap_get_capType(srcSlot->cap) == cap_null_cap) {/*Z 验证源不为空 */
            current_syscall_error.type = seL4_FailedLookup;
            current_syscall_error.failedLookupWasSource = 1;
            current_lookup_fault = lookup_fault_missing_capability_new(srcDepth);
            return EXCEPTION_SYSCALL_ERROR;
        }

        if (cap_get_capType(pivotSlot->cap) == cap_null_cap) {/*Z 验证中间方不为空 */
            current_syscall_error.type = seL4_FailedLookup;
            current_syscall_error.failedLookupWasSource = 0;
            current_lookup_fault = lookup_fault_missing_capability_new(pivotDepth);
            return EXCEPTION_SYSCALL_ERROR;
        }
        /*Z 更新源、中间方能力的新数据 */
        newSrcCap = updateCapData(true, srcNewData, srcSlot->cap);
        newPivotCap = updateCapData(true, pivotNewData, pivotSlot->cap);

        if (cap_get_capType(newSrcCap) == cap_null_cap) {
            userError("CNode Rotate: Source cap invalid.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }

        if (cap_get_capType(newPivotCap) == cap_null_cap) {
            userError("CNode Rotate: Pivot cap invalid.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }

        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeCNodeRotate(newSrcCap, newPivotCap,/*Z 旋转移动CSlot：1 -> 2 -> 3(1) */
                                 srcSlot, pivotSlot, destSlot);
    }

    return EXCEPTION_NONE;
}
/*Z 回收能力：递归删除子能力 */
exception_t invokeCNodeRevoke(cte_t *destSlot)
{
    return cteRevoke(destSlot);
}
/*Z 删除能力，摘出CSlot关联链条 */
exception_t invokeCNodeDelete(cte_t *destSlot)
{
    return cteDelete(destSlot, true);
}
/*Z 取消EP队列中阻塞于该EP的线程阻塞 */
exception_t invokeCNodeCancelBadgedSends(cap_t cap)
{
    word_t badge = cap_endpoint_cap_get_capEPBadge(cap);
    if (badge) {
        endpoint_t *ep = (endpoint_t *)
                         cap_endpoint_cap_get_capEPPtr(cap);
        cancelBadgedSends(ep, badge);/*Z 取消EP队列中阻塞于指定标记的线程阻塞 */
    }
    return EXCEPTION_NONE;
}
/*Z 源、目的CSlot建立关联，并将新能力拷贝到目的CSlot。目的CSlot必须为空能力 */
exception_t invokeCNodeInsert(cap_t cap, cte_t *srcSlot, cte_t *destSlot)
{
    cteInsert(cap, srcSlot, destSlot);

    return EXCEPTION_NONE;
}
/*Z 移动CSlot(目标使用新能力)。目标CSlot必须为空能力 */
exception_t invokeCNodeMove(cap_t cap, cte_t *srcSlot, cte_t *destSlot)
{
    cteMove(cap, srcSlot, destSlot);

    return EXCEPTION_NONE;
}
/*Z 旋转移动CSlot：1 -> 2 -> 3(1) */
exception_t invokeCNodeRotate(cap_t cap1, cap_t cap2, cte_t *slot1,/*Z 源能力、支点能力、源CSlot */
                              cte_t *slot2, cte_t *slot3)/*Z 支点CSlot、目标CSlot */
{
    if (slot1 == slot3) {/*Z 如果源、目标CSlot相同，交换源与支点 */
        cteSwap(cap1, slot1, cap2, slot2);
    } else {/*Z 否则，支点移到目标，源移到支点 */
        cteMove(cap2, slot2, slot3);
        cteMove(cap1, slot1, slot2);
    }

    return EXCEPTION_NONE;
}

#ifndef CONFIG_KERNEL_MCS
/*Z 将当前线程(主动方)设置的tcbCaller回复能力，移至目标(被要求方)CSlot */
exception_t invokeCNodeSaveCaller(cte_t *destSlot)
{
    cap_t cap;
    cte_t *srcSlot;
    /*Z 当前线程回复能力，被要求方接收 */
    srcSlot = TCB_PTR_CTE_PTR(NODE_STATE(ksCurThread), tcbCaller);
    cap = srcSlot->cap;

    switch (cap_get_capType(cap)) {
    case cap_null_cap:
        userError("CNode SaveCaller: Reply cap not present.");
        break;

    case cap_reply_cap:
        if (!cap_reply_cap_get_capReplyMaster(cap)) {
            cteMove(cap, srcSlot, destSlot);/*Z 移动CSlot(目标使用新能力) */
        }
        break;

    default:
        fail("caller capability must be null or reply");
        break;
    }

    return EXCEPTION_NONE;
}
#endif
/*Z 如果新旧untyped内存完全相同，则标记旧untyped内存满 */
/*
 * If creating a child UntypedCap, don't allow new objects to be created in the
 * parent.
 */
static void setUntypedCapAsFull(cap_t srcCap, cap_t newCap, cte_t *srcSlot)
{
    if ((cap_get_capType(srcCap) == cap_untyped_cap)
        && (cap_get_capType(newCap) == cap_untyped_cap)) {
        if ((cap_untyped_cap_get_capPtr(srcCap)
             == cap_untyped_cap_get_capPtr(newCap))
            && (cap_untyped_cap_get_capBlockSize(newCap)
                == cap_untyped_cap_get_capBlockSize(srcCap))) {
            cap_untyped_cap_ptr_set_capFreeIndex(&(srcSlot->cap),
                                                 MAX_FREE_INDEX(cap_untyped_cap_get_capBlockSize(srcCap)));
        }
    }
}
/*Z 源、目的CSlot建立关联，并将新能力拷贝到目的CSlot。目的CSlot必须为空能力 */
void cteInsert(cap_t newCap, cte_t *srcSlot, cte_t *destSlot)
{
    mdb_node_t srcMDB, newMDB;
    cap_t srcCap;
    bool_t newCapIsRevocable;

    srcMDB = srcSlot->cteMDBNode;
    srcCap = srcSlot->cap;
    /*Z 新能力相对于源能力是否可撤销 */
    newCapIsRevocable = isCapRevocable(newCap, srcCap);
    /*Z 建立CSlot间关联 */
    newMDB = mdb_node_set_mdbPrev(srcMDB, CTE_REF(srcSlot));
    newMDB = mdb_node_set_mdbRevocable(newMDB, newCapIsRevocable);
    newMDB = mdb_node_set_mdbFirstBadged(newMDB, newCapIsRevocable);

    /* Haskell error: "cteInsert to non-empty destination" */
    assert(cap_get_capType(destSlot->cap) == cap_null_cap);
    /* Haskell error: "cteInsert: mdb entry must be empty" */
    assert((cte_t *)mdb_node_get_mdbNext(destSlot->cteMDBNode) == NULL &&
           (cte_t *)mdb_node_get_mdbPrev(destSlot->cteMDBNode) == NULL);

    /* Prevent parent untyped cap from being used again if creating a child
     * untyped from it. */
    setUntypedCapAsFull(srcCap, newCap, srcSlot);

    destSlot->cap = newCap;
    destSlot->cteMDBNode = newMDB;
    mdb_node_ptr_set_mdbNext(&srcSlot->cteMDBNode, CTE_REF(destSlot));
    if (mdb_node_get_mdbNext(newMDB)) {/*Z 如果源CSlot有下一个关联 */
        mdb_node_ptr_set_mdbPrev(/*Z 则下一个关联的前向指向目的CSlot，即目的CSlot插入关联链条 */
            &CTE_PTR(mdb_node_get_mdbNext(newMDB))->cteMDBNode,
            CTE_REF(destSlot));
    }
}
/*Z 移动CSlot(目标使用新能力)。目标CSlot必须为空能力 */
void cteMove(cap_t newCap, cte_t *srcSlot, cte_t *destSlot)
{
    mdb_node_t mdb;
    word_t prev_ptr, next_ptr;

    /* Haskell error: "cteMove to non-empty destination" */
    assert(cap_get_capType(destSlot->cap) == cap_null_cap);
    /* Haskell error: "cteMove: mdb entry must be empty" */
    assert((cte_t *)mdb_node_get_mdbNext(destSlot->cteMDBNode) == NULL &&
           (cte_t *)mdb_node_get_mdbPrev(destSlot->cteMDBNode) == NULL);

    mdb = srcSlot->cteMDBNode;
    destSlot->cap = newCap;
    srcSlot->cap = cap_null_cap_new();
    destSlot->cteMDBNode = mdb;
    srcSlot->cteMDBNode = nullMDBNode;

    prev_ptr = mdb_node_get_mdbPrev(mdb);
    if (prev_ptr)
        mdb_node_ptr_set_mdbNext(
            &CTE_PTR(prev_ptr)->cteMDBNode,
            CTE_REF(destSlot));

    next_ptr = mdb_node_get_mdbNext(mdb);
    if (next_ptr)
        mdb_node_ptr_set_mdbPrev(
            &CTE_PTR(next_ptr)->cteMDBNode,
            CTE_REF(destSlot));
}
/*Z 交换两个CSlot的能力和关联 */
void capSwapForDelete(cte_t *slot1, cte_t *slot2)
{
    cap_t cap1, cap2;

    if (slot1 == slot2) {
        return;
    }

    cap1 = slot1->cap;
    cap2 = slot2->cap;

    cteSwap(cap1, slot1, cap2, slot2);
}
/*Z 交换两个CSlot的能力和关联 */
void cteSwap(cap_t cap1, cte_t *slot1, cap_t cap2, cte_t *slot2)
{
    mdb_node_t mdb1, mdb2;
    word_t next_ptr, prev_ptr;
    /*Z 交换能力 */
    slot1->cap = cap2;
    slot2->cap = cap1;
    /*Z 交换关联 */
    mdb1 = slot1->cteMDBNode;

    prev_ptr = mdb_node_get_mdbPrev(mdb1);
    if (prev_ptr)
        mdb_node_ptr_set_mdbNext(
            &CTE_PTR(prev_ptr)->cteMDBNode,
            CTE_REF(slot2));

    next_ptr = mdb_node_get_mdbNext(mdb1);
    if (next_ptr)
        mdb_node_ptr_set_mdbPrev(
            &CTE_PTR(next_ptr)->cteMDBNode,
            CTE_REF(slot2));

    mdb2 = slot2->cteMDBNode;
    slot1->cteMDBNode = mdb2;
    slot2->cteMDBNode = mdb1;

    prev_ptr = mdb_node_get_mdbPrev(mdb2);
    if (prev_ptr)
        mdb_node_ptr_set_mdbNext(
            &CTE_PTR(prev_ptr)->cteMDBNode,
            CTE_REF(slot1));

    next_ptr = mdb_node_get_mdbNext(mdb2);
    if (next_ptr)
        mdb_node_ptr_set_mdbPrev(
            &CTE_PTR(next_ptr)->cteMDBNode,
            CTE_REF(slot1));
}
/*Z 回收能力：递归删除子能力 */
exception_t cteRevoke(cte_t *slot)
{
    cte_t *nextPtr;
    exception_t status;
    /*Z 递归删除子能力 */
    /* there is no need to check for a NullCap as NullCaps are
       always accompanied by null mdb pointers */
    for (nextPtr = CTE_PTR(mdb_node_get_mdbNext(slot->cteMDBNode));
         nextPtr && isMDBParentOf(slot, nextPtr);
         nextPtr = CTE_PTR(mdb_node_get_mdbNext(slot->cteMDBNode))) {
        status = cteDelete(nextPtr, true);/*Z 置空CSlot摘出关联链条，实施清理工作 */
        if (status != EXCEPTION_NONE) {
            return status;
        }
        /*Z 检查抢占点 */
        status = preemptionPoint();
        if (status != EXCEPTION_NONE) {
            return status;
        }
    }

    return EXCEPTION_NONE;
}
/*Z 置空CSlot摘出关联链条，实施清理工作 */
exception_t cteDelete(cte_t *slot, bool_t exposed)
{
    finaliseSlot_ret_t fs_ret;
    /*Z 最后化CSlot */
    fs_ret = finaliseSlot(slot, exposed);
    if (fs_ret.status != EXCEPTION_NONE) {
        return fs_ret.status;
    }
    /*Z 置空CSlot摘出关联链条，实施后续清理工作 */
    if (exposed || fs_ret.success) {
        emptySlot(slot, fs_ret.cleanupInfo);
    }
    return EXCEPTION_NONE;
}
/*Z 置空CSlot摘出关联链条，实施后续清理工作 */
static void emptySlot(cte_t *slot, cap_t cleanupInfo)
{
    if (cap_get_capType(slot->cap) != cap_null_cap) {/*Z 不是空能力的，作清理 */
        mdb_node_t mdbNode;
        cte_t *prev, *next;
        /*Z 关系的父子CSlot */
        mdbNode = slot->cteMDBNode;
        prev = CTE_PTR(mdb_node_get_mdbPrev(mdbNode));
        next = CTE_PTR(mdb_node_get_mdbNext(mdbNode));
        /*Z 从关联链条中摘出 */
        if (prev) {
            mdb_node_ptr_set_mdbNext(&prev->cteMDBNode, CTE_REF(next));
        }
        if (next) {
            mdb_node_ptr_set_mdbPrev(&next->cteMDBNode, CTE_REF(prev));
        }
        if (next)/*Z 如果是首个标记的，则传递给下一个 */
            mdb_node_ptr_set_mdbFirstBadged(&next->cteMDBNode,
                                            mdb_node_get_mdbFirstBadged(next->cteMDBNode) ||
                                            mdb_node_get_mdbFirstBadged(mdbNode));
        slot->cap = cap_null_cap_new();
        slot->cteMDBNode = nullMDBNode;
        /*Z 删除能力后的清理：对IRQ处理能力设置该IRQ全局禁用，对I/O端口访问能力清除全局分配位标记 */
        postCapDeletion(cleanupInfo);
    }
}
/*Z 空能力、容量为0的Zombie能力、CSlot是Zombie能力的唯一内容，这些是可删除的 */
static inline bool_t CONST capRemovable(cap_t cap, cte_t *slot)
{
    switch (cap_get_capType(cap)) {
    case cap_null_cap:
        return true;
    case cap_zombie_cap: {/*Z 获取Zombie能力的容量、地址 */
        word_t n = cap_zombie_cap_get_capZombieNumber(cap);
        cte_t *z_slot = (cte_t *)cap_zombie_cap_get_capZombiePtr(cap);
        return (n == 0 || (n == 1 && slot == z_slot));
    }
    default:
        fail("finaliseCap should only return Zombie or NullCap");
    }
}
/*Z Zombie能力指向的CSlot，是可以循环再使用的 */
static inline bool_t CONST capCyclicZombie(cap_t cap, cte_t *slot)
{
    return cap_get_capType(cap) == cap_zombie_cap &&
           CTE_PTR(cap_zombie_cap_get_capZombiePtr(cap)) == slot;
}
/*Z 最后化CSlot。稀里糊涂??? */
static finaliseSlot_ret_t finaliseSlot(cte_t *slot, bool_t immediate)
{
    bool_t final;
    finaliseCap_ret_t fc_ret;
    exception_t status;
    finaliseSlot_ret_t ret;
    /*Z 递归清理 */
    while (cap_get_capType(slot->cap) != cap_null_cap) {
        final = isFinalCapability(slot);/*Z 是否对资源的最后一个引用 */
        fc_ret = finaliseCap(slot->cap, final, false);/*Z 根据能力种类做不同的清理工作，返回后续还需要的清理工作 */
        /*Z 空能力、容量为0的Zombie能力、CSlot是Zombie能力的唯一内容，这些是可删除的，因此finalize完毕 */
        if (capRemovable(fc_ret.remainder, slot)) {
            ret.status = EXCEPTION_NONE;
            ret.success = true;
            ret.cleanupInfo = fc_ret.cleanupInfo;/*Z 后续需要清理的 */
            return ret;
        }
        /*Z 继续剩余的finalize：容量不为0的Zombie */
        slot->cap = fc_ret.remainder;
        /*Z 不立即删除的且该CSlot已可再利用，结束finalize并做好标记 */
        if (!immediate && capCyclicZombie(fc_ret.remainder, slot)) {
            ret.status = EXCEPTION_NONE;
            ret.success = false;/*Z 这里有个false标记 */
            ret.cleanupInfo = fc_ret.cleanupInfo;
            return ret;
        }
        /*Z 删除Zombie CNode的一个能力，或与原能力交换一个能力以便后续再删除 */
        status = reduceZombie(slot, immediate);
        if (status != EXCEPTION_NONE) {
            ret.status = status;
            ret.success = false;
            ret.cleanupInfo = cap_null_cap_new();
            return ret;
        }
        /*Z 检查抢占点 */
        status = preemptionPoint();
        if (status != EXCEPTION_NONE) {
            ret.status = status;
            ret.success = false;
            ret.cleanupInfo = cap_null_cap_new();
            return ret;
        }
    }
    ret.status = EXCEPTION_NONE;
    ret.success = true;
    ret.cleanupInfo = cap_null_cap_new();
    return ret;
}
/*Z 删除Zombie CNode的一个能力，或与原能力交换一个能力以便后续再删除 */
static exception_t reduceZombie(cte_t *slot, bool_t immediate)
{
    cte_t *ptr;
    word_t n, type;
    exception_t status;

    assert(cap_get_capType(slot->cap) == cap_zombie_cap);
    ptr = (cte_t *)cap_zombie_cap_get_capZombiePtr(slot->cap);/*Z 原能力地址 */
    n = cap_zombie_cap_get_capZombieNumber(slot->cap);/*Z 原能力容量 */
    type = cap_zombie_cap_get_capZombieType(slot->cap);/*Z 原能力类型 */

    /* Haskell error: "reduceZombie: expected unremovable zombie" */
    assert(n > 0);

    if (immediate) {
        cte_t *endSlot = &ptr[n - 1];/*Z 原能力最后一个CSlot */
        /*Z 删除它 */
        status = cteDelete(endSlot, false);
        if (status != EXCEPTION_NONE) {
            return status;
        }

        switch (cap_get_capType(slot->cap)) {
        case cap_null_cap:
            break;

        case cap_zombie_cap: {
            cte_t *ptr2 =
                (cte_t *)cap_zombie_cap_get_capZombiePtr(slot->cap);

            if (ptr == ptr2 &&
                cap_zombie_cap_get_capZombieNumber(slot->cap) == n &&
                cap_zombie_cap_get_capZombieType(slot->cap) == type) {
                assert(cap_get_capType(endSlot->cap) == cap_null_cap);
                slot->cap =
                    cap_zombie_cap_set_capZombieNumber(slot->cap, n - 1);
            } else {
                /* Haskell error:
                 * "Expected new Zombie to be self-referential."
                 */
                assert(ptr2 == slot && ptr != slot);
            }
            break;
        }

        default:
            fail("Expected recursion to result in Zombie.");
        }
    } else {/*Z 不立即删除，但容量大于1 */
        /* Haskell error: "Cyclic zombie passed to unexposed reduceZombie" */
        assert(ptr != slot);

        if (cap_get_capType(ptr->cap) == cap_zombie_cap) {
            /* Haskell error: "Moving self-referential Zombie aside." */
            assert(ptr != CTE_PTR(cap_zombie_cap_get_capZombiePtr(ptr->cap)));
        }
        /*Z 交换两个CSlot的能力和关联。原能力处变成了Zombie，参数的Zombie处接收了一个原CSlot */
        capSwapForDelete(ptr, slot);
    }
    return EXCEPTION_NONE;
}
/*Z 删除指定的CSlot */
void cteDeleteOne(cte_t *slot)
{
    word_t cap_type = cap_get_capType(slot->cap);
    if (cap_type != cap_null_cap) {
        bool_t final;
        finaliseCap_ret_t fc_ret UNUSED;

        /** GHOSTUPD: "(gs_get_assn cteDeleteOne_'proc \<acute>ghost'state = (-1)
            \<or> gs_get_assn cteDeleteOne_'proc \<acute>ghost'state = \<acute>cap_type, id)" */
        /*Z CSlot是否为其对象(资源)的最后一个能力 */
        final = isFinalCapability(slot);
        fc_ret = finaliseCap(slot->cap, final, true);/*Z 做清理工作 */
        /* Haskell error: "cteDeleteOne: cap should be removable" */
        assert(capRemovable(fc_ret.remainder, slot) &&/*Z 能力必须要是可删除的 */
               cap_get_capType(fc_ret.cleanupInfo) == cap_null_cap);/*Z 且没有剩余要处理的 */
        emptySlot(slot, cap_null_cap_new());/*Z 摘出CSlot关联链条 */
    }
}
/*Z slot赋值能力并插入到父CSlot关联链的头，置首个、可撤销标记 */
void insertNewCap(cte_t *parent, cte_t *slot, cap_t cap)
{
    cte_t *next;

    next = CTE_PTR(mdb_node_get_mdbNext(parent->cteMDBNode));
    slot->cap = cap;            /*Z 这意味着一条CSlot关联链中可有多个节点有首标记 */
    slot->cteMDBNode = mdb_node_new(CTE_REF(next), true, true, CTE_REF(parent));
    if (next) {
        mdb_node_ptr_set_mdbPrev(&next->cteMDBNode, CTE_REF(slot));
    }
    mdb_node_ptr_set_mdbNext(&parent->cteMDBNode, CTE_REF(slot));
}

#ifndef CONFIG_KERNEL_MCS
/*Z 如果线程未设置回复能力，则设置为主叫、允许授权回复、可撤销、首个标记 */
void setupReplyMaster(tcb_t *thread)
{
    cte_t *slot;

    slot = TCB_PTR_CTE_PTR(thread, tcbReply);
    if (cap_get_capType(slot->cap) == cap_null_cap) {
        /* Haskell asserts that no reply caps exist for this thread here. This
         * cannot be translated. */
        slot->cap = cap_reply_cap_new(true, true, TCB_REF(thread));
        slot->cteMDBNode = nullMDBNode;
        mdb_node_ptr_set_mdbRevocable(&slot->cteMDBNode, true);
        mdb_node_ptr_set_mdbFirstBadged(&slot->cteMDBNode, true);
    }
}
#endif
/*Z 判断CSlot a、b是否父子关系 */
bool_t PURE isMDBParentOf(cte_t *cte_a, cte_t *cte_b)
{
    if (!mdb_node_get_mdbRevocable(cte_a->cteMDBNode)) {/*Z 不可撤销的不是 */
        return false;
    }
    if (!sameRegionAs(cte_a->cap, cte_b->cap)) {/*Z 能力相关内存不同的不是 */
        return false;
    }
    switch (cap_get_capType(cte_a->cap)) {
    case cap_endpoint_cap: {
        word_t badge;

        badge = cap_endpoint_cap_get_capEPBadge(cte_a->cap);
        if (badge == 0) {/*Z a是端点能力且无标记则是 */
            return true;
        }   /*Z 标记相同且b不是首个标记的则是 */
        return (badge == cap_endpoint_cap_get_capEPBadge(cte_b->cap)) &&
               !mdb_node_get_mdbFirstBadged(cte_b->cteMDBNode);
        break;
    }

    case cap_notification_cap: {
        word_t badge;

        badge = cap_notification_cap_get_capNtfnBadge(cte_a->cap);
        if (badge == 0) {/*Z a是通知能力且无标记则是 */
            return true;
        }   /*Z 标记相同且b不是首个标记的则是 */
        return
            (badge == cap_notification_cap_get_capNtfnBadge(cte_b->cap)) &&
            !mdb_node_get_mdbFirstBadged(cte_b->cteMDBNode);
        break;
    }

    default:    /*Z 默认是 */
        return true;
        break;  /*Z 不好 */
    }
}
/*Z 确认CSlot无子CSlot */
exception_t ensureNoChildren(cte_t *slot)
{       /*Z 获取CSlot关联的下一个CSlot */
    if (mdb_node_get_mdbNext(slot->cteMDBNode) != 0) {
        cte_t *next;
                        /*Z MDB的u64[1]末两位置0 */
        next = CTE_PTR(mdb_node_get_mdbNext(slot->cteMDBNode));
        if (isMDBParentOf(slot, next)) {/*Z 判断CSlots是否父子关系 */
            current_syscall_error.type = seL4_RevokeFirst;
            return EXCEPTION_SYSCALL_ERROR;
        }
    }

    return EXCEPTION_NONE;
}
/*Z CSlot是否为空 */
exception_t ensureEmptySlot(cte_t *slot)
{
    if (cap_get_capType(slot->cap) != cap_null_cap) {
        current_syscall_error.type = seL4_DeleteFirst;
        return EXCEPTION_SYSCALL_ERROR;
    }

    return EXCEPTION_NONE;
}
/*Z CSlot是否为其对象(资源)的最后一个能力 */
bool_t PURE isFinalCapability(cte_t *cte)
{
    mdb_node_t mdb;
    bool_t prevIsSameObject;

    mdb = cte->cteMDBNode;
    /*Z 确定与父CSlot所指是否为同一对象 */
    if (mdb_node_get_mdbPrev(mdb) == 0) {
        prevIsSameObject = false;
    } else {
        cte_t *prev;

        prev = CTE_PTR(mdb_node_get_mdbPrev(mdb));
        prevIsSameObject = sameObjectAs(prev->cap, cte->cap);/*Z 两个能力指的是否同一对象 */
    }

    if (prevIsSameObject) {
        return false;
    } else {
        if (mdb_node_get_mdbNext(mdb) == 0) {
            return true;
        } else {
            cte_t *next;

            next = CTE_PTR(mdb_node_get_mdbNext(mdb));
            return !sameObjectAs(cte->cap, next->cap);
        }
    }
}
/*Z 孤立的TCB、CNode、Zombie能力是已删除能力。可以这样理解，任何一个能力都应直接或间接衍生于rootserver，因此未显式删除时就不是已删除的；
而可能存在rootserver创造的“独立”能力，即不与rootserver建立关联，当它们的所有子孙消失后，自己也就消失了，也就是已经删除了 */
bool_t PURE slotCapLongRunningDelete(cte_t *slot)
{
    if (cap_get_capType(slot->cap) == cap_null_cap) {/*Z 如果是空能力，则否 */
        return false;
    } else if (! isFinalCapability(slot)) {/*Z 如果不是空能力也不是最后的能力，则否 */
        return false;
    }
    switch (cap_get_capType(slot->cap)) {/*Z 如果是最后的能力 */
    case cap_thread_cap:
    case cap_zombie_cap:
    case cap_cnode_cap:
        return true;
    default:
        return false;
    }
}
/*Z 从线程IPC buffer固定位置获取指示，查找能力接收CSlot地址(空能力) */
/* This implementation is specialised to the (current) limit
 * of one cap receive slot. */
cte_t *getReceiveSlots(tcb_t *thread, word_t *buffer)
{
    cap_transfer_t ct;
    cptr_t cptr;
    lookupCap_ret_t luc_ret;
    lookupSlot_ret_t lus_ret;
    cte_t *slot;
    cap_t cnode;

    if (!buffer) {
        return NULL;
    }
    /*Z 从IPC buffer的固定位置获取能力接收位置指示 */
    ct = loadCapTransfer(buffer);
    cptr = ct.ctReceiveRoot;
    /*Z 查找接收CNode */
    luc_ret = lookupCap(thread, cptr);
    if (luc_ret.status != EXCEPTION_NONE) {
        return NULL;
    }
    cnode = luc_ret.cap;
    /*Z 查找最终CSlot地址 */
    lus_ret = lookupTargetSlot(cnode, ct.ctReceiveIndex, ct.ctReceiveDepth);
    if (lus_ret.status != EXCEPTION_NONE) {
        return NULL;
    }
    slot = lus_ret.slot;
    /*Z 必须是空白能力 */
    if (cap_get_capType(slot->cap) != cap_null_cap) {
        return NULL;
    }

    return slot;
}
/*Z 从IPC buffer的固定位置获取能力接收位置指示 */
cap_transfer_t PURE loadCapTransfer(word_t *buffer)
{
    const int offset = seL4_MsgMaxLength + seL4_MsgMaxExtraCaps + 2;
    return capTransferFromWords(buffer + offset);
}
