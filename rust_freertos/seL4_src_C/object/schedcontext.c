/*
 * Copyright 2020, Data61, CSIRO (ABN 41 687 119 230)
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <machine/timer.h>
#include <object/schedcontext.h>

static exception_t invokeSchedContext_UnbindObject(sched_context_t *sc, cap_t cap)
{
    switch (cap_get_capType(cap)) {
    case cap_thread_cap:
        schedContext_unbindTCB(sc, sc->scTcb);
        break;
    case cap_notification_cap:
        schedContext_unbindNtfn(sc);
        break;
    default:
        fail("invalid cap type");
    }

    return EXCEPTION_NONE;
}

static exception_t decodeSchedContext_UnbindObject(sched_context_t *sc, extra_caps_t extraCaps)
{
    if (extraCaps.excaprefs[0] == NULL) {
        userError("SchedContext_Unbind: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    cap_t cap = extraCaps.excaprefs[0]->cap;
    switch (cap_get_capType(cap)) {
    case cap_thread_cap:
        if (sc->scTcb != TCB_PTR(cap_thread_cap_get_capTCBPtr(cap))) {
            userError("SchedContext UnbindObject: object not bound");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        if (sc->scTcb == NODE_STATE(ksCurThread)) {
            userError("SchedContext UnbindObject: cannot unbind sc of current thread");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        break;
    case cap_notification_cap:
        if (sc->scNotification != NTFN_PTR(cap_notification_cap_get_capNtfnPtr(cap))) {
            userError("SchedContext UnbindObject: object not bound");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        break;

    default:
        userError("SchedContext_Unbind: invalid cap");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;

    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSchedContext_UnbindObject(sc, cap);
}

static exception_t invokeSchedContext_Bind(sched_context_t *sc, cap_t cap)
{
    switch (cap_get_capType(cap)) {
    case cap_thread_cap:
        schedContext_bindTCB(sc, TCB_PTR(cap_thread_cap_get_capTCBPtr(cap)));
        break;
    case cap_notification_cap:
        schedContext_bindNtfn(sc, NTFN_PTR(cap_notification_cap_get_capNtfnPtr(cap)));
        break;
    default:
        fail("invalid cap type");
    }

    return EXCEPTION_NONE;
}

static exception_t decodeSchedContext_Bind(sched_context_t *sc, extra_caps_t extraCaps)
{
    if (extraCaps.excaprefs[0] == NULL) {
        userError("SchedContext_Bind: Truncated Message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    cap_t cap = extraCaps.excaprefs[0]->cap;

    if (sc->scTcb != NULL || sc->scNotification != NULL) {
        userError("SchedContext_Bind: sched context already bound.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    switch (cap_get_capType(cap)) {
    case cap_thread_cap:
        if (TCB_PTR(cap_thread_cap_get_capTCBPtr(cap))->tcbSchedContext != NULL) {
            userError("SchedContext_Bind: tcb already bound.");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }

        break;
    case cap_notification_cap:
        if (notification_ptr_get_ntfnSchedContext(NTFN_PTR(cap_notification_cap_get_capNtfnPtr(cap)))) {
            userError("SchedContext_Bind: notification already bound");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        break;
    default:
        userError("SchedContext_Bind: invalid cap.");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSchedContext_Bind(sc, cap);
}

static exception_t invokeSchedContext_Unbind(sched_context_t *sc)
{
    schedContext_unbindAllTCBs(sc);
    schedContext_unbindNtfn(sc);
    if (sc->scReply) {
        sc->scReply->replyNext = call_stack_new(0, false);
        sc->scReply = NULL;
    }
    return EXCEPTION_NONE;
}

#ifdef ENABLE_SMP_SUPPORT
static inline void maybeStallSC(sched_context_t *sc)
{
    if (sc->scTcb) {
        remoteTCBStall(sc->scTcb);
    }
}
#endif
/*Z 清零调度上下文的累计已消费微秒数，并将其写入ksCurThread的消息寄存器，以及消息指示寄存器 */
static inline void setConsumed(sched_context_t *sc, word_t *buffer)
{
    time_t consumed = schedContext_updateConsumed(sc);/*Z 清零调度上下文的累计已消费，并返回其微秒值 */
    word_t length = mode_setTimeArg(0, consumed, buffer, NODE_STATE(ksCurThread));/*Z 写入ksCurThread的消息寄存器 */
    setRegister(NODE_STATE(ksCurThread), msgInfoRegister, wordFromMessageInfo(seL4_MessageInfo_new(0, 0, 0, length)));
}

static exception_t invokeSchedContext_Consumed(sched_context_t *sc, word_t *buffer)
{
    setConsumed(sc, buffer);
    return EXCEPTION_NONE;
}
/*Z 出让当前线程的执行权(注意不是出让预算)，如果受让线程不可调度或无可用预算，什么也没发生；
如果有预算可调度且在本核上同优先级，则受让者优先调度，否则ready队列头等待调度 */
static exception_t invokeSchedContext_YieldTo(sched_context_t *sc, word_t *buffer)
{   /*Z 清零原来受让的累计已消费时间，并将其写入ksCurThread的消息寄存器，出让者和受让者清除yield标识 */
    if (sc->scYieldFrom) {
        schedContext_completeYieldTo(sc->scYieldFrom);
        assert(sc->scYieldFrom == NULL);
    }
    /*Z 如果线程可调度但预算不可用或不足，则将线程加入release队列 */
    /* if the tcb is in the scheduler, it's ready and sufficient.
     * Otherwise, check that it is ready and sufficient and if not,
     * place the thread in the release queue. This way, from this point,
     * if the thread isSchedulable, it is ready and sufficient.*/
    schedContext_resume(sc);

    bool_t return_now = true;
    if (isSchedulable(sc->scTcb)) {/*Z 经过上步schedContext_resume后，可调度的一定有可用预算 */
        refill_unblock_check(sc);/*Z 规范refill的动作，无实质东西 */
        if (SMP_COND_STATEMENT(sc->scCore != getCurrentCPUIndex() ||)
            sc->scTcb->tcbPriority < NODE_STATE(ksCurThread)->tcbPriority) {
            tcbSchedDequeue(sc->scTcb);
            SCHED_ENQUEUE(sc->scTcb);/*Z 受让线程不在本核上，或比出让者优先级低，则入ready队列头 */
        } else {
            NODE_STATE(ksCurThread)->tcbYieldTo = sc;
            sc->scYieldFrom = NODE_STATE(ksCurThread);/*Z 建立受让关系 */
            tcbSchedDequeue(sc->scTcb);
            tcbSchedEnqueue(NODE_STATE(ksCurThread));
            tcbSchedEnqueue(sc->scTcb);/*Z 一个核上且优先级相同，受让线程优先调度 */
            rescheduleRequired();
            /*Z 受让成功时，当前线程不再运行，因此将受让者累计消费时间推迟到当前线程再次调度时计算发送 */
            /* we are scheduling the thread associated with sc,
             * so we don't need to write to the ipc buffer
             * until the caller is scheduled again */
            return_now = false;
        }
    }

    if (return_now) {
        setConsumed(sc, buffer);/*Z BUG: schedContext_completeYieldTo也调用了这个函数 1:1的关系没问题因为activateThread，但n:1??? */
    }

    return EXCEPTION_NONE;
}

static exception_t decodeSchedContext_YieldTo(sched_context_t *sc, word_t *buffer)
{   /*Z 受让SC不能绑定的是当前线程 */
    if (sc->scTcb == NODE_STATE(ksCurThread)) {
        userError("SchedContext_YieldTo: cannot seL4_SchedContext_YieldTo on self");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 受让SC不能无绑定线程 */
    if (sc->scTcb == NULL) {
        userError("SchedContext_YieldTo: cannot yield to an inactive sched context");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
    /*Z 受让SC的绑定线程的优先级不能超过当前线程的MCP */
    if (sc->scTcb->tcbPriority > NODE_STATE(ksCurThread)->tcbMCP) {
        userError("SchedContext_YieldTo: insufficient mcp (%lu) to yield to a thread with prio (%lu)",
                  (unsigned long) NODE_STATE(ksCurThread)->tcbMCP, (unsigned long) sc->scTcb->tcbPriority);
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSchedContext_YieldTo(sc, buffer);
}

exception_t decodeSchedContextInvocation(word_t label, cap_t cap, extra_caps_t extraCaps, word_t *buffer)
{
    sched_context_t *sc = SC_PTR(cap_sched_context_cap_get_capSCPtr(cap));

    SMP_COND_STATEMENT((maybeStallSC(sc));)

    switch (label) {
    case SchedContextConsumed:
        /* no decode */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeSchedContext_Consumed(sc, buffer);
    case SchedContextBind:
        return decodeSchedContext_Bind(sc, extraCaps);
    case SchedContextUnbindObject:
        return decodeSchedContext_UnbindObject(sc, extraCaps);
    case SchedContextUnbind:
        /* no decode */
        if (sc->scTcb == NODE_STATE(ksCurThread)) {
            userError("SchedContext UnbindObject: cannot unbind sc of current thread");
            current_syscall_error.type = seL4_IllegalOperation;
            return EXCEPTION_SYSCALL_ERROR;
        }
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeSchedContext_Unbind(sc);
    case SchedContextYieldTo:
        return decodeSchedContext_YieldTo(sc, buffer);
    default:
        userError("SchedContext invocation: Illegal operation attempted.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
}
/*Z 如果线程可调度但预算不可用或不足，则将线程加入release队列 */
void schedContext_resume(sched_context_t *sc)
{
    assert(!sc || sc->scTcb != NULL);
    if (likely(sc) && isSchedulable(sc->scTcb)) {/*Z 可调度但预算启用时间可能未到 */
        if (!(refill_ready(sc) && refill_sufficient(sc, 0))) {
            assert(!thread_state_get_tcbQueued(sc->scTcb->tcbState));
            postpone(sc);
        }
    }
}
/*Z 线程、SC建立一对一绑定 */
void schedContext_bindTCB(sched_context_t *sc, tcb_t *tcb)
{
    assert(sc->scTcb == NULL);
    assert(tcb->tcbSchedContext == NULL);

    tcb->tcbSchedContext = sc;
    sc->scTcb = tcb;

    SMP_COND_STATEMENT(migrateTCB(tcb, sc->scCore));

    schedContext_resume(sc);
    if (isSchedulable(tcb)) {
        SCHED_ENQUEUE(tcb);
        rescheduleRequired();
        // TODO -- at some stage we should take this call out of any TCB invocations that
        // alter capabilities, so that we can do a direct switch. The prefernce here is to
        // remove seL4_SetSchedParams from using ThreadControl. It's currently out of scope for
        // verification work, so the work around is to use rescheduleRequired()
        //possibleSwitchTo(tcb);
    }
}
/*Z 解除线程、SC绑定，将线程移出可能的ready/release队列 */
void schedContext_unbindTCB(sched_context_t *sc, tcb_t *tcb)
{
    assert(sc->scTcb == tcb);

    /* tcb must already be stalled at this point */
    if (tcb == NODE_STATE(ksCurThread)) {
        rescheduleRequired();
    }

    tcbSchedDequeue(sc->scTcb);
    tcbReleaseRemove(sc->scTcb);

    sc->scTcb->tcbSchedContext = NULL;
    sc->scTcb = NULL;
}

void schedContext_unbindAllTCBs(sched_context_t *sc)
{
    if (sc->scTcb) {
        SMP_COND_STATEMENT(remoteTCBStall(sc->scTcb));
        schedContext_unbindTCB(sc, sc->scTcb);
    }
}

void schedContext_donate(sched_context_t *sc, tcb_t *to)
{
    assert(sc != NULL);
    assert(to != NULL);
    assert(to->tcbSchedContext == NULL);

    tcb_t *from = sc->scTcb;
    if (from) {
        SMP_COND_STATEMENT(remoteTCBStall(from));
        tcbSchedDequeue(from);
        from->tcbSchedContext = NULL;
        if (from == NODE_STATE(ksCurThread) || from == NODE_STATE(ksSchedulerAction)) {
            rescheduleRequired();
        }
    }
    sc->scTcb = to;
    to->tcbSchedContext = sc;

    SMP_COND_STATEMENT(migrateTCB(to, sc->scCore));
}

void schedContext_bindNtfn(sched_context_t *sc, notification_t *ntfn)
{
    notification_ptr_set_ntfnSchedContext(ntfn, SC_REF(sc));
    sc->scNotification = ntfn;
}

void schedContext_unbindNtfn(sched_context_t *sc)
{
    if (sc && sc->scNotification) {
        notification_ptr_set_ntfnSchedContext(sc->scNotification, SC_REF(0));
        sc->scNotification = NULL;
    }
}
/*Z 清零调度上下文的累计已消费，并返回其微秒值 */
time_t schedContext_updateConsumed(sched_context_t *sc)
{
    ticks_t consumed = sc->scConsumed;
    if (consumed >= getMaxTicksToUs()) {
        sc->scConsumed -= getMaxTicksToUs();
        return getMaxTicksToUs();
    } else {
        sc->scConsumed = 0;
        return ticksToUs(consumed);
    }
}
/*Z 出让者和受让者清除yield标识 */
void schedContext_cancelYieldTo(tcb_t *tcb)
{
    if (tcb && tcb->tcbYieldTo) {
        tcb->tcbYieldTo->scYieldFrom = NULL;
        tcb->tcbYieldTo = NULL;
    }
}
/*Z 清零指定线程的受让者SC累计已消费时间，并将其写入ksCurThread的消息寄存器，出让者和受让者清除yield标识 */
void schedContext_completeYieldTo(tcb_t *yielder)
{
    if (yielder && yielder->tcbYieldTo) {        /*Z yielder's buffer */
        setConsumed(yielder->tcbYieldTo, lookupIPCBuffer(true, yielder));/*Z 清零SC的累计已消费微秒数，并将其写入ksCurThread的消息寄存器 */
        schedContext_cancelYieldTo(yielder);/*Z 出让者和受让者清除yield标识 */
    }
}
