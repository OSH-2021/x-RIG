/*Z
CNode层级(必须是cap_cnode_cap连接起来)：
                                            一级                    二级            ...
                                            CSlot0            ->    CSlot0
                                            CSlot1           /      ...
                                            ...             /   --> CSlotm
                                     -->    CSlotn(CNode能力)  |    ...
                                    |       ...                |
                                    |                          |
                                    |                          |
                                    |                          |
CSlot句柄(64位字)：  一级保护位 一级索引位 二级保护位 二级索引位 ...
*/
/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <types.h>
#include <object.h>
#include <api/failures.h>
#include <kernel/thread.h>
#include <kernel/cspace.h>
#include <model/statedata.h>
#include <arch/machine.h>
/*Z 查找线程中CSlot句柄指示的CSlot的能力 */
lookupCap_ret_t lookupCap(tcb_t *thread, cptr_t cPtr)
{
    lookupSlot_raw_ret_t lu_ret;
    lookupCap_ret_t ret;
    /*Z 查找线程中CSlot句柄指示的CSlot */
    lu_ret = lookupSlot(thread, cPtr);
    if (unlikely(lu_ret.status != EXCEPTION_NONE)) {
        ret.status = lu_ret.status;
        ret.cap = cap_null_cap_new();
        return ret;
    }

    ret.status = EXCEPTION_NONE;
    ret.cap = lu_ret.slot->cap;
    return ret;
}
/*Z 查询线程中CSlot句柄指示的CSlot及其能力 */
lookupCapAndSlot_ret_t lookupCapAndSlot(tcb_t *thread, cptr_t cPtr)
{
    lookupSlot_raw_ret_t lu_ret;
    lookupCapAndSlot_ret_t ret;
    /*Z 查找线程中CSlot句柄指示的CSlot */
    lu_ret = lookupSlot(thread, cPtr);
    if (unlikely(lu_ret.status != EXCEPTION_NONE)) {
        ret.status = lu_ret.status;
        ret.slot = NULL;
        ret.cap = cap_null_cap_new();
        return ret;
    }

    ret.status = EXCEPTION_NONE;
    ret.slot = lu_ret.slot;
    ret.cap = lu_ret.slot->cap;
    return ret;
}
/*Z 在指定线程CSpace中按机器字浓度查找句柄指示的CSlot */
lookupSlot_raw_ret_t lookupSlot(tcb_t *thread, cptr_t capptr)
{
    cap_t threadRoot;
    resolveAddressBits_ret_t res_ret;
    lookupSlot_raw_ret_t ret;
    /*Z 自根CNode开始，解析参数指示的CSlot地址，直至达到“叶子”CSlot或指示位用完 */
    threadRoot = TCB_PTR_CTE_PTR(thread, tcbCTable)->cap;
    res_ret = resolveAddressBits(threadRoot, capptr, wordBits);

    ret.status = res_ret.status;
    ret.slot = res_ret.slot;
    return ret;
}
/*Z 自CNode能力开始，按CSlot句柄中右数depth位数的指示，查找最终CSlot地址。isSource标识是否IPC源方，仅用于错误标识 */
lookupSlot_ret_t lookupSlotForCNodeOp(bool_t isSource, cap_t root, cptr_t capptr,
                                      word_t depth)
{
    resolveAddressBits_ret_t res_ret;
    lookupSlot_ret_t ret;

    ret.slot = NULL;
    /*Z 能力参数必须是CNode能力 */
    if (unlikely(cap_get_capType(root) != cap_cnode_cap)) {
        current_syscall_error.type = seL4_FailedLookup;
        current_syscall_error.failedLookupWasSource = isSource;
        current_lookup_fault = lookup_fault_invalid_root_new();
        ret.status = EXCEPTION_SYSCALL_ERROR;
        return ret;
    }
    /*Z 位深度指示要在要求范围内 */
    if (unlikely(depth < 1 || depth > wordBits)) {
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = 1;
        current_syscall_error.rangeErrorMax = wordBits;
        ret.status = EXCEPTION_SYSCALL_ERROR;
        return ret;
    }/*Z 自root开始，按capptr CSlot句柄中右数depth位数的指示，解析CSlot地址，直至达到“叶子”CSlot或指示位用完 */
    res_ret = resolveAddressBits(root, capptr, depth);
    if (unlikely(res_ret.status != EXCEPTION_NONE)) {
        current_syscall_error.type = seL4_FailedLookup;
        current_syscall_error.failedLookupWasSource = isSource;
        /* current_lookup_fault will have been set by resolveAddressBits */
        ret.status = EXCEPTION_SYSCALL_ERROR;
        return ret;
    }
    /*Z 有剩余位说明，要么depth超过了所有层级，要么capptr句柄提前解析到了非CNode能力的“叶子”CSlot */
    if (unlikely(res_ret.bitsRemaining != 0)) {
        current_syscall_error.type = seL4_FailedLookup;
        current_syscall_error.failedLookupWasSource = isSource;
        current_lookup_fault =
            lookup_fault_depth_mismatch_new(0, res_ret.bitsRemaining);
        ret.status = EXCEPTION_SYSCALL_ERROR;
        return ret;
    }

    ret.slot = res_ret.slot;
    ret.status = EXCEPTION_NONE;
    return ret;
}
/*Z 自IPC源方CNode能力开始，按CSlot句柄中右数depth位数的指示，查找最终CSlot地址 */
lookupSlot_ret_t lookupSourceSlot(cap_t root, cptr_t capptr, word_t depth)
{
    return lookupSlotForCNodeOp(true, root, capptr, depth);
}
/*Z 自IPC目标方CNode能力开始，按CSlot句柄中右数depth位数的指示，查找最终CSlot地址 */
lookupSlot_ret_t lookupTargetSlot(cap_t root, cptr_t capptr, word_t depth)
{
    return lookupSlotForCNodeOp(false, root, capptr, depth);
}
/*Z 自CNode能力开始，按CSlot句柄中右数depth位数的指示，查找最终CSlot地址 */
lookupSlot_ret_t lookupPivotSlot(cap_t root, cptr_t capptr, word_t depth)
{
    return lookupSlotForCNodeOp(true, root, capptr, depth);
}
/*Z 自nodeCap能力开始，按capptr CSlot句柄中右数n_bits位数的指示，解析CSlot地址，直至达到“叶子”CSlot或指示位用完 */
resolveAddressBits_ret_t resolveAddressBits(cap_t nodeCap, cptr_t capptr, word_t n_bits)
{
    resolveAddressBits_ret_t ret;
    word_t radixBits, guardBits, levelBits, guard;
    word_t capGuard, offset;
    cte_t *slot;

    ret.bitsRemaining = n_bits;
    ret.slot = NULL;
    /*Z 给定的能力不是CNode访问能力 */
    if (unlikely(cap_get_capType(nodeCap) != cap_cnode_cap)) {
        current_lookup_fault = lookup_fault_invalid_root_new();
        ret.status = EXCEPTION_LOOKUP_FAULT;
        return ret;
    }

    while (1) {
        radixBits = cap_cnode_cap_get_capCNodeRadix(nodeCap);       /*Z 获取能力的CSlot索引位位数 */
        guardBits = cap_cnode_cap_get_capCNodeGuardSize(nodeCap);   /*Z 获取能力的保护位位数 */
        levelBits = radixBits + guardBits;                          /*Z 获取能力所在层级的总位数 */

        /* Haskell error: "All CNodes must resolve bits" */
        assert(levelBits != 0);

        capGuard = cap_cnode_cap_get_capCNodeGuard(nodeCap);        /*Z 获取能力的保护位 */

        /* sjw --- the MASK(5) here is to avoid the case where n_bits = 32
           and guardBits = 0, as it violates the C spec to >> by more
           than 31 */
                /*Z 获取capptr中的保护位。这个时候capptr中的有效位必须是靠右排列的 */
        guard = (capptr >> ((n_bits - guardBits) & MASK(wordRadix))) & MASK(guardBits);
        if (unlikely(guardBits > n_bits || guard != capGuard)) {
            current_lookup_fault =  /*Z 保护位错误 */
                lookup_fault_guard_mismatch_new(capGuard, n_bits, guardBits);
            ret.status = EXCEPTION_LOOKUP_FAULT;
            return ret;
        }

        if (unlikely(levelBits > n_bits)) {
            current_lookup_fault =  /*Z CNode层级错误 */
                lookup_fault_depth_mismatch_new(levelBits, n_bits);
            ret.status = EXCEPTION_LOOKUP_FAULT;
            return ret;
        }

        offset = (capptr >> (n_bits - levelBits)) & MASK(radixBits);/*Z 获取capptr中的索引值 */
        slot = CTE_PTR(cap_cnode_cap_get_capCNodePtr(nodeCap)) + offset;/*Z 获取该索引值对应的CSlot */

        if (likely(n_bits <= levelBits)) {/*Z 解析位恰好用完(小于的情况上面已排除)，结束 */
            ret.status = EXCEPTION_NONE;
            ret.slot = slot;
            ret.bitsRemaining = 0;
            return ret;
        }

        /** GHOSTUPD: "(\<acute>levelBits > 0, id)" */
        /*Z 递进至下一级 */
        n_bits -= levelBits;
        nodeCap = slot->cap;
        /*Z 下一级的能力不是CNode能力，则此“下一级”是叶子CSlot，结束 */
        if (unlikely(cap_get_capType(nodeCap) != cap_cnode_cap)) {
            ret.status = EXCEPTION_NONE;
            ret.slot = slot;
            ret.bitsRemaining = n_bits;
            return ret;
        }
    }
    /*Z 不好：永远到不了的位置 */
    ret.status = EXCEPTION_NONE;
    return ret;
}
