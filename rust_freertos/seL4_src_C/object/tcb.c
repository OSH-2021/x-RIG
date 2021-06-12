/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <config.h>
#include <types.h>
#include <api/failures.h>
#include <api/invocation.h>
#include <api/syscall.h>
#include <sel4/shared_types.h>
#include <machine/io.h>
#include <object/structures.h>
#include <object/objecttype.h>
#include <object/cnode.h>
#ifdef CONFIG_KERNEL_MCS
#include <object/schedcontext.h>
#endif
#include <object/tcb.h>
#include <kernel/cspace.h>
#include <kernel/thread.h>
#include <kernel/vspace.h>
#include <model/statedata.h>
#include <util.h>
#include <string.h>
#include <stdint.h>
#include <arch/smp/ipi_inline.h>

#define NULL_PRIO 0
/*Z 检查优先级不超过线程最大可控优先级 */
static exception_t checkPrio(prio_t prio, tcb_t *auth)
{
    prio_t mcp;

    mcp = auth->tcbMCP;

    /* system invariant: existing MCPs are bounded */
    assert(mcp <= seL4_MaxPrio);

    /* can't assign a priority greater than our own mcp */
    if (prio > mcp) {
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = seL4_MinPrio;
        current_syscall_error.rangeErrorMax = mcp;
        return EXCEPTION_SYSCALL_ERROR;
    }

    return EXCEPTION_NONE;
}
/*Z 指定cpu的指定域的指定优先级位图置1 */
static inline void addToBitmap(word_t cpu, word_t dom, word_t prio)
{
    word_t l1index;
    word_t l1index_inverted;

    l1index = prio_to_l1index(prio);/*Z 一级位图索引 */
    l1index_inverted = invert_l1index(l1index);/*Z 二级位图索引 */

    NODE_STATE_ON_CORE(ksReadyQueuesL1Bitmap[dom], cpu) |= BIT(l1index);/*Z 更新一级位图 */
    /* we invert the l1 index when accessed the 2nd level of the bitmap in
       order to increase the liklihood that high prio threads l2 index word will
       be on the same cache line as the l1 index word - this makes sure the
       fastpath is fastest for high prio threads *//*Z 更新二级位图 */
    NODE_STATE_ON_CORE(ksReadyQueuesL2Bitmap[dom][l1index_inverted], cpu) |= BIT(prio & MASK(wordRadix));
}
/*Z 指定cpu的指定域的指定优先级位图置0 */
static inline void removeFromBitmap(word_t cpu, word_t dom, word_t prio)
{
    word_t l1index;
    word_t l1index_inverted;

    l1index = prio_to_l1index(prio);
    l1index_inverted = invert_l1index(l1index);
    NODE_STATE_ON_CORE(ksReadyQueuesL2Bitmap[dom][l1index_inverted], cpu) &= ~BIT(prio & MASK(wordRadix));
    if (unlikely(!NODE_STATE_ON_CORE(ksReadyQueuesL2Bitmap[dom][l1index_inverted], cpu))) {
        NODE_STATE_ON_CORE(ksReadyQueuesL1Bitmap[dom], cpu) &= ~BIT(l1index);
    }
}
/*Z 将线程加入ready队列头 */
/* Add TCB to the head of a scheduler queue */
void tcbSchedEnqueue(tcb_t *tcb)
{
#ifdef CONFIG_KERNEL_MCS
    assert(isSchedulable(tcb));/*Z 可调度执行 */
    assert(refill_sufficient(tcb->tcbSchedContext, 0));/*Z 可运行时间足够进出内核 */
#endif

    if (!thread_state_get_tcbQueued(tcb->tcbState)) {/*Z 不在调度队列中 */
        tcb_queue_t queue;
        dom_t dom;
        prio_t prio;
        word_t idx;
        /*Z 找到亲和cpu的可运行队列 */
        dom = tcb->tcbDomain;
        prio = tcb->tcbPriority;
        idx = ready_queues_index(dom, prio);/*Z 用域和优先级计算可运行队列索引 */
        queue = NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity);
        /*Z 插入队列头 */
        if (!queue.end) { /* Empty list */
            queue.end = tcb;                /*Z 更新优先级队列位图 */
            addToBitmap(SMP_TERNARY(tcb->tcbAffinity, 0), dom, prio);
        } else {
            queue.head->tcbSchedPrev = tcb;
        }
        tcb->tcbSchedPrev = NULL;
        tcb->tcbSchedNext = queue.head;
        queue.head = tcb;

        NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity) = queue;

        thread_state_ptr_set_tcbQueued(&tcb->tcbState, true);
    }
}
/*Z 将线程加入ready队列尾 */
/* Add TCB to the end of a scheduler queue */
void tcbSchedAppend(tcb_t *tcb)
{
#ifdef CONFIG_KERNEL_MCS
    assert(isSchedulable(tcb));
    assert(refill_sufficient(tcb->tcbSchedContext, 0));
    assert(refill_ready(tcb->tcbSchedContext));
#endif
    if (!thread_state_get_tcbQueued(tcb->tcbState)) {
        tcb_queue_t queue;
        dom_t dom;
        prio_t prio;
        word_t idx;

        dom = tcb->tcbDomain;
        prio = tcb->tcbPriority;
        idx = ready_queues_index(dom, prio);
        queue = NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity);

        if (!queue.head) { /* Empty list */
            queue.head = tcb;
            addToBitmap(SMP_TERNARY(tcb->tcbAffinity, 0), dom, prio);
        } else {
            queue.end->tcbSchedNext = tcb;
        }
        tcb->tcbSchedPrev = queue.end;
        tcb->tcbSchedNext = NULL;
        queue.end = tcb;

        NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity) = queue;

        thread_state_ptr_set_tcbQueued(&tcb->tcbState, true);
    }
}
/*Z 从ready队列中摘出线程 */
/* Remove TCB from a scheduler queue */
void tcbSchedDequeue(tcb_t *tcb)
{
    if (thread_state_get_tcbQueued(tcb->tcbState)) {/*Z 还在ready队列里 */
        tcb_queue_t queue;
        dom_t dom;
        prio_t prio;
        word_t idx;

        dom = tcb->tcbDomain;
        prio = tcb->tcbPriority;
        idx = ready_queues_index(dom, prio);/*Z 用域和优先级计算可运行队列索引 */
        queue = NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity);/*Z ready调度队列 */

        if (tcb->tcbSchedPrev) {
            tcb->tcbSchedPrev->tcbSchedNext = tcb->tcbSchedNext;
        } else {
            queue.head = tcb->tcbSchedNext;
            if (likely(!tcb->tcbSchedNext)) {/*Z 线程是该队列的唯一线程 */
                removeFromBitmap(SMP_TERNARY(tcb->tcbAffinity, 0), dom, prio);
            }
        }

        if (tcb->tcbSchedNext) {
            tcb->tcbSchedNext->tcbSchedPrev = tcb->tcbSchedPrev;
        } else {
            queue.end = tcb->tcbSchedPrev;
        }

        NODE_STATE_ON_CORE(ksReadyQueues[idx], tcb->tcbAffinity) = queue;

        thread_state_ptr_set_tcbQueued(&tcb->tcbState, false);
    }
}

#ifdef CONFIG_DEBUG_BUILD
/*Z 将线程插入到亲和cpu的DEBUG线程双向链表头 */
void tcbDebugAppend(tcb_t *tcb)
{
    /* prepend to the list */
    tcb->tcbDebugPrev = NULL;

    tcb->tcbDebugNext = NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity);

    if (NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity)) {
        NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity)->tcbDebugPrev = tcb;
    }

    NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity) = tcb;
}
/*Z 将线程从DEBUG队列中摘出 */
void tcbDebugRemove(tcb_t *tcb)
{
    assert(NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity) != NULL);
    if (tcb == NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity)) {/*Z 正在亲和cpu上运行则换下一个 */
        NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity) = NODE_STATE_ON_CORE(ksDebugTCBs, tcb->tcbAffinity)->tcbDebugNext;
    } else {
        assert(tcb->tcbDebugPrev);
        tcb->tcbDebugPrev->tcbDebugNext = tcb->tcbDebugNext;
    }

    if (tcb->tcbDebugNext) {
        tcb->tcbDebugNext->tcbDebugPrev = tcb->tcbDebugPrev;
    }

    tcb->tcbDebugPrev = NULL;
    tcb->tcbDebugNext = NULL;
}
#endif /* CONFIG_DEBUG_BUILD */

#ifndef CONFIG_KERNEL_MCS
/* Add TCB to the end of an endpoint queue */
/*Z 将线程加入到队列尾 */
tcb_queue_t tcbEPAppend(tcb_t *tcb, tcb_queue_t queue)
{
    if (!queue.head) { /* Empty list */
        queue.head = tcb;
    } else {
        queue.end->tcbEPNext = tcb;
    }
    tcb->tcbEPPrev = queue.end;
    tcb->tcbEPNext = NULL;
    queue.end = tcb;

    return queue;
}
#endif
/*Z 将线程从EP或NF队列中摘出 */
/* Remove TCB from an endpoint queue */
tcb_queue_t tcbEPDequeue(tcb_t *tcb, tcb_queue_t queue)
{
    if (tcb->tcbEPPrev) {
        tcb->tcbEPPrev->tcbEPNext = tcb->tcbEPNext;
    } else {
        queue.head = tcb->tcbEPNext;
    }

    if (tcb->tcbEPNext) {
        tcb->tcbEPNext->tcbEPPrev = tcb->tcbEPPrev;
    } else {
        queue.end = tcb->tcbEPPrev;
    }

    return queue;
}

#ifdef CONFIG_KERNEL_MCS
/*Z 移出release队列，视情更新全局变量、重设定时器 */
void tcbReleaseRemove(tcb_t *tcb)
{
    if (likely(thread_state_get_tcbInReleaseQueue(tcb->tcbState))) {
        if (tcb->tcbSchedPrev) {
            tcb->tcbSchedPrev->tcbSchedNext = tcb->tcbSchedNext;
        } else {
            NODE_STATE_ON_CORE(ksReleaseHead, tcb->tcbAffinity) = tcb->tcbSchedNext;
            /* the head has changed, we might need to set a new timeout */
            NODE_STATE_ON_CORE(ksReprogram, tcb->tcbAffinity) = true;
        }

        if (tcb->tcbSchedNext) {
            tcb->tcbSchedNext->tcbSchedPrev = tcb->tcbSchedPrev;
        }

        tcb->tcbSchedNext = NULL;
        tcb->tcbSchedPrev = NULL;
        thread_state_ptr_set_tcbInReleaseQueue(&tcb->tcbState, false);
    }
}
/*Z 将线程加入待充值队列，置线程状态标识位 */
void tcbReleaseEnqueue(tcb_t *tcb)
{
    assert(thread_state_get_tcbInReleaseQueue(tcb->tcbState) == false);
    assert(thread_state_get_tcbQueued(tcb->tcbState) == false);

    tcb_t *before = NULL;
    tcb_t *after = NODE_STATE_ON_CORE(ksReleaseHead, tcb->tcbAffinity);

    /* find our place in the ordered queue */
    while (after != NULL &&
           REFILL_HEAD(tcb->tcbSchedContext).rTime >= REFILL_HEAD(after->tcbSchedContext).rTime) {
        before = after;
        after = after->tcbSchedNext;
    }

    if (before == NULL) {
        /* insert at head */
        NODE_STATE_ON_CORE(ksReleaseHead, tcb->tcbAffinity) = tcb;
        NODE_STATE_ON_CORE(ksReprogram, tcb->tcbAffinity) = true;
    } else {
        before->tcbSchedNext = tcb;
    }

    if (after != NULL) {
        after->tcbSchedPrev = tcb;
    }

    tcb->tcbSchedNext = after;
    tcb->tcbSchedPrev = before;

    thread_state_ptr_set_tcbInReleaseQueue(&tcb->tcbState, true);
}
/*Z 待充值链表出表头 */
tcb_t *tcbReleaseDequeue(void)
{
    assert(NODE_STATE(ksReleaseHead) != NULL);
    assert(NODE_STATE(ksReleaseHead)->tcbSchedPrev == NULL);
    SMP_COND_STATEMENT(assert(NODE_STATE(ksReleaseHead)->tcbAffinity == getCurrentCPUIndex()));
    /*Z 取出待充值队列头 */
    tcb_t *detached_head = NODE_STATE(ksReleaseHead);
    NODE_STATE(ksReleaseHead) = NODE_STATE(ksReleaseHead)->tcbSchedNext;

    if (NODE_STATE(ksReleaseHead)) {
        NODE_STATE(ksReleaseHead)->tcbSchedPrev = NULL;
    }

    if (detached_head->tcbSchedNext) {
        detached_head->tcbSchedNext->tcbSchedPrev = NULL;/*Z 不好：上条已处理完了 */
        detached_head->tcbSchedNext = NULL;
    }

    thread_state_ptr_set_tcbInReleaseQueue(&detached_head->tcbState, false);
    NODE_STATE(ksReprogram) = true;/*Z 估计是重设调度计时器????? */

    return detached_head;
}
#endif
/*Z 从IPC buffer最大消息容量+2处取出第i个extraCap */
cptr_t PURE getExtraCPtr(word_t *bufferPtr, word_t i)
{
    return (cptr_t)bufferPtr[seL4_MsgMaxLength + 2 + i];
}
/*Z 将badge写入IPC buffer中第i个extraCaps位置 */
void setExtraBadge(word_t *bufferPtr, word_t badge,
                   word_t i)
{
    bufferPtr[seL4_MsgMaxLength + 2 + i] = badge;
}

#ifndef CONFIG_KERNEL_MCS
/*Z 设置发送者为阻塞于接收回复状态，为接收者创建拷贝指向发送者的回复能力。canGrant来自接收者 */
void setupCallerCap(tcb_t *sender, tcb_t *receiver, bool_t canGrant)
{
    cte_t *replySlot, *callerSlot;
    cap_t masterCap UNUSED, callerCap UNUSED;

    setThreadState(sender, ThreadState_BlockedOnReply);                 /*Z 设置发送者为阻塞于接收回复 */
    replySlot = TCB_PTR_CTE_PTR(sender, tcbReply);
    masterCap = replySlot->cap;                                         /*Z 取出发送者的tcbReply回复能力 */
    /* Haskell error: "Sender must have a valid master reply cap" */
    assert(cap_get_capType(masterCap) == cap_reply_cap);                /*Z                         能力类型是cap_reply_cap */
    assert(cap_reply_cap_get_capReplyMaster(masterCap));                /*Z                         是要求方 */
    assert(cap_reply_cap_get_capReplyCanGrant(masterCap));              /*Z                         能授予别人 */
    assert(TCB_PTR(cap_reply_cap_get_capTCBPtr(masterCap)) == sender);  /*Z                         TCB指针是自己 */
    callerSlot = TCB_PTR_CTE_PTR(receiver, tcbCaller);
    callerCap = callerSlot->cap;                                        /*Z 取出接收者的tcbCaller能力 */
    /* Haskell error: "Caller cap must not already exist" */
    assert(cap_get_capType(callerCap) == cap_null_cap);                 /*Z                      能力类型是空能力 */
    cteInsert(cap_reply_cap_new(canGrant, false, TCB_REF(sender)),
              replySlot, callerSlot);                                   /*Z 创建指向发送者的回复能力，拷贝给接收者 */
}
/*Z 删除线程的tcbCaller回复能力 */
void deleteCallerCap(tcb_t *receiver)
{
    cte_t *callerSlot;

    callerSlot = TCB_PTR_CTE_PTR(receiver, tcbCaller);
    /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (ucast cap_reply_cap))" */
    cteDeleteOne(callerSlot);/*Z 删除指定的CSlot */
}
#endif
/*Z 全局性的当前extraCaps */
extra_caps_t current_extra_caps;
/*Z 根据消息指示，从线程的IPC buffer中取出extraCaps句柄并在其CSpace中按机器字深度查找对应的CSlot指针，赋予全局变量 */
exception_t lookupExtraCaps(tcb_t *thread, word_t *bufferPtr, seL4_MessageInfo_t info)
{
    lookupSlot_raw_ret_t lu_ret;
    cptr_t cptr;
    word_t i, length;

    if (!bufferPtr) {/*Z 这一点说明：extraCaps通过IPC buffer传递 */
        current_extra_caps.excaprefs[0] = NULL;
        return EXCEPTION_NONE;
    }
    /*Z 从给定的消息中获取extraCaps的数量 */
    length = seL4_MessageInfo_get_extraCaps(info);

    for (i = 0; i < length; i++) {
        cptr = getExtraCPtr(bufferPtr, i);/*Z 从IPC buffer最大消息容量+2处逐一取出extraCaps句柄 */
        /*Z 在指定线程CSpace中按机器字深度查找句柄指示的CSlot */
        lu_ret = lookupSlot(thread, cptr);
        if (lu_ret.status != EXCEPTION_NONE) {
            current_fault = seL4_Fault_CapFault_new(cptr, false);
            return lu_ret.status;
        }

        current_extra_caps.excaprefs[i] = lu_ret.slot;
    }
    if (i < seL4_MsgMaxExtraCaps) {
        current_extra_caps.excaprefs[i] = NULL;
    }

    return EXCEPTION_NONE;
}
/*Z 就是个copy，返回实际拷贝的机器字数 */
/* Copy IPC MRs from one thread to another */
word_t copyMRs(tcb_t *sender, word_t *sendBuf, tcb_t *receiver,
               word_t *recvBuf, word_t n)
{
    word_t i;

    /* Copy inline words */
    for (i = 0; i < n && i < n_msgRegisters; i++) {
        setRegister(receiver, msgRegisters[i],
                    getRegister(sender, msgRegisters[i]));
    }

    if (!recvBuf || !sendBuf) {
        return i;
    }

    /* Copy out-of-line words */
    for (; i < n; i++) {
        recvBuf[i + 1] = sendBuf[i + 1];
    }

    return i;
}

#ifdef ENABLE_SMP_SUPPORT
/*Z 如果新加入调度队列的线程属于别的cpu，设置需要发出重调度IPI */
/* This checks if the current updated to scheduler queue is changing the previous scheduling
 * decision made by the scheduler. If its a case, an `irq_reschedule_ipi` is sent */
void remoteQueueUpdate(tcb_t *tcb)
{   /*Z 因为亲和cpu不同、且属于当前调度域的入队，需要特别处理，因为动了别人正用的调度队列 */
    /* only ipi if the target is for the current domain */
    if (tcb->tcbAffinity != getCurrentCPUIndex() && tcb->tcbDomain == ksCurDomain) {
        tcb_t *targetCurThread = NODE_STATE_ON_CORE(ksCurThread, tcb->tcbAffinity);/*Z 别人的当前线程 */
        /*Z 重调度的三种情况：目标的当前线程是idle线程(有事要做了)、优先级低(抢占)、计划重设定时器 */
        /* reschedule if the target core is idle or we are waking a higher priority thread (or
         * if a new irq would need to be set on MCS) */
        if (targetCurThread == NODE_STATE_ON_CORE(ksIdleThread, tcb->tcbAffinity)  ||
            tcb->tcbPriority > targetCurThread->tcbPriority
#ifdef CONFIG_KERNEL_MCS
            || NODE_STATE_ON_CORE(ksReprogram, tcb->tcbAffinity)
#endif
           ) {  /*Z 设置待发重调度IPI位图 */
            ARCH_NODE_STATE(ipiReschedulePending) |= BIT(tcb->tcbAffinity);
        }
    }
}
/*Z 停止线程在别的cpu上的当前运行，将其加入ready队列头 */
/* This makes sure the the TCB is not being run on other core.
 * It would request 'IpiRemoteCall_Stall' to switch the core from this TCB
 * We also request the 'irq_reschedule_ipi' to restore the state of target core */
void remoteTCBStall(tcb_t *tcb)
{

    if (
#ifdef CONFIG_KERNEL_MCS
        tcb->tcbSchedContext &&
#endif
        tcb->tcbAffinity != getCurrentCPUIndex() &&
        NODE_STATE_ON_CORE(ksCurThread, tcb->tcbAffinity) == tcb) {
        doRemoteStall(tcb->tcbAffinity);/*Z 发送停止当前线程IPI，对方cpu会切换至idle线程 */
        ARCH_NODE_STATE(ipiReschedulePending) |= BIT(tcb->tcbAffinity);/*Z 再准备发出重调度IPI以使目标cpu切换回正常程序 */
    }
}

#ifndef CONFIG_KERNEL_MCS
/*Z 设置线程新的亲和cpu，相应迁移ready、DEBUG队列、重调度等 */
static exception_t invokeTCB_SetAffinity(tcb_t *thread, word_t affinity)
{
    /* remove the tcb from scheduler queue in case it is already in one
     * and add it to new queue if required */
    tcbSchedDequeue(thread);
    migrateTCB(thread, affinity);/*Z 设置线程新的亲和cpu，迁移DEBUG队列 */
    if (isRunnable(thread)) {
        SCHED_APPEND(thread);
    }
    /* reschedule current cpu if tcb moves itself */
    if (thread == NODE_STATE(ksCurThread)) {
        rescheduleRequired();
    }
    return EXCEPTION_NONE;
}
/*Z 设置线程新的亲和cpu，相应迁移ready、DEBUG队列、重调度等 */
static exception_t decodeSetAffinity(cap_t cap, word_t length, word_t *buffer)
{
    tcb_t *tcb;
    word_t affinity;

    if (length < 1) {
        userError("TCB SetAffinity: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));

    affinity = getSyscallArg(0, buffer);/*Z ----------------------------消息传参：0-新的亲和cpu */
    if (affinity >= ksNumCPUs) {
        userError("TCB SetAffinity: Requested CPU does not exist.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_SetAffinity(tcb, affinity);/*Z 设置线程新的亲和cpu，相应迁移ready、DEBUG队列、重调度等 */
}
#endif
#endif /* ENABLE_SMP_SUPPORT */

#ifdef CONFIG_HARDWARE_DEBUG_API
/*Z 设置单步中断要跳过的指令数(n_instrs=0禁用)，使能(禁用)第bp_num个断点 */
static exception_t invokeConfigureSingleStepping(word_t *buffer, tcb_t *t,
                                                 uint16_t bp_num, word_t n_instrs)
{
    bool_t bp_was_consumed;
    /*Z 设置单步中断参数：n_instr要跳过的指令数，0-禁用单步中断 */
    bp_was_consumed = configureSingleStepping(t, bp_num, n_instrs, false);
    if (n_instrs == 0) {
        unsetBreakpointUsedFlag(t, bp_num);/*Z 禁用第bp_num个断点 */
        setMR(NODE_STATE(ksCurThread), buffer, 0, false);
    } else {
        setBreakpointUsedFlag(t, bp_num);/*Z 使能第bp_num个断点 */
        setMR(NODE_STATE(ksCurThread), buffer, 0, bp_was_consumed);
    }
    return EXCEPTION_NONE;
}
/*Z 设置单步中断要跳过的指令数(n_instrs=0禁用)，使能(禁用)第bp_num个断点 */
static exception_t decodeConfigureSingleStepping(cap_t cap, word_t *buffer)
{
    uint16_t bp_num;
    word_t n_instrs;
    tcb_t *tcb;
    syscall_error_t syserr;

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));

    bp_num = getSyscallArg(0, buffer);/*Z --------------------------------消息传参：0-要设置的断点序号 */
    n_instrs = getSyscallArg(1, buffer);                                        /*Z 1-要跳过的指令数 */
    /*Z x86什么也没做。返回无错误 */
    syserr = Arch_decodeConfigureSingleStepping(tcb, bp_num, n_instrs, false);
    if (syserr.type != seL4_NoError) {
        current_syscall_error = syserr;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeConfigureSingleStepping(buffer, tcb, bp_num, n_instrs);/*Z 设置单步中断要跳过的指令数(n_instrs=0禁用)，使能(禁用)第bp_num个断点 */
}
/*Z 设置TCB上下文中断点条件、数据区域大小、地址、使能参数 */
static exception_t invokeSetBreakpoint(tcb_t *tcb, uint16_t bp_num,
                                       word_t vaddr, word_t type, word_t size, word_t rw)
{   /*Z 设置TCB上下文中断点条件、数据区域大小、地址、使能参数 */
    setBreakpoint(tcb, bp_num, vaddr, type, size, rw);
    /* Signal restore_user_context() to pop the breakpoint context on return. */
    setBreakpointUsedFlag(tcb, bp_num);/*Z 使能第bp_num个断点(TCB中指示位图) */
    return EXCEPTION_NONE;
}
/*Z 设置TCB上下文中断点条件、数据区域大小、地址、使能参数 */
static exception_t decodeSetBreakpoint(cap_t cap, word_t *buffer)
{
    uint16_t bp_num;
    word_t vaddr, type, size, rw;
    tcb_t *tcb;
    syscall_error_t error;

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    bp_num = getSyscallArg(0, buffer);/*Z ----------------------------------------消息传参：0-断点序号 */
    vaddr = getSyscallArg(1, buffer);                                                   /*Z 1-断点地址 */
    type = getSyscallArg(2, buffer);                                                    /*Z 2-断点类型：0-数据断点，1-指令断点 */
    size = getSyscallArg(3, buffer);                                                    /*Z 3-断点数据尺寸：指令断点必须为0 */
    rw = getSyscallArg(4, buffer);                                                      /*Z 4-断点触发操作 */

    /* We disallow the user to set breakpoint addresses that are in the kernel
     * vaddr range.
     */
    if (vaddr >= (word_t)USER_TOP) {/*Z 验证断点地址不超限 */
        userError("Debug: Invalid address %lx: bp addresses must be userspace "
                  "addresses.",
                  vaddr);
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (type != seL4_InstructionBreakpoint && type != seL4_DataBreakpoint) {/*Z 验证断点类型 */
        userError("Debug: Unknown breakpoint type %lx.", type);
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 2;
        return EXCEPTION_SYSCALL_ERROR;
    } else if (type == seL4_InstructionBreakpoint) {
        if (size != 0) {/*Z 指令断点，尺寸必须为0 */
            userError("Debug: Instruction bps must have size of 0.");
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 3;
            return EXCEPTION_SYSCALL_ERROR;
        }
        if (rw != seL4_BreakOnRead) {
            userError("Debug: Instruction bps must be break-on-read.");
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 4;
            return EXCEPTION_SYSCALL_ERROR;
        }   /*Z 硬件断点有关检查，x86忽略 */
        if ((seL4_FirstWatchpoint == -1 || bp_num >= seL4_FirstWatchpoint)
            && seL4_FirstBreakpoint != seL4_FirstWatchpoint) {
            userError("Debug: Can't specify a watchpoint ID with type seL4_InstructionBreakpoint.");
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 2;
            return EXCEPTION_SYSCALL_ERROR;
        }
    } else if (type == seL4_DataBreakpoint) {
        if (size == 0) {/*Z 数据断点，尺寸不能为0 */
            userError("Debug: Data bps cannot have size of 0.");
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 3;
            return EXCEPTION_SYSCALL_ERROR;
        }
        if (seL4_FirstWatchpoint != -1 && bp_num < seL4_FirstWatchpoint) {
            userError("Debug: Data watchpoints cannot specify non-data watchpoint ID.");
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 2;
            return EXCEPTION_SYSCALL_ERROR;
        }
    } else if (type == seL4_SoftwareBreakRequest) {/*Z INT 3不用预置断点 */
        userError("Debug: Use a software breakpoint instruction to trigger a "
                  "software breakpoint.");
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 2;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (rw != seL4_BreakOnRead && rw != seL4_BreakOnWrite
        && rw != seL4_BreakOnReadWrite) {
        userError("Debug: Unknown access-type %lu.", rw);
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 3;
        return EXCEPTION_SYSCALL_ERROR;
    }
    if (size != 0 && size != 1 && size != 2 && size != 4 && size != 8) {
        userError("Debug: Invalid size %lu.", size);
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 3;
        return EXCEPTION_SYSCALL_ERROR;
    }
    if (size > 0 && vaddr & (size - 1)) {/*Z 断据断点有对齐要求 */
        /* Just Don't allow unaligned watchpoints. They are undefined
         * both ARM and x86.
         *
         * X86: Intel manuals, vol3, 17.2.5:
         *  "Two-byte ranges must be aligned on word boundaries; 4-byte
         *   ranges must be aligned on doubleword boundaries"
         *  "Unaligned data or I/O breakpoint addresses do not yield valid
         *   results"
         *
         * ARM: ARMv7 manual, C11.11.44:
         *  "A DBGWVR is programmed with a word-aligned address."
         */
        userError("Debug: Unaligned data watchpoint address %lx (size %lx) "
                  "rejected.\n",
                  vaddr, size);

        current_syscall_error.type = seL4_AlignmentError;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 检查断点序号、8字节尺寸是否支持 */
    error = Arch_decodeSetBreakpoint(tcb, bp_num, vaddr, type, size, rw);
    if (error.type != seL4_NoError) {
        current_syscall_error = error;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSetBreakpoint(tcb, bp_num,/*Z 设置TCB上下文中断点条件、数据区域大小、地址、使能参数 */
                               vaddr, type, size, rw);
}
/*Z 获取断点有关参数，写入消息寄存器(IPC buffer) */
static exception_t invokeGetBreakpoint(word_t *buffer, tcb_t *tcb, uint16_t bp_num)
{
    getBreakpoint_t res;
    /*Z 获取断点有关参数，写入消息寄存器(IPC buffer) */
    res = getBreakpoint(tcb, bp_num);
    setMR(NODE_STATE(ksCurThread), buffer, 0, res.vaddr);
    setMR(NODE_STATE(ksCurThread), buffer, 1, res.type);
    setMR(NODE_STATE(ksCurThread), buffer, 2, res.size);
    setMR(NODE_STATE(ksCurThread), buffer, 3, res.rw);
    setMR(NODE_STATE(ksCurThread), buffer, 4, res.is_enabled);
    return EXCEPTION_NONE;
}
/*Z 获取断点有关参数，写入消息寄存器(IPC buffer) */
static exception_t decodeGetBreakpoint(cap_t cap, word_t *buffer)
{
    tcb_t *tcb;
    uint16_t bp_num;
    syscall_error_t error;

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    bp_num = getSyscallArg(0, buffer);  /*Z --------------------------------消息传参：0-断点编号 */
    /*Z 检查断点编号是否超限 */
    error = Arch_decodeGetBreakpoint(tcb, bp_num);
    if (error.type != seL4_NoError) {
        current_syscall_error = error;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeGetBreakpoint(buffer, tcb, bp_num);/*Z 获取断点有关参数，写入消息寄存器(IPC buffer) */
}
/*Z 清除TCB上下文中相应的断点设置 */
static exception_t invokeUnsetBreakpoint(tcb_t *tcb, uint16_t bp_num)
{
    /* Maintain the bitfield of in-use breakpoints. */
    unsetBreakpoint(tcb, bp_num);/*Z 清除断点对应的TCB上下文中保存值 */
    unsetBreakpointUsedFlag(tcb, bp_num);/*Z 禁用第bp_num个断点(TCB中指示位图) */
    return EXCEPTION_NONE;
}
/*Z 清除TCB上下文中相应的断点设置 */
static exception_t decodeUnsetBreakpoint(cap_t cap, word_t *buffer)
{
    tcb_t *tcb;
    uint16_t bp_num;
    syscall_error_t error;

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    bp_num = getSyscallArg(0, buffer);/*Z ---------------------------消息传参：0-断点编号 */
    /*Z 检查断点编号是否超限 */
    error = Arch_decodeUnsetBreakpoint(tcb, bp_num);
    if (error.type != seL4_NoError) {
        current_syscall_error = error;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeUnsetBreakpoint(tcb, bp_num);/*Z 清除TCB上下文中相应的断点设置 */
}
#endif /* CONFIG_HARDWARE_DEBUG_API */
/*Z 设置TCB上下文中FS段寄存器，用于线程本地存储变量 */
static exception_t invokeSetTLSBase(tcb_t *thread, word_t tls_base)
{
    setRegister(thread, TLS_BASE, tls_base);
    if (thread == NODE_STATE(ksCurThread)) {
        /* If this is the current thread force a reschedule to ensure that any changes
         * to the TLS_BASE are realized */
        rescheduleRequired();
    }

    return EXCEPTION_NONE;
}
/*Z 设置TCB上下文中FS段寄存器，用于线程本地存储变量 */
static exception_t decodeSetTLSBase(cap_t cap, word_t length, word_t *buffer)
{
    word_t tls_base;

    if (length < 1) {
        userError("TCB SetTLSBase: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tls_base = getSyscallArg(0, buffer);/*Z ------------------------------消息传参：0-要设置的TLS基地址 */

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSetTLSBase(TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), tls_base);/*Z 设置TCB上下文中FS段寄存器，用于线程本地存储变量 */
}
/*Z 引用cap_thread_cap能力的主动类系统调用 */
/* The following functions sit in the syscall error monad, but include the
 * exception cases for the preemptible bottom end, as they call the invoke
 * functions directly.  This is a significant deviation from the Haskell
 * spec. */
exception_t decodeTCBInvocation(/*Z 系统调用传入的当前线程参数：*/
    word_t invLabel,        /*Z 消息标签(错误类型) */
    word_t length,          /*Z 消息长度 */
    cap_t cap,              /*Z 其能力 */
    cte_t *slot,            /*Z 其CSlot */
    extra_caps_t excaps,    /*Z 额外能力 */
    bool_t call,            /*Z 是否Call调用 */
    word_t *buffer          /*Z IPC buffer */
)
{   /*Z 停止线程在别的cpu上的运行，将其加入ready队列头 */
    /* Stall the core if we are operating on a remote TCB that is currently running */
    SMP_COND_STATEMENT(remoteTCBStall(TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)));)

    switch (invLabel) {
    case TCBReadRegisters:      /*Z ----------------------------------------子功能：Call类调用读对方TCB上下文寄存器(到当前线程消息寄存器(IPC buffer)) */
        /* Second level of decoding */
        return decodeReadRegisters(cap, length, call, buffer);

    case TCBWriteRegisters:     /*Z ---------------------------------------子功能：写对方TCB上下文寄存器 */
        return decodeWriteRegisters(cap, length, buffer);

    case TCBCopyRegisters:      /*Z ----------------------------------------子功能：拷贝对方TCB上下文寄存器，基本就是克隆线程 */
        return decodeCopyRegisters(cap, length, excaps, buffer);

    case TCBSuspend:            /*Z ----------------------------------------子功能：暂停线程 */
        /* Jump straight to the invoke */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeTCB_Suspend(
                   TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)));

    case TCBResume:             /*Z ----------------------------------------子功能：重启线程 */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeTCB_Resume(
                   TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)));

    case TCBConfigure:          /*Z ----------------------------------------子功能：配置错误处理句柄、CNode、VSpace、IPC buffer */
        return decodeTCBConfigure(cap, length, slot, excaps, buffer);

    case TCBSetPriority:        /*Z ----------------------------------------子功能：设置线程优先级 */
        return decodeSetPriority(cap, length, excaps, buffer);

    case TCBSetMCPriority:      /*Z ----------------------------------------子功能：设置线程最大可控优先级 */
        return decodeSetMCPriority(cap, length, excaps, buffer);

    case TCBSetSchedParams:     /*Z ----------------------------------------子功能：设置线程优先级和最大可控优先级 */
#ifdef CONFIG_KERNEL_MCS
        return decodeSetSchedParams(cap, length, slot, excaps, buffer);
#else
        return decodeSetSchedParams(cap, length, excaps, buffer);
#endif

    case TCBSetIPCBuffer:       /*Z ----------------------------------------子功能：设置IPC buffer(但能力映射标志可能未设置) */
        return decodeSetIPCBuffer(cap, length, slot, excaps, buffer);

    case TCBSetSpace:           /*Z ----------------------------------------子功能：设置错误处理句柄、CNode、VSpace */
        return decodeSetSpace(cap, length, slot, excaps, buffer);

    case TCBBindNotification:   /*Z ----------------------------------------子功能：绑定通知对象 */
        return decodeBindNotification(cap, excaps);

    case TCBUnbindNotification: /*Z ----------------------------------------子功能：解除绑定的通知对象 */
        return decodeUnbindNotification(cap);

#ifdef CONFIG_KERNEL_MCS
    case TCBSetTimeoutEndpoint: /*Z ----------------------------------------子功能：设置超时处理EP能力 */
        return decodeSetTimeoutEndpoint(cap, slot, excaps);
#else
#ifdef ENABLE_SMP_SUPPORT
    case TCBSetAffinity:        /*Z ----------------------------------------子功能：设置线程新的亲和cpu */
        return decodeSetAffinity(cap, length, buffer);
#endif /* ENABLE_SMP_SUPPORT */
#endif

        /* There is no notion of arch specific TCB invocations so this needs to go here */
#ifdef CONFIG_VTX
    case TCBSetEPTRoot:         /*Z ----------------------------------------子功能：设置EPT一级页表 */
        return decodeSetEPTRoot(cap, excaps);
#endif

#ifdef CONFIG_HARDWARE_DEBUG_API
    case TCBConfigureSingleStepping:/*Z ----------------------------------------子功能：设置单步中断要跳过的指令数，使能(禁用)断点 */
        return decodeConfigureSingleStepping(cap, buffer);

    case TCBSetBreakpoint:      /*Z ----------------------------------------子功能：设置断点条件、数据区域大小、地址、使能参数 */
        return decodeSetBreakpoint(cap, buffer);

    case TCBGetBreakpoint:      /*Z ----------------------------------------子功能： 获取断点有关参数 */
        return decodeGetBreakpoint(cap, buffer);

    case TCBUnsetBreakpoint:    /*Z ----------------------------------------子功能： 清除断点设置 */
        return decodeUnsetBreakpoint(cap, buffer);
#endif

    case TCBSetTLSBase:         /*Z ----------------------------------------子功能：设置TLS线程本地存储地址 */
        return decodeSetTLSBase(cap, length, buffer);

    default:
        /* Haskell: "throw IllegalOperation" */
        userError("TCB: Illegal operation.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
}

enum CopyRegistersFlags {
    CopyRegisters_suspendSource = 0,    /*Z 拷贝寄存器标志位：暂停对方线程 */
    CopyRegisters_resumeTarget = 1,     /*Z 拷贝寄存器标志位：继续本方线程 */
    CopyRegisters_transferFrame = 2,    /*Z 拷贝寄存器标志位：拷贝大多数寄存器 */
    CopyRegisters_transferInteger = 3   /*Z 拷贝寄存器标志位：拷贝FS、GS寄存器 */
};
/*Z 引用线程能力的系统调用：拷贝别的线程TCB上下文寄存器，基本就是克隆线程 */
exception_t decodeCopyRegisters(cap_t cap, word_t length,/*Z 引用的能力、消息长度 */
                                extra_caps_t excaps, word_t *buffer)/*Z extraCaps、当前线程IPC buffer */
{
    word_t transferArch;
    tcb_t *srcTCB;
    cap_t source_cap;
    word_t flags;

    if (length < 1 || excaps.excaprefs[0] == NULL) {
        userError("TCB CopyRegisters: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    flags = getSyscallArg(0, buffer);/*Z ---------------------------------消息传参：0-标志 */
    /*Z x86总是返回0 */
    transferArch = Arch_decodeTransfer(flags >> 8);

    source_cap = excaps.excaprefs[0]->cap;                                      /*Z extraCaps0-要拷贝的TCB访问能力 */

    if (cap_get_capType(source_cap) == cap_thread_cap) {
        srcTCB = TCB_PTR(cap_thread_cap_get_capTCBPtr(source_cap));
    } else {
        userError("TCB CopyRegisters: Invalid source TCB.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_CopyRegisters(/*Z 拷贝线程TCB上下文寄存器，基本就是克隆线程 */
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), srcTCB,
               flags & BIT(CopyRegisters_suspendSource),
               flags & BIT(CopyRegisters_resumeTarget),
               flags & BIT(CopyRegisters_transferFrame),
               flags & BIT(CopyRegisters_transferInteger),
               transferArch);

}

enum ReadRegistersFlags {
    ReadRegisters_suspend = 0/*Z 读别的线程寄存器系统调用的标志位：暂停对方线程 */
};
/*Z Call类调用读对方TCB上下文寄存器，写入当前线程消息寄存器(IPC buffer)；根据标志暂停对方 */
exception_t decodeReadRegisters(
    cap_t cap,              /*Z 引用的对方TCB访问能力 */
    word_t length,          /*Z 消息长度 */
    bool_t call,            /*Z 是否Call调用 */
    word_t *buffer          /*Z IPC buffer */
)
{
    word_t transferArch, flags, n;
    tcb_t *thread;

    if (length < 2) {
        userError("TCB ReadRegisters: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    flags = getSyscallArg(0, buffer);/*Z -------------------------------------消息传参：0-是否暂停对方 */
    n     = getSyscallArg(1, buffer);                                               /*Z 1-要读的寄存器数量 */

    if (n < 1 || n > n_frameRegisters + n_gpRegisters) {
        userError("TCB ReadRegisters: Attempted to read an invalid number of registers (%d).",
                  (int)n);
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = 1;
        current_syscall_error.rangeErrorMax = n_frameRegisters +
                                              n_gpRegisters;
        return EXCEPTION_SYSCALL_ERROR;
    }

    transferArch = Arch_decodeTransfer(flags >> 8);

    thread = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    if (thread == NODE_STATE(ksCurThread)) {/*Z 注意：这里引用的能力指向别的线程 */
        userError("TCB ReadRegisters: Attempted to read our own registers.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_ReadRegisters(/*Z Call类调用读对方TCB上下文寄存器，写入当前线程消息寄存器(IPC buffer)；根据标志暂停对方 */
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)),
               flags & BIT(ReadRegisters_suspend),/*Z 暂停对方线程的标志位 */
               n, transferArch, call);
}

enum WriteRegistersFlags {
    WriteRegisters_resume = 0/*Z 写别的线程寄存器系统调用的标志位：继续对方线程 */
};
/*Z 引用对方TCB访问能力的系统调用：写对方TCB上下文寄存器，根据标志重运行对方线程 */
exception_t decodeWriteRegisters(cap_t cap, word_t length, word_t *buffer)
{                           /*Z 引用的对方TCB访问能力、消息长度、当前线程IPC buffer */
    word_t flags, w;
    word_t transferArch;
    tcb_t *thread;

    if (length < 2) {
        userError("TCB WriteRegisters: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    flags = getSyscallArg(0, buffer);/*Z -----------------------------------------------------消息传参：0-是否继续对方的执行 */
    w     = getSyscallArg(1, buffer);                                                               /*Z 1-要写的寄存器数量 */
                                                                                                    /*Z 后面为该数量的要写的消息值 */
    if (length - 2 < w) {
        userError("TCB WriteRegisters: Message too short for requested write size (%d/%d).",
                  (int)(length - 2), (int)w);
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    transferArch = Arch_decodeTransfer(flags >> 8);

    thread = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    if (thread == NODE_STATE(ksCurThread)) {/*Z 系统调用不用于写自己 */
        userError("TCB WriteRegisters: Attempted to write our own registers.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_WriteRegisters(thread,/*Z 引用对方TCB访问能力的系统调用：写对方TCB上下文寄存器，根据标志重运行对方线程 */
                                    flags & BIT(WriteRegisters_resume),/*Z 继续执行标志 */
                                    w, transferArch, buffer);
}

#ifdef CONFIG_KERNEL_MCS
/*Z 错误处理能力是否有效：能发送或授权的EP能力可用于错误处理，空能力表示不进行错误处理，其它无效 */
static bool_t validFaultHandler(cap_t cap)
{
    switch (cap_get_capType(cap)) {
    case cap_endpoint_cap:
        if (!cap_endpoint_cap_get_capCanSend(cap) ||
            (!cap_endpoint_cap_get_capCanGrant(cap) &&
             !cap_endpoint_cap_get_capCanGrantReply(cap))) {
            current_syscall_error.type = seL4_InvalidCapability;
            return false;
        }
        break;
    case cap_null_cap:
        /* just has no fault endpoint */
        break;
    default:
        current_syscall_error.type = seL4_InvalidCapability;
        return false;
    }
    return true;
}
#endif
/*Z 引用对方(要配置的线程)cap_thread_cap能力的系统调用：配置错误处理句柄、CNode、VSpace、IPC buffer */
/* TCBConfigure batches SetIPCBuffer and parts of SetSpace. */
exception_t decodeTCBConfigure(cap_t cap, word_t length, cte_t *slot,/*Z 引用的能力、消息长度、其CSlot */
                               extra_caps_t rootCaps, word_t *buffer)/*Z 引用的额外能力、IPC buffer */
{
    cte_t *bufferSlot, *cRootSlot, *vRootSlot;
    cap_t bufferCap, cRootCap, vRootCap;
    deriveCap_ret_t dc_ret;
    word_t cRootData, vRootData, bufferAddr;
#ifdef CONFIG_KERNEL_MCS
#define TCBCONFIGURE_ARGS 3
#else
#define TCBCONFIGURE_ARGS 4
#endif
    if (length < TCBCONFIGURE_ARGS || rootCaps.excaprefs[0] == NULL
        || rootCaps.excaprefs[1] == NULL
        || rootCaps.excaprefs[2] == NULL) {
        userError("TCB Configure: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

#ifdef CONFIG_KERNEL_MCS
    cRootData     = getSyscallArg(0, buffer);
    vRootData     = getSyscallArg(1, buffer);
    bufferAddr    = getSyscallArg(2, buffer);
#else
    cptr_t faultEP       = getSyscallArg(0, buffer);/*Z ----------------------------------消息传参：0-要配置的错误处理句柄 */
    cRootData     = getSyscallArg(1, buffer);                                                   /*Z 1-要配置的CNode保护位及其位数：末6位指示位数，其它指示保护位 */
    vRootData     = getSyscallArg(2, buffer);                                                   /*Z 2-要配置的VSpace数据。x86忽略 */
    bufferAddr    = getSyscallArg(3, buffer);                                                   /*Z 3-要配置的IPC buffer虚拟地址 */
#endif

    cRootSlot  = rootCaps.excaprefs[0];                                                         /*Z extraCaps0-引用的源CNode */
    cRootCap   = rootCaps.excaprefs[0]->cap;
    vRootSlot  = rootCaps.excaprefs[1];                                                         /*Z extraCaps1-引用的源VSpace */
    vRootCap   = rootCaps.excaprefs[1]->cap;
    bufferSlot = rootCaps.excaprefs[2];                                                         /*Z extraCaps2-引用的源IPC buffer能力 */
    bufferCap  = rootCaps.excaprefs[2]->cap;
    /*Z 根据源和要配置的IPC buffer能力数据，准备新能力 */
    if (bufferAddr == 0) {
        bufferSlot = NULL;
    } else {
        dc_ret = deriveCap(bufferSlot, bufferCap);/*Z 从IPC buffer源能力导出新能力：尚未作映射 */
        if (dc_ret.status != EXCEPTION_NONE) {
            return dc_ret.status;
        }
        bufferCap = dc_ret.cap;
        /*Z 检查能力及虚拟地址是否适合作IPC buffer */
        exception_t e = checkValidIPCBuffer(bufferAddr, bufferCap);
        if (e != EXCEPTION_NONE) {
            return e;
        }
    }
    /*Z 这里是想验证引用者本身不是处于“已被废弃的空间”里 */
    if (slotCapLongRunningDelete(
            TCB_PTR_CTE_PTR(cap_thread_cap_get_capTCBPtr(cap), tcbCTable)) ||
        slotCapLongRunningDelete(/*Z 不好：此条永远为假 */
            TCB_PTR_CTE_PTR(cap_thread_cap_get_capTCBPtr(cap), tcbVTable))) {
        userError("TCB Configure: CSpace or VSpace currently being deleted.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 根据源和要配置的CNode能力数据，准备新能力 */
    if (cRootData != 0) {
        cRootCap = updateCapData(false, cRootData, cRootCap);
    }

    dc_ret = deriveCap(cRootSlot, cRootCap);
    if (dc_ret.status != EXCEPTION_NONE) {
        return dc_ret.status;
    }
    cRootCap = dc_ret.cap;

    if (cap_get_capType(cRootCap) != cap_cnode_cap) {
        userError("TCB Configure: CSpace cap is invalid.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 根据源和要配置的VSpace能力数据，准备新能力。x86忽略 */
    if (vRootData != 0) {
        vRootCap = updateCapData(false, vRootData, vRootCap);
    }

    dc_ret = deriveCap(vRootSlot, vRootCap);
    if (dc_ret.status != EXCEPTION_NONE) {
        return dc_ret.status;
    }
    vRootCap = dc_ret.cap;

    if (!isValidVTableRoot(vRootCap)) {
        userError("TCB Configure: VSpace cap is invalid.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlCaps(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               cRootCap, cRootSlot,
               vRootCap, vRootSlot,
               bufferAddr, bufferCap,
               bufferSlot, thread_control_caps_update_space |
               thread_control_caps_update_ipc_buffer);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               faultEP, NULL_PRIO, NULL_PRIO,
               cRootCap, cRootSlot,
               vRootCap, vRootSlot,
               bufferAddr, bufferCap,
               bufferSlot, thread_control_update_space |
               thread_control_update_ipc_buffer);
#endif
}
/*Z 引用目标cap_thread_cap能力的系统调用：设置其优先级 */
exception_t decodeSetPriority(cap_t cap, word_t length, extra_caps_t excaps, word_t *buffer)
{
    if (length < 1 || excaps.excaprefs[0] == NULL) {
        userError("TCB SetPriority: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    prio_t newPrio = getSyscallArg(0, buffer);/*Z ------------------------------------消息传参：0-要设置的优先级 */
    cap_t authCap = excaps.excaprefs[0]->cap;                                               /*Z extraCaps0-引用的授权线程能力 */

    if (cap_get_capType(authCap) != cap_thread_cap) {
        userError("Set priority: authority cap not a TCB.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcb_t *authTCB = TCB_PTR(cap_thread_cap_get_capTCBPtr(authCap));
    exception_t status = checkPrio(newPrio, authTCB);/*Z 检查优先级不超过线程最大可控优先级 */
    if (status != EXCEPTION_NONE) {
        userError("TCB SetPriority: Requested priority %lu too high (max %lu).",
                  (unsigned long) newPrio, (unsigned long) authTCB->tcbMCP);
        return status;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlSched(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), NULL,
               cap_null_cap_new(), NULL,
               NULL_PRIO, newPrio,
               NULL, thread_control_sched_update_priority);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), NULL,
               0, NULL_PRIO, newPrio,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               0, cap_null_cap_new(),
               NULL, thread_control_update_priority);/*Z 配置优先级 */
#endif
}
/*Z 引用目标cap_thread_cap能力的系统调用：设置其最大可控优先级 */
exception_t decodeSetMCPriority(cap_t cap, word_t length, extra_caps_t excaps, word_t *buffer)
{
    if (length < 1 || excaps.excaprefs[0] == NULL) {
        userError("TCB SetMCPriority: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    prio_t newMcp = getSyscallArg(0, buffer);/*Z -------------------------------------消息传参：0-要设置的最大可控优先级 */
    cap_t authCap = excaps.excaprefs[0]->cap;                                               /*Z extraCaps0-引用的授权线程能力 */

    if (cap_get_capType(authCap) != cap_thread_cap) {
        userError("TCB SetMCPriority: authority cap not a TCB.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcb_t *authTCB = TCB_PTR(cap_thread_cap_get_capTCBPtr(authCap));
    exception_t status = checkPrio(newMcp, authTCB);
    if (status != EXCEPTION_NONE) {
        userError("TCB SetMCPriority: Requested maximum controlled priority %lu too high (max %lu).",
                  (unsigned long) newMcp, (unsigned long) authTCB->tcbMCP);
        return status;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlSched(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), NULL,
               cap_null_cap_new(), NULL,
               newMcp, NULL_PRIO,
               NULL, thread_control_sched_update_mcp);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), NULL,
               0, newMcp, NULL_PRIO,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               0, cap_null_cap_new(),
               NULL, thread_control_update_mcp);/*Z 配置最大可控优先级 */
#endif
}

#ifdef CONFIG_KERNEL_MCS
/*Z 引用目标cap_thread_cap能力的系统调用：设置超时处理能力 */
exception_t decodeSetTimeoutEndpoint(cap_t cap, cte_t *slot, extra_caps_t excaps)
{
    if (excaps.excaprefs[0] == NULL) {
        userError("TCB SetSchedParams: Truncated message.");
        return EXCEPTION_SYSCALL_ERROR;
    }

    cte_t *thSlot = excaps.excaprefs[0];/*Z ----------------------------消息传参：extraCaps0-用于超时处理的EP能力 */
    cap_t thCap   = excaps.excaprefs[0]->cap;
    /*Z 错误处理能力是否有效 */
    /* timeout handler */
    if (!validFaultHandler(thCap)) {
        userError("TCB SetTimeoutEndpoint: timeout endpoint cap invalid.");
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_ThreadControlCaps(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               cap_null_cap_new(), NULL,
               thCap, thSlot,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               0, cap_null_cap_new(), NULL,
               thread_control_caps_update_timeout);
}
#endif

#ifdef CONFIG_KERNEL_MCS
exception_t decodeSetSchedParams(cap_t cap, word_t length, cte_t *slot, extra_caps_t excaps, word_t *buffer)
#else
/*Z 引用目标cap_thread_cap能力的系统调用：设置其优先级和最大可控优先级 */
exception_t decodeSetSchedParams(cap_t cap, word_t length, extra_caps_t excaps, word_t *buffer)
#endif
{
    if (length < 2 || excaps.excaprefs[0] == NULL
#ifdef CONFIG_KERNEL_MCS
        || excaps.excaprefs[1] == NULL || excaps.excaprefs[2] == NULL
#endif
       ) {
        userError("TCB SetSchedParams: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    prio_t newMcp = getSyscallArg(0, buffer);/*Z -------------------------------------消息传参：0-要设置的最大可控优先级 */
    prio_t newPrio = getSyscallArg(1, buffer);                                              /*Z 1-要设置的优先级 */
    cap_t authCap = excaps.excaprefs[0]->cap;                                               /*Z extraCaps0-授权线程 */
#ifdef CONFIG_KERNEL_MCS
    cap_t scCap   = excaps.excaprefs[1]->cap;                                               /*Z extraCaps1-要绑定的无主SC(仅MCS) */
    cte_t *fhSlot = excaps.excaprefs[2];                                                    /*Z extraCaps2-要设置的错误处理能力(仅MCS) */
    cap_t fhCap   = excaps.excaprefs[2]->cap;
#endif

    if (cap_get_capType(authCap) != cap_thread_cap) {
        userError("TCB SetSchedParams: authority cap not a TCB.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcb_t *authTCB = TCB_PTR(cap_thread_cap_get_capTCBPtr(authCap));
    exception_t status = checkPrio(newMcp, authTCB);
    if (status != EXCEPTION_NONE) {
        userError("TCB SetSchedParams: Requested maximum controlled priority %lu too high (max %lu).",
                  (unsigned long) newMcp, (unsigned long) authTCB->tcbMCP);
        return status;
    }

    status = checkPrio(newPrio, authTCB);
    if (status != EXCEPTION_NONE) {
        userError("TCB SetSchedParams: Requested priority %lu too high (max %lu).",
                  (unsigned long) newPrio, (unsigned long) authTCB->tcbMCP);
        return status;
    }

#ifdef CONFIG_KERNEL_MCS
    tcb_t *tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));
    sched_context_t *sc = NULL;
    switch (cap_get_capType(scCap)) {
    case cap_sched_context_cap:
        sc = SC_PTR(cap_sched_context_cap_get_capSCPtr(scCap));
        if (tcb->tcbSchedContext) {/*Z 验证目标线程还没有SC */
            userError("TCB Configure: tcb already has a scheduling context.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        if (sc->scTcb) {/*Z 验证SC还未绑定线程。由此：线程与SC是一对一关系 */
            userError("TCB Configure: sched contextext already bound.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        break;
    case cap_null_cap:
        if (tcb == NODE_STATE(ksCurThread)) {/*Z 不能解绑当前线程SC */
            userError("TCB SetSchedParams: Cannot change sched_context of current thread");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        break;
    default:
        userError("TCB Configure: sched context cap invalid.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 2;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (!validFaultHandler(fhCap)) {/*Z 验证错误处理能力有效性 */
        userError("TCB Configure: fault endpoint cap invalid.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 3;
        return EXCEPTION_SYSCALL_ERROR;
    }
#endif
    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlSched(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               fhCap, fhSlot,
               newMcp, newPrio,
               sc,
               thread_control_sched_update_mcp |
               thread_control_sched_update_priority |
               thread_control_sched_update_sc |
               thread_control_sched_update_fault);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), NULL,
               0, newMcp, newPrio,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               0, cap_null_cap_new(),
               NULL, thread_control_update_mcp |
               thread_control_update_priority);
#endif
}

/*Z 引用目标cap_thread_cap能力的系统调用：设置IPC buffer。但能力映射标志可能未设置 */
exception_t decodeSetIPCBuffer(cap_t cap, word_t length, cte_t *slot,
                               extra_caps_t excaps, word_t *buffer)
{
    cptr_t cptr_bufferPtr;
    cap_t bufferCap;
    cte_t *bufferSlot;

    if (length < 1 || excaps.excaprefs[0] == NULL) {
        userError("TCB SetIPCBuffer: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    cptr_bufferPtr  = getSyscallArg(0, buffer);/*Z ---------------------------消息传参：0-要设置的IPC buffer虚拟地址 */
    bufferSlot = excaps.excaprefs[0];                                               /*Z extraCaps0-要设置的IPC buffer访问能力 */
    bufferCap  = excaps.excaprefs[0]->cap;

    if (cptr_bufferPtr == 0) {
        bufferSlot = NULL;
    } else {/*Z 如果赋予新的虚拟地址，新IPC buffer能力将是未映射的，后续还需引用页能力进行映射系统调用 */
        exception_t e;
        deriveCap_ret_t dc_ret;
        /*Z 拷贝页访问能力，将映射类型置为未映射、ASID置为无 */
        dc_ret = deriveCap(bufferSlot, bufferCap);
        if (dc_ret.status != EXCEPTION_NONE) {
            return dc_ret.status;
        }/*Z 检查页能力及虚拟地址是否适合作IPC buffer */
        bufferCap = dc_ret.cap;
        e = checkValidIPCBuffer(cptr_bufferPtr, bufferCap);
        if (e != EXCEPTION_NONE) {
            return e;
        }
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlCaps(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               cptr_bufferPtr, bufferCap,
               bufferSlot, thread_control_caps_update_ipc_buffer);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               0, NULL_PRIO, NULL_PRIO,
               cap_null_cap_new(), NULL,
               cap_null_cap_new(), NULL,
               cptr_bufferPtr, bufferCap,
               bufferSlot, thread_control_update_ipc_buffer);/*Z 配置IPC buffer。但能力映射标志可能未设置 */

#endif
}

#ifdef CONFIG_KERNEL_MCS
#define DECODE_SET_SPACE_PARAMS 2
#else
#define DECODE_SET_SPACE_PARAMS 3
#endif
/*Z 引用目标cap_thread_cap能力的系统调用：设置错误处理句柄、CNode、VSpace */
exception_t decodeSetSpace(cap_t cap, word_t length, cte_t *slot,
                           extra_caps_t excaps, word_t *buffer)
{
    word_t cRootData, vRootData;
    cte_t *cRootSlot, *vRootSlot;
    cap_t cRootCap, vRootCap;
    deriveCap_ret_t dc_ret;

    if (length < DECODE_SET_SPACE_PARAMS || excaps.excaprefs[0] == NULL
        || excaps.excaprefs[1] == NULL
#ifdef CONFIG_KERNEL_MCS
        || excaps.excaprefs[2] == NULL
#endif
       ) {
        userError("TCB SetSpace: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

#ifdef CONFIG_KERNEL_MCS
    cRootData = getSyscallArg(0, buffer);
    vRootData = getSyscallArg(1, buffer);

    cte_t *fhSlot     = excaps.excaprefs[0];
    cap_t fhCap      = excaps.excaprefs[0]->cap;
    cRootSlot  = excaps.excaprefs[1];
    cRootCap   = excaps.excaprefs[1]->cap;
    vRootSlot  = excaps.excaprefs[2];
    vRootCap   = excaps.excaprefs[2]->cap;
#else
    cptr_t faultEP   = getSyscallArg(0, buffer);/*Z ----------------------------------消息传参：0-要设置的错误处理句柄 */
    cRootData = getSyscallArg(1, buffer);                                                   /*Z 1-要设置的CNode数据 */
    vRootData = getSyscallArg(2, buffer);                                                   /*Z 2-要设置的VSpace数据(x86忽略) */

    cRootSlot  = excaps.excaprefs[0];                                                       /*Z extraCaps0-目标CNode */
    cRootCap   = excaps.excaprefs[0]->cap;
    vRootSlot  = excaps.excaprefs[1];                                                       /*Z extraCaps1-目标VSpace */
    vRootCap   = excaps.excaprefs[1]->cap;
#endif
    /*Z 孤立的TCB、CNode、Zombie能力是已删除能力 */
    if (slotCapLongRunningDelete(
            TCB_PTR_CTE_PTR(cap_thread_cap_get_capTCBPtr(cap), tcbCTable)) ||
        slotCapLongRunningDelete(
            TCB_PTR_CTE_PTR(cap_thread_cap_get_capTCBPtr(cap), tcbVTable))) {
        userError("TCB SetSpace: CSpace or VSpace currently being deleted.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 更新CNode保护位及其位数 */
    if (cRootData != 0) {
        cRootCap = updateCapData(false, cRootData, cRootCap);
    }
    /*Z 无变化拷贝 */
    dc_ret = deriveCap(cRootSlot, cRootCap);
    if (dc_ret.status != EXCEPTION_NONE) {
        return dc_ret.status;
    }
    cRootCap = dc_ret.cap;

    if (cap_get_capType(cRootCap) != cap_cnode_cap) {
        userError("TCB SetSpace: Invalid CNode cap.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (vRootData != 0) {
        vRootCap = updateCapData(false, vRootData, vRootCap);
    }
    /*Z 无变化拷贝 */
    dc_ret = deriveCap(vRootSlot, vRootCap);
    if (dc_ret.status != EXCEPTION_NONE) {
        return dc_ret.status;
    }
    vRootCap = dc_ret.cap;

    if (!isValidVTableRoot(vRootCap)) {
        userError("TCB SetSpace: Invalid VSpace cap.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

#ifdef CONFIG_KERNEL_MCS
    /* fault handler */
    if (!validFaultHandler(fhCap)) {
        userError("TCB SetSpace: fault endpoint cap invalid.");
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }
#endif

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
#ifdef CONFIG_KERNEL_MCS
    return invokeTCB_ThreadControlCaps(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               fhCap, fhSlot,
               cap_null_cap_new(), NULL,
               cRootCap, cRootSlot,
               vRootCap, vRootSlot,
               0, cap_null_cap_new(), NULL, thread_control_caps_update_space | thread_control_caps_update_fault);
#else
    return invokeTCB_ThreadControl(
               TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)), slot,
               faultEP,
               NULL_PRIO, NULL_PRIO,
               cRootCap, cRootSlot,
               vRootCap, vRootSlot,
               0, cap_null_cap_new(), NULL, thread_control_update_space);/*Z 配置错误处理句柄、CNode、VSpace */
#endif
}
/*Z 引用cap_domain_cap的系统调用：设置线程的调度域 */
exception_t decodeDomainInvocation(word_t invLabel, word_t length, extra_caps_t excaps, word_t *buffer)
{
    word_t domain;
    cap_t tcap;

    if (unlikely(invLabel != DomainSetSet)) {/*Z -----------------------------一个功能 */
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (unlikely(length == 0)) {
        userError("Domain Configure: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    } else {
        domain = getSyscallArg(0, buffer);/*Z ----------------------------消息传参：0-要设置的调度域 */
        if (domain >= CONFIG_NUM_DOMAINS) {
            userError("Domain Configure: invalid domain (%lu >= %u).",
                      domain, CONFIG_NUM_DOMAINS);
            current_syscall_error.type = seL4_InvalidArgument;
            current_syscall_error.invalidArgumentNumber = 0;
            return EXCEPTION_SYSCALL_ERROR;
        }
    }

    if (unlikely(excaps.excaprefs[0] == NULL)) {
        userError("Domain Configure: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcap = excaps.excaprefs[0]->cap;                                            /*Z extraCaps-要设置的线程CSlot */
    if (unlikely(cap_get_capType(tcap) != cap_thread_cap)) {
        userError("Domain Configure: thread cap required.");
        current_syscall_error.type = seL4_InvalidArgument;
        current_syscall_error.invalidArgumentNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    setDomain(TCB_PTR(cap_thread_cap_get_capTCBPtr(tcap)), domain);/*Z 设置线程的调度域 */
    return EXCEPTION_NONE;
}
/*Z 引用cap_thread_cap的系统调用：建立线程与通知对象的绑定 */
exception_t decodeBindNotification(cap_t cap, extra_caps_t excaps)
{
    notification_t *ntfnPtr;
    tcb_t *tcb;
    cap_t ntfn_cap;

    if (excaps.excaprefs[0] == NULL) {
        userError("TCB BindNotification: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));

    if (tcb->tcbBoundNotification) {/*Z 线程只能绑定一个NF对象 */
        userError("TCB BindNotification: TCB already has a bound notification.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    ntfn_cap = excaps.excaprefs[0]->cap;/*Z ----------------------------------------------消息传参：extraCaps0-要绑定的NF能力 */

    if (cap_get_capType(ntfn_cap) == cap_notification_cap) {
        ntfnPtr = NTFN_PTR(cap_notification_cap_get_capNtfnPtr(ntfn_cap));
    } else {
        userError("TCB BindNotification: Notification is invalid.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (!cap_notification_cap_get_capNtfnCanReceive(ntfn_cap)) {/*Z 验证NF对象要能收信 */
        userError("TCB BindNotification: Insufficient access rights");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z NF对象与绑定线程是一对一关系：一个线程只能绑定一个NF对象，且NF队列要为空；一个NF对象只能绑定一个线程，且其它线程不能再使用该NF对象。绑定线程与NF队列不相容 */
    if ((tcb_t *)notification_ptr_get_ntfnQueue_head(ntfnPtr)
        || (tcb_t *)notification_ptr_get_ntfnBoundTCB(ntfnPtr)) {/*Z NF队列要为空且未绑定线程 */
        userError("TCB BindNotification: Notification cannot be bound.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }


    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_NotificationControl(tcb, ntfnPtr);/*Z 建立线程与通知对象的绑定 */
}
/*Z 引用cap_thread_cap的系统调用：解除线程与通知对象的绑定 */
exception_t decodeUnbindNotification(cap_t cap)
{
    tcb_t *tcb;

    tcb = TCB_PTR(cap_thread_cap_get_capTCBPtr(cap));

    if (!tcb->tcbBoundNotification) {
        userError("TCB UnbindNotification: TCB already has no bound Notification.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeTCB_NotificationControl(tcb, NULL);/*Z 解除线程的通知对象绑定 */
}
/*Z 暂停线程：取消IPC，置不活跃状态并摘出调度队列 */
/* The following functions sit in the preemption monad and implement the
 * preemptible, non-faulting bottom end of a TCB invocation. */
exception_t invokeTCB_Suspend(tcb_t *thread)
{
    suspend(thread);
    return EXCEPTION_NONE;
}
/*Z 重启动线程 */
exception_t invokeTCB_Resume(tcb_t *thread)
{
    restart(thread);
    return EXCEPTION_NONE;
}

#ifdef CONFIG_KERNEL_MCS
/*Z 为TCB CNode索引为index的CSlot安装新的能力，原能力删除 */
static inline exception_t installTCBCap(tcb_t *target, cap_t tCap, cte_t *slot,/*Z 线程及其能力和CSlot */
                                        tcb_cnode_index_t index, cap_t newCap, cte_t *srcSlot)/*Z TCB CNode索引、新能力及其源CSlot */
{
    cte_t *rootSlot = TCB_PTR_CTE_PTR(target, index);
    UNUSED exception_t e = cteDelete(rootSlot, true);
    if (e != EXCEPTION_NONE) {
        return e;
    }

    /* cteDelete on a cap installed in the tcb cannot fail */
    if (sameObjectAs(newCap, srcSlot->cap) &&
        sameObjectAs(tCap, slot->cap)) {
        cteInsert(newCap, srcSlot, rootSlot);
    }
    return e;
}
#endif

#ifdef CONFIG_KERNEL_MCS
exception_t invokeTCB_ThreadControlCaps(tcb_t *target, cte_t *slot,
                                        cap_t fh_newCap, cte_t *fh_srcSlot,
                                        cap_t th_newCap, cte_t *th_srcSlot,
                                        cap_t cRoot_newCap, cte_t *cRoot_srcSlot,
                                        cap_t vRoot_newCap, cte_t *vRoot_srcSlot,
                                        word_t bufferAddr, cap_t bufferCap,
                                        cte_t *bufferSrcSlot,
                                        thread_control_flag_t updateFlags)
{
    exception_t e;
    cap_t tCap = cap_thread_cap_new((word_t)target);

    if (updateFlags & thread_control_caps_update_fault) {
        e = installTCBCap(target, tCap, slot, tcbFaultHandler, fh_newCap, fh_srcSlot);
        if (e != EXCEPTION_NONE) {
            return e;
        }

    }

    if (updateFlags & thread_control_caps_update_timeout) {
        e = installTCBCap(target, tCap, slot, tcbTimeoutHandler, th_newCap, th_srcSlot);
        if (e != EXCEPTION_NONE) {
            return e;
        }
    }

    if (updateFlags & thread_control_caps_update_space) {
        e = installTCBCap(target, tCap, slot, tcbCTable, cRoot_newCap, cRoot_srcSlot);
        if (e != EXCEPTION_NONE) {
            return e;
        }

        e = installTCBCap(target, tCap, slot, tcbVTable, vRoot_newCap, vRoot_srcSlot);
        if (e != EXCEPTION_NONE) {
            return e;
        }
    }

    if (updateFlags & thread_control_caps_update_ipc_buffer) {
        cte_t *bufferSlot;

        bufferSlot = TCB_PTR_CTE_PTR(target, tcbBuffer);
        e = cteDelete(bufferSlot, true);
        if (e != EXCEPTION_NONE) {
            return e;
        }
        target->tcbIPCBuffer = bufferAddr;

        if (bufferSrcSlot && sameObjectAs(bufferCap, bufferSrcSlot->cap) &&
            sameObjectAs(tCap, slot->cap)) {
            cteInsert(bufferCap, bufferSrcSlot, bufferSlot);
        }

        if (target == NODE_STATE(ksCurThread)) {
            rescheduleRequired();
        }
    }

    return EXCEPTION_NONE;
}
#else
exception_t invokeTCB_ThreadControl(tcb_t *target, cte_t *slot,
                                    cptr_t faultep, prio_t mcp, prio_t priority,
                                    cap_t cRoot_newCap, cte_t *cRoot_srcSlot,
                                    cap_t vRoot_newCap, cte_t *vRoot_srcSlot,
                                    word_t bufferAddr, cap_t bufferCap,
                                    cte_t *bufferSrcSlot,
                                    thread_control_flag_t updateFlags)
{
    exception_t e;
    cap_t tCap = cap_thread_cap_new((word_t)target);

    if (updateFlags & thread_control_update_space) {
        target->tcbFaultHandler = faultep;
    }

    if (updateFlags & thread_control_update_mcp) {
        setMCPriority(target, mcp);
    }

    if (updateFlags & thread_control_update_space) {
        cte_t *rootSlot;
        /*Z 目标线程根CNode CSlot */
        rootSlot = TCB_PTR_CTE_PTR(target, tcbCTable);
        e = cteDelete(rootSlot, true);/*Z 清空原来的CNode */
        if (e != EXCEPTION_NONE) {
            return e;
        }/*Z 如果源CNode与新CNode指的都是同一个CNode，并且引用的就是对方TCB能力，则建立关联并赋予其新CNode能力 */
        if (sameObjectAs(cRoot_newCap, cRoot_srcSlot->cap) &&
            sameObjectAs(tCap, slot->cap)) {
            cteInsert(cRoot_newCap, cRoot_srcSlot, rootSlot);/*Z 注意：多个线程可共享同一CNode */
        }
    }
    /*Z 配置VSpace */
    if (updateFlags & thread_control_update_space) {
        cte_t *rootSlot;

        rootSlot = TCB_PTR_CTE_PTR(target, tcbVTable);
        e = cteDelete(rootSlot, true);
        if (e != EXCEPTION_NONE) {
            return e;
        }/*Z 指向同一个页表 */
        if (sameObjectAs(vRoot_newCap, vRoot_srcSlot->cap) &&
            sameObjectAs(tCap, slot->cap)) {
            cteInsert(vRoot_newCap, vRoot_srcSlot, rootSlot);/*Z 注意：多个线程可共享同一VSpace。猜测相当于linux同一进程的多个线程 */
        }
    }
    /*Z 配置IPC Buffer*/
    if (updateFlags & thread_control_update_ipc_buffer) {
        cte_t *bufferSlot;

        bufferSlot = TCB_PTR_CTE_PTR(target, tcbBuffer);
        e = cteDelete(bufferSlot, true);
        if (e != EXCEPTION_NONE) {
            return e;
        }
        target->tcbIPCBuffer = bufferAddr;

        if (bufferSrcSlot && sameObjectAs(bufferCap, bufferSrcSlot->cap) &&
            sameObjectAs(tCap, slot->cap)) {
            cteInsert(bufferCap, bufferSrcSlot, bufferSlot);/*Z 注意：多个线程可共享同一IPC buffer */
        }

        if (target == NODE_STATE(ksCurThread)) {
            rescheduleRequired();
        }
    }
    /*Z 配置优先级 */
    if (updateFlags & thread_control_update_priority) {
        setPriority(target, priority);
    }

    return EXCEPTION_NONE;
}
#endif

#ifdef CONFIG_KERNEL_MCS
exception_t invokeTCB_ThreadControlSched(tcb_t *target, cte_t *slot,
                                         cap_t fh_newCap, cte_t *fh_srcSlot,
                                         prio_t mcp, prio_t priority,
                                         sched_context_t *sc,
                                         thread_control_flag_t updateFlags)
{
    if (updateFlags & thread_control_sched_update_fault) {
        cap_t tCap = cap_thread_cap_new((word_t)target);
        exception_t e = installTCBCap(target, tCap, slot, tcbFaultHandler, fh_newCap, fh_srcSlot);
        if (e != EXCEPTION_NONE) {
            return e;
        }
    }

    if (updateFlags & thread_control_sched_update_mcp) {
        setMCPriority(target, mcp);
    }

    if (updateFlags & thread_control_sched_update_priority) {
        setPriority(target, priority);
    }

    if (updateFlags & thread_control_sched_update_sc) {
        if (sc != NULL && sc != target->tcbSchedContext) {
            schedContext_bindTCB(sc, target);
        } else if (sc == NULL && target->tcbSchedContext != NULL) {
            schedContext_unbindTCB(target->tcbSchedContext, target);
        }
    }

    return EXCEPTION_NONE;
}
#endif

exception_t invokeTCB_CopyRegisters(tcb_t *dest, tcb_t *tcb_src,
                                    bool_t suspendSource, bool_t resumeTarget,
                                    bool_t transferFrame, bool_t transferInteger,
                                    word_t transferArch)
{
    if (suspendSource) {
        suspend(tcb_src);
    }

    if (resumeTarget) {
        restart(dest);
    }

    if (transferFrame) {
        word_t i;
        word_t v;
        word_t pc;

        for (i = 0; i < n_frameRegisters; i++) {
            v = getRegister(tcb_src, frameRegisters[i]);
            setRegister(dest, frameRegisters[i], v);
        }

        pc = getRestartPC(dest);
        setNextPC(dest, pc);
    }

    if (transferInteger) {
        word_t i;
        word_t v;

        for (i = 0; i < n_gpRegisters; i++) {
            v = getRegister(tcb_src, gpRegisters[i]);
            setRegister(dest, gpRegisters[i], v);
        }
    }
    /*Z 如果不是当前线程，置TCB上下文错误码寄存器值0 */
    Arch_postModifyRegisters(dest);

    if (dest == NODE_STATE(ksCurThread)) {/*Z 不好：唯一的调用者已确认了此条件 */
        /* If we modified the current thread we may need to reschedule
         * due to changing registers are only reloaded in Arch_switchToThread */
        rescheduleRequired();
    }
            /*Z x86什么也没做 */
    return Arch_performTransfer(transferArch, tcb_src, dest);
}
/*Z Call类调用读对方TCB上下文寄存器，写入当前线程消息寄存器(IPC buffer)；根据标志暂停对方 */
/* ReadRegisters is a special case: replyFromKernel & setMRs are
 * unfolded here, in order to avoid passing the large reply message up
 * to the top level in a global (and double-copying). We prevent the
 * top-level replyFromKernel_success_empty() from running by setting the
 * thread state. Retype does this too.
 */
exception_t invokeTCB_ReadRegisters(tcb_t *tcb_src, bool_t suspendSource,/*Z 要被读的线程、是否暂停之 */
                                    word_t n, word_t arch, bool_t call)/*Z 要读的寄存器数量、x86恒0、是否Call类调用 */
{                   
    word_t i, j;
    exception_t e;
    tcb_t *thread;

    thread = NODE_STATE(ksCurThread);
    /*Z 暂停线程：取消IPC，置不活跃状态并摘出调度队列 */
    if (suspendSource) {
        suspend(tcb_src);
    }
    /*Z x86什么也没做 */
    e = Arch_performTransfer(arch, tcb_src, NODE_STATE(ksCurThread));
    if (e != EXCEPTION_NONE) {
        return e;
    }

    if (call) {
        word_t *ipcBuffer;

        ipcBuffer = lookupIPCBuffer(true, thread);

        setRegister(thread, badgeRegister, 0);

        for (i = 0; i < n && i < n_frameRegisters && i < n_msgRegisters; i++) {
            setRegister(thread, msgRegisters[i],
                        getRegister(tcb_src, frameRegisters[i]));
        }

        if (ipcBuffer != NULL && i < n && i < n_frameRegisters) {
            for (; i < n && i < n_frameRegisters; i++) {
                ipcBuffer[i + 1] = getRegister(tcb_src, frameRegisters[i]);
            }
        }

        j = i;

        for (i = 0; i < n_gpRegisters && i + n_frameRegisters < n
             && i + n_frameRegisters < n_msgRegisters; i++) {/*Z 不好：基本没用的代码 */
            setRegister(thread, msgRegisters[i + n_frameRegisters],
                        getRegister(tcb_src, gpRegisters[i]));
        }

        if (ipcBuffer != NULL && i < n_gpRegisters
            && i + n_frameRegisters < n) {
            for (; i < n_gpRegisters && i + n_frameRegisters < n; i++) {
                ipcBuffer[i + n_frameRegisters + 1] =
                    getRegister(tcb_src, gpRegisters[i]);
            }
        }
        /*Z 读取结果写到当前线程指示消息寄存器中 */
        setRegister(thread, msgInfoRegister, wordFromMessageInfo(
                        seL4_MessageInfo_new(0, 0, 0, i + j)));
    }
    setThreadState(thread, ThreadState_Running);

    return EXCEPTION_NONE;
}
/*Z 引用对方TCB访问能力的系统调用：写对方TCB上下文寄存器，根据标志重运行对方线程 */
exception_t invokeTCB_WriteRegisters(tcb_t *dest, bool_t resumeTarget,/*Z 对方线程、继续对方执行标志 */
                                     word_t n, word_t arch, word_t *buffer)/*Z 写的数量、x86恒0、我方IPC buffer */
{
    word_t i;
    word_t pc;
    exception_t e;
    bool_t archInfo;
    /*Z x86什么也没做 */
    e = Arch_performTransfer(arch, NODE_STATE(ksCurThread), dest);
    if (e != EXCEPTION_NONE) {
        return e;
    }

    if (n > n_frameRegisters + n_gpRegisters) {
        n = n_frameRegisters + n_gpRegisters;
    }
    /*Z x86恒0 */
    archInfo = Arch_getSanitiseRegisterInfo(dest);

    for (i = 0; i < n_frameRegisters && i < n; i++) {
        /* Offset of 2 to get past the initial syscall arguments */
        setRegister(dest, frameRegisters[i],
                    sanitiseRegister(frameRegisters[i],
                                     getSyscallArg(i + 2, buffer), archInfo));
    }

    for (i = 0; i < n_gpRegisters && i + n_frameRegisters < n; i++) {
        setRegister(dest, gpRegisters[i],
                    sanitiseRegister(gpRegisters[i],
                                     getSyscallArg(i + n_frameRegisters + 2,
                                                   buffer), archInfo));
    }
    /*Z 返回TCB上下文中记录的发生错误时的指令地址 */
    pc = getRestartPC(dest);
    setNextPC(dest, pc);/*Z 在TCB上下文中保存下条指令地址 */
    /*Z 如果不是当前线程，置TCB上下文错误码寄存器值0 */
    Arch_postModifyRegisters(dest);

    if (resumeTarget) {
        restart(dest);
    }
    /*Z 不好：唯一的调用者已排除了这种可能 */
    if (dest == NODE_STATE(ksCurThread)) {
        /* If we modified the current thread we may need to reschedule
         * due to changing registers are only reloaded in Arch_switchToThread */
        rescheduleRequired();
    }

    return EXCEPTION_NONE;
}
/*Z 通知对象不为空则建立线程与通知对象的绑定，否则解除线程的通知对象绑定 */
exception_t invokeTCB_NotificationControl(tcb_t *tcb, notification_t *ntfnPtr)
{
    if (ntfnPtr) {
        bindNotification(tcb, ntfnPtr);/*Z 线程与通知对象建立一对一的绑定关系 */
    } else {
        unbindNotification(tcb);/*Z 解除可能的线程与通知对象的绑定 */
    }

    return EXCEPTION_NONE;
}

#ifdef CONFIG_DEBUG_BUILD
void setThreadName(tcb_t *tcb, const char *name)/*Z 设置线程名 */
{
    strlcpy(tcb->tcbName, name, TCB_NAME_LENGTH);
}
#endif
/*Z 将当前系统调用错误信息写到线程消息寄存器(IPC buffer) */
word_t setMRs_syscall_error(tcb_t *thread, word_t *receiveIPCBuffer)
{
    switch (current_syscall_error.type) {
    case seL4_InvalidArgument:
        return setMR(thread, receiveIPCBuffer, 0,
                     current_syscall_error.invalidArgumentNumber);

    case seL4_InvalidCapability:
        return setMR(thread, receiveIPCBuffer, 0,
                     current_syscall_error.invalidCapNumber);

    case seL4_IllegalOperation:
        return 0;

    case seL4_RangeError:
        setMR(thread, receiveIPCBuffer, 0,
              current_syscall_error.rangeErrorMin);
        return setMR(thread, receiveIPCBuffer, 1,
                     current_syscall_error.rangeErrorMax);

    case seL4_AlignmentError:
        return 0;

    case seL4_FailedLookup:
        setMR(thread, receiveIPCBuffer, 0,
              current_syscall_error.failedLookupWasSource ? 1 : 0);
        return setMRs_lookup_failure(thread, receiveIPCBuffer,
                                     current_lookup_fault, 1);

    case seL4_TruncatedMessage:
    case seL4_DeleteFirst:
    case seL4_RevokeFirst:
        return 0;
    case seL4_NotEnoughMemory:
        return setMR(thread, receiveIPCBuffer, 0,
                     current_syscall_error.memoryLeft);
    default:
        fail("Invalid syscall error");
    }
}
