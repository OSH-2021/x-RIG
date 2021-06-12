/*Z
按需调度器：
    ready可运行队列：每核、每域、每优先级一个
    待充值队列：每核一个
*/
/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <config.h>
#include <object.h>
#include <util.h>
#include <api/faults.h>
#include <api/types.h>
#include <kernel/cspace.h>
#include <kernel/thread.h>
#include <kernel/vspace.h>
#ifdef CONFIG_KERNEL_MCS
#include <object/schedcontext.h>
#endif
#include <model/statedata.h>
#include <arch/machine.h>
#include <arch/kernel/thread.h>
#include <machine/registerset.h>
#include <linker.h>
/*Z 根据指示信息，将授予的能力(extraCaps)或其标记(badge)拷贝给接收者，返回更新后的消息指示字。注意：拷贝的能力只能有一个 */
static seL4_MessageInfo_t
transferCaps(seL4_MessageInfo_t info, extra_caps_t caps,
             endpoint_t *endpoint, tcb_t *receiver,
             word_t *receiveBuffer);
/*Z 设置idle线程预先保存的上下文寄存器和线程状态 */
BOOT_CODE void configureIdleThread(tcb_t *tcb)
{   /*Z 在TCB中保存上下文相关寄存器的指定值 */
    Arch_configureIdleThread(tcb);
    setThreadState(tcb, ThreadState_IdleThreadState);
}
/*Z 对ksCurThread，处理出让、重运行问题 */
void activateThread(void)
{
#ifdef CONFIG_KERNEL_MCS
    if (unlikely(NODE_STATE(ksCurThread)->tcbYieldTo)) {/*Z 如果出让 */
        schedContext_completeYieldTo(NODE_STATE(ksCurThread));/*Z 清零受让者SC的累计消费时间，并将其写入ksCurThread的消息寄存器，清除双方yield标识 */
        assert(thread_state_get_tsType(NODE_STATE(ksCurThread)->tcbState) == ThreadState_Running);
    }
#endif

    switch (thread_state_get_tsType(NODE_STATE(ksCurThread)->tcbState)) {
    case ThreadState_Running:
#ifdef CONFIG_VTX
    case ThreadState_RunningVM:
#endif
        break;

    case ThreadState_Restart: {
        word_t pc;
        
        pc = getRestartPC(NODE_STATE(ksCurThread));/*Z 返回TCB上下文中记录的发生错误时的指令地址 */
        setNextPC(NODE_STATE(ksCurThread), pc);/*Z 在TCB上下文中保存下条指令地址 */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Running);
        break;
    }

    case ThreadState_IdleThreadState:
        Arch_activateIdleThread(NODE_STATE(ksCurThread));
        break;

    default:/*Z 4个block状态以及Inactive */
        fail("Current thread is blocked");
    }
}
/*Z 暂停线程：取消IPC，置不活跃状态并摘出调度队列 */
void suspend(tcb_t *target)
{   /*Z 取消线程的IPC状态：置空要发送的错误、从EP(NF)队列中摘除、删除与reply对象的关联、置状态为不活跃 */
    cancelIPC(target);
    if (thread_state_get_tsType(target->tcbState) == ThreadState_Running) {
        /* whilst in the running state it is possible that restart pc of a thread is
         * incorrect. As we do not know what state this thread will transition to
         * after we make it inactive we update its restart pc so that the thread next
         * runs at the correct address whether it is restarted or moved directly to
         * running *//*Z 置TCB上下文中FaultIP的值为NextIP的值 */
        updateRestartPC(target);
    }
    setThreadState(target, ThreadState_Inactive);
    tcbSchedDequeue(target);
#ifdef CONFIG_KERNEL_MCS
    tcbReleaseRemove(target);
    schedContext_cancelYieldTo(target);
#endif
}
/*Z 重启动线程 */
void restart(tcb_t *target)
{
    if (isStopped(target)) {
        cancelIPC(target);
#ifdef CONFIG_KERNEL_MCS
        setThreadState(target, ThreadState_Restart);
        schedContext_resume(target->tcbSchedContext);
        if (isSchedulable(target)) {
            possibleSwitchTo(target);
        }
#else   /*Z 如果线程未设置回复能力，则设置为主叫、允许授权回复、可撤销、首个标记 */
        setupReplyMaster(target);/*Z 目标作为回复的要求方是为什么??????? */
        setThreadState(target, ThreadState_Restart);
        SCHED_ENQUEUE(target);
        possibleSwitchTo(target);
#endif
    }
}
/*Z 将sender待发送的错误消息(优先)或正常消息，及可选的授予能力copy给接收者 */
void doIPCTransfer(tcb_t *sender, endpoint_t *endpoint, word_t badge,
                   bool_t grant, tcb_t *receiver)
{
    void *receiveBuffer, *sendBuffer;

    receiveBuffer = lookupIPCBuffer(true, receiver);

    if (likely(seL4_Fault_get_seL4_FaultType(sender->tcbFault) == seL4_Fault_NullFault)) {
        sendBuffer = lookupIPCBuffer(false, sender);/*Z 根据sender的指示消息，将发送者的消息(及授予的能力或标记)copy给接收者 */
        doNormalTransfer(sender, sendBuffer, endpoint, badge, grant,
                         receiver, receiveBuffer);
    } else {/*Z 发送错误类消息+一条指示性消息+一条badge消息(实质是写消息寄存器或IPC buffer) */
        doFaultTransfer(badge, sender, receiver, receiveBuffer);
    }
}

#ifdef CONFIG_KERNEL_MCS
void doReplyTransfer(tcb_t *sender, reply_t *reply, bool_t grant)
#else
/*Z 回复对方，之后删除回复能力。如果对方当前有错误，则不回复而是处理错误，并置对方重运行或不活跃。
sender回复方，receiver对方(要求回复方)，slot回复方CSlot，canGrant回复方属性 */
void doReplyTransfer(tcb_t *sender, tcb_t *receiver, cte_t *slot, bool_t grant)
#endif
{
#ifdef CONFIG_KERNEL_MCS
    if (reply->replyTCB == NULL ||
        thread_state_get_tsType(reply->replyTCB->tcbState) != ThreadState_BlockedOnReply) {
        /* nothing to do */
        return;
    }

    tcb_t *receiver = reply->replyTCB;
    reply_remove(reply);
    assert(thread_state_get_replyObject(receiver->tcbState) == REPLY_REF(0));
    assert(reply->replyTCB == NULL);
#else
    assert(thread_state_get_tsType(receiver->tcbState) ==
           ThreadState_BlockedOnReply);
#endif

    word_t fault_type = seL4_Fault_get_seL4_FaultType(receiver->tcbFault);
    if (likely(fault_type == seL4_Fault_NullFault)) {/*Z 对方(要求回复方)当前没有要处理的错误 */
        doIPCTransfer(sender, NULL, 0, grant, receiver);
#ifdef CONFIG_KERNEL_MCS
        setThreadState(receiver, ThreadState_Running);
#else
        /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (ucast cap_reply_cap))" */
        cteDeleteOne(slot);/*Z 回复对象用完就删 */
        setThreadState(receiver, ThreadState_Running);
        possibleSwitchTo(receiver);
#endif
    } else {/*Z 对方当前有要处理的错误。这是错误处理程序应答的路径 */
#ifndef CONFIG_KERNEL_MCS
        /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (ucast cap_reply_cap))" */
        cteDeleteOne(slot);/*Z 删除回复对象 */
#endif  /*Z 处理对方(要求回复方)的错误，返回对方是否需要重运行 */
        bool_t restart = handleFaultReply(receiver, sender);
        receiver->tcbFault = seL4_Fault_NullFault_new();/*Z 清除对方的错误 */
        if (restart) {
            setThreadState(receiver, ThreadState_Restart);
#ifndef CONFIG_KERNEL_MCS
            possibleSwitchTo(receiver);
#endif
        } else {
            setThreadState(receiver, ThreadState_Inactive);
        }
    }

#ifdef CONFIG_KERNEL_MCS
    if (receiver->tcbSchedContext && isRunnable(receiver)) {
        if ((refill_ready(receiver->tcbSchedContext) && refill_sufficient(receiver->tcbSchedContext, 0))) {
            possibleSwitchTo(receiver);
        } else {
            if (validTimeoutHandler(receiver) && fault_type != seL4_Fault_Timeout) {
                current_fault = seL4_Fault_Timeout_new(receiver->tcbSchedContext->scBadge);
                handleTimeout(receiver);
            } else {
                postpone(receiver->tcbSchedContext);
            }
        }
    }
#endif
}
/*Z 根据sender的指示消息，将发送者的消息(及授予的能力或标记)copy给接收者 */
void doNormalTransfer(tcb_t *sender, word_t *sendBuffer, endpoint_t *endpoint,
                      word_t badge, bool_t canGrant, tcb_t *receiver,
                      word_t *receiveBuffer)
{
    word_t msgTransferred;
    seL4_MessageInfo_t tag;
    exception_t status;
    extra_caps_t caps;
    /*Z 获取sender的指示消息 */
    tag = messageInfoFromWord(getRegister(sender, msgInfoRegister));

    if (canGrant) { /*Z 如果sender有CanGrant能力，则要查看其是否提供了extraCaps */
        status = lookupExtraCaps(sender, sendBuffer, tag);/*Z 在sender的IPC buffer中查找其消息指示的extraCaps内容 */
        caps = current_extra_caps;
        if (unlikely(status != EXCEPTION_NONE)) {/*Z 出错就认为没有 */
            caps.excaprefs[0] = NULL;
        }
    } else {        /*Z 如果sender没有CanGrant能力，则自然无需extraCaps */
        caps = current_extra_caps;
        caps.excaprefs[0] = NULL;
    }
    /*Z 将发送者的消息缓冲区copy给接收者 */
    msgTransferred = copyMRs(sender, sendBuffer, receiver, receiveBuffer,
                             seL4_MessageInfo_get_length(tag));
    /*Z 将发送者的授权能力(extraCaps)或其标记(badge)拷贝给接收者，返回实际发送数量的消息指示字。注意：拷贝的能力只能有一个 */
    tag = transferCaps(tag, caps, endpoint, receiver, receiveBuffer);
    /*Z 将消息指示字和标记发给接收者 */
    tag = seL4_MessageInfo_set_length(tag, msgTransferred);
    setRegister(receiver, msgInfoRegister, wordFromMessageInfo(tag));
    setRegister(receiver, badgeRegister, badge);
}
/*Z 发送错误类消息+一条指示性消息+一条badge消息(实质是写消息寄存器或IPC buffer) */
void doFaultTransfer(word_t badge, tcb_t *sender, tcb_t *receiver,
                     word_t *receiverIPCBuffer)
{
    word_t sent;
    seL4_MessageInfo_t msgInfo;
    /*Z 发送错误类消息 */
    sent = setMRs_fault(sender, receiver, receiverIPCBuffer);
    msgInfo = seL4_MessageInfo_new(/*Z 生成一个总结性(指示性)消息，记录了错误类型和消息长度 */
                  seL4_Fault_get_seL4_FaultType(sender->tcbFault), 0, 0, sent);
    setRegister(receiver, msgInfoRegister, wordFromMessageInfo(msgInfo));
    setRegister(receiver, badgeRegister, badge);
}
/*Z 根据指示信息，将授予的能力(extraCaps)或其标记(badge)拷贝给接收者，返回实际发送数量的消息指示字。注意：实际seL4目前最大只支持拷贝一个、打开一个能力 */
/* Like getReceiveSlots, this is specialised for single-cap transfer. */
static seL4_MessageInfo_t transferCaps(seL4_MessageInfo_t info, extra_caps_t caps,
                                       endpoint_t *endpoint, tcb_t *receiver,
                                       word_t *receiveBuffer)
{
    word_t i;
    cte_t *destSlot;
    /*Z 清掉capsUnwrapped和extraCaps字段 */
    info = seL4_MessageInfo_set_extraCaps(info, 0);
    info = seL4_MessageInfo_set_capsUnwrapped(info, 0);
    /*Z 无extraCaps传输需求就返回 */
    if (likely(!caps.excaprefs[0] || !receiveBuffer)) {
        return info;
    }
    /*Z 从接收者固定位置获取指示，查找能力接收CSlot地址(空能力) */
    destSlot = getReceiveSlots(receiver, receiveBuffer);

    for (i = 0; i < seL4_MsgMaxExtraCaps && caps.excaprefs[i] != NULL; i++) {
        cte_t *slot = caps.excaprefs[i];
        cap_t cap = slot->cap;/*Z 获取一个extraCap能力 */
        /*Z 如果要传递的能力是端点能力且EP指针指向的就是sender的EP队列(说明sender认为接收者已在队列) */
        if (cap_get_capType(cap) == cap_endpoint_cap &&
            EP_PTR(cap_endpoint_cap_get_capEPPtr(cap)) == endpoint) {
            /* If this is a cap to the endpoint on which the message was sent,
             * only transfer the badge, not the cap. */
            setExtraBadge(receiveBuffer,        /*Z 则只将badge写入接收者的IPC buffer(占用extraCaps位置) */
                          cap_endpoint_cap_get_capEPBadge(cap), i);
                                                /*Z 并更新消息指示字中的打开能力位图 */
            info = seL4_MessageInfo_set_capsUnwrapped(info,
                                                      seL4_MessageInfo_get_capsUnwrapped(info) | (1 << i));

        } else {
            deriveCap_ret_t dc_ret;

            if (!destSlot) {
                break;
            }
            /*Z 返回拷贝(导出)的能力 */
            dc_ret = deriveCap(slot, cap);

            if (dc_ret.status != EXCEPTION_NONE) {
                break;
            }
            if (cap_get_capType(dc_ret.cap) == cap_null_cap) {
                break;
            }
            /*Z 源、目的CSlot建立关联，并将新能力拷贝到目的CSlot。这里destSlot只能拷贝一次 */
            cteInsert(dc_ret.cap, slot, destSlot);

            destSlot = NULL;
        }
    }
    /*Z 更新消息指示字中要传输的extraCaps数量 */
    return seL4_MessageInfo_set_extraCaps(info, i);
}
/*Z 不阻塞接收失败时返回静默(标记寄存器值为0) */
void doNBRecvFailedTransfer(tcb_t *thread)
{
    /* Set the badge register to 0 to indicate there was no message */
    setRegister(thread, badgeRegister, 0);
}
/*Z 设置准备切换到下一个调度域 */
static void nextDomain(void)
{
    ksDomScheduleIdx++;/*Z 1. 更新活跃域索引 */
    if (ksDomScheduleIdx >= ksDomScheduleLength) {
        ksDomScheduleIdx = 0;
    }
#ifdef CONFIG_KERNEL_MCS
    NODE_STATE(ksReprogram) = true;/*Z 2. 设置重设调度计时 */
#endif
    ksWorkUnitsCompleted = 0;/*Z 3. 内核连续工作量计数复位 */
    ksCurDomain = ksDomSchedule[ksDomScheduleIdx].domain;/*Z 4. 更新当前域指示 */
#ifdef CONFIG_KERNEL_MCS
    ksDomainTime = usToTicks(ksDomSchedule[ksDomScheduleIdx].length * US_IN_MS);
#else
    ksDomainTime = ksDomSchedule[ksDomScheduleIdx].length;/*Z 5. 更新当前域剩余时间 */
#endif
}

#ifdef CONFIG_KERNEL_MCS
/*Z 切换当前调度上下文为ksCurThread的调度上下文，并对原上下文消费时间扣减预算 */
static void switchSchedContext(void)
{   /*Z 如果当前调度上下文不是ksCurThread的，且其充值循环队列元素数量上限不为0 */
    if (unlikely(NODE_STATE(ksCurSC) != NODE_STATE(ksCurThread)->tcbSchedContext) && NODE_STATE(ksCurSC)->scRefillMax) {
        NODE_STATE(ksReprogram) = true;/*Z 需重设调度计时器 */
        refill_unblock_check(NODE_STATE(ksCurThread->tcbSchedContext));/*Z 检查合并可以开始充值且有重叠的refill元素 */

        assert(refill_ready(NODE_STATE(ksCurThread->tcbSchedContext)));
        assert(refill_sufficient(NODE_STATE(ksCurThread->tcbSchedContext), 0));
    }

    if (NODE_STATE(ksReprogram)) {
        /* if we are reprogamming, we have acted on the new kernel time and cannot
         * rollback -> charge the current thread */
        commitTime();/*Z 对当前调度上下文，用ksConsumed扣除头元素预算，更新refill元素队列、scConsumed、ksDomainTime */
    }

    NODE_STATE(ksCurSC) = NODE_STATE(ksCurThread)->tcbSchedContext;
}
#endif
/*Z 如果当前调度域时间用尽，换下一个域。切换到最高优先级ready队列的首个线程页表，设置ksCurThread */
static void scheduleChooseNewThread(void)
{   /*Z 如果当前调度域时间用尽，换下一个域 */
    if (ksDomainTime == 0) {
        nextDomain();
    }
    chooseThread();/*Z 切换到最高优先级ready队列的首个线程页表，设置ksCurThread */
}
/*Z seL4调度器：定时做出或调整调度决策，发出等待的重调度IPI，切换调度上下文并扣减消费时间，设定下次调度deadline。
后面紧接着的是activateThread()实施决策。启动末尾和中断处理时调用，调用前会获取内核锁，因此任何时刻，仅有一个cpu处于此关键区。
调度决策表（重选规则是最高优先级ready队列的首线程）：
                调度前                                                   调度后：ksSchedulerAction=ResumeCurrent
ksSchedulerAction    ksCurThread   优先级   预选对象是当前域最高优先级       步骤及ksCurThread（空白处的是选择了预选对象）
----------------------------------------------------------------------      ---------------
ResumeCurrent        idle                                                      1. 不变
ResumeCurrent        可运行的                                                  1. 不变
ResumeCurrent        不可运行的                                                1. 不变

ChooseNew            idle                                                      2. 重选
ChooseNew            可运行的                                                  2. 重选
ChooseNew            不可运行的                                                2. 重选

预选对象             idle                         是                           
预选对象             idle                         不是                         3. 重选，预选优先
预选对象             可运行的       >             是
预选对象             可运行的       >             不是                         -
预选对象             可运行的       =             是                           4. 重选，原Cur优先，预选不优先
预选对象             可运行的       =             不是                         4. 重选，原Cur优先，预选不优先
预选对象             可运行的       <             是                           -
预选对象             可运行的       <             不是                         3. 重选，原Cur、预选优先
预选对象             不可运行的     >             是
预选对象             不可运行的     >             不是                         -
预选对象             不可运行的     =             是
预选对象             不可运行的     =             不是                         -
预选对象             不可运行的     <             是
预选对象             不可运行的     <             不是                         3. 重选，预选优先
 */
void schedule(void)
{
#ifdef CONFIG_KERNEL_MCS
    awaken();/*Z 检查待充值链表，视情出表头、加入调度队列、标记重调度 */
#endif

    if (NODE_STATE(ksSchedulerAction) != SchedulerAction_ResumeCurrentThread) {/*Z 没有最终决策时 */
        bool_t was_runnable;
        if (isSchedulable(NODE_STATE(ksCurThread))) {/*Z ksCurThread是可调度运行的 */
            was_runnable = true;
            SCHED_ENQUEUE_CURRENT_TCB;/*Z 重新加回ready队列头(优先) */
        } else {/*Z 这种情况仅发生在MCS配置下，其调度上下文被本次内核进入剥夺了 */
            was_runnable = false;
        }

        if (NODE_STATE(ksSchedulerAction) == SchedulerAction_ChooseNewThread) {
            scheduleChooseNewThread();/*Z 如果当前调度域时间用尽，换下一个域。切换到最高优先级ready队列的首个线程页表，设置ksCurThread */
        } else {/*Z 有预选对象则和ksCurThread比 */
            tcb_t *candidate = NODE_STATE(ksSchedulerAction);
            assert(isSchedulable(candidate));
            /* Avoid checking bitmap when ksCurThread is higher prio, to
             * match fast path.
             * Don't look at ksCurThread prio when it's idle, to respect
             * information flow in non-fastpath cases. */
            bool_t fastfail =
                NODE_STATE(ksCurThread) == NODE_STATE(ksIdleThread)
                || (candidate->tcbPriority < NODE_STATE(ksCurThread)->tcbPriority);
            if (fastfail && /*Z (ksCurThread是idle线程 || 比预选对象高) && 预选对象不是当前域的最高ready优先级，重新选择 */
                !isHighestPrio(ksCurDomain, candidate->tcbPriority)) {
                SCHED_ENQUEUE(candidate);/*Z 预选对象入队头(优先) */
                /* we can't, need to reschedule */
                NODE_STATE(ksSchedulerAction) = SchedulerAction_ChooseNewThread;
                scheduleChooseNewThread();                              /*Z ksCurThread可调度运行 && 它俩同级，重新选择 */
            } else if (was_runnable && candidate->tcbPriority == NODE_STATE(ksCurThread)->tcbPriority) {
                /* We append the candidate at the end of the scheduling queue, that way the
                 * current thread, that was enqueued at the start of the scheduling queue
                 * will get picked during chooseNewThread */
                SCHED_APPEND(candidate);/*Z 预选对象入队尾(不优先) */
                NODE_STATE(ksSchedulerAction) = SchedulerAction_ChooseNewThread;
                scheduleChooseNewThread();
            } else {                                    /*Z 其它所有情况，切换到预选对象页表，设置ksCurThread */
                assert(candidate != NODE_STATE(ksCurThread));
                switchToThread(candidate);/*Z 原来的不可运行的ksCurThread，如果没人干预将永远不会再运行。这是MCS要考虑的问题 */
            }
        }
    }
    NODE_STATE(ksSchedulerAction) = SchedulerAction_ResumeCurrentThread;
#ifdef ENABLE_SMP_SUPPORT   /*Z 发出等待的重调度IPI */
    doMaskReschedule(ARCH_NODE_STATE(ipiReschedulePending));
    ARCH_NODE_STATE(ipiReschedulePending) = 0;
#endif /* ENABLE_SMP_SUPPORT */

#ifdef CONFIG_KERNEL_MCS
    switchSchedContext();/*Z 切换当前调度上下文为ksCurThread的调度上下文，并对原上下文消费时间扣减预算 */

    if (NODE_STATE(ksReprogram)) {
        setNextInterrupt();/*Z 取ksCurThread的预算到时、当前调度域到时、最早release预算启用到时的最小值，设定deadline中断 */
        NODE_STATE(ksReprogram) = false;
    }
#endif
}
/*Z 切换到最高优先级ready队列的首个线程页表，设置ksCurThread */
void chooseThread(void)
{
    word_t prio;
    word_t dom;
    tcb_t *thread;

    if (CONFIG_NUM_DOMAINS > 1) {
        dom = ksCurDomain;
    } else {
        dom = 0;
    }

    if (likely(NODE_STATE(ksReadyQueuesL1Bitmap[dom]))) {/*Z ready队列不空 */
        prio = getHighestPrio(dom);/*Z 非空ready队列的最高优先级 */
        thread = NODE_STATE(ksReadyQueues)[ready_queues_index(dom, prio)].head;/*Z 首个线程 */
        assert(thread);
        assert(isSchedulable(thread));
#ifdef CONFIG_KERNEL_MCS
        assert(refill_sufficient(thread->tcbSchedContext, 0));
        assert(refill_ready(thread->tcbSchedContext));
#endif
        switchToThread(thread);/*Z 切换到指定线程页表，设置ksCurThread */
    } else {/*Z 切换到idle线程页表，设置ksCurThread */
        switchToIdleThread();
    }
}
/*Z 切换到指定线程页表，设置ksCurThread */
void switchToThread(tcb_t *thread)
{
#ifdef CONFIG_KERNEL_MCS
    assert(thread->tcbSchedContext != NULL);
    assert(!thread_state_get_tcbInReleaseQueue(thread->tcbState));
    assert(refill_sufficient(thread->tcbSchedContext, 0));
    assert(refill_ready(thread->tcbSchedContext));
#endif

#ifdef CONFIG_BENCHMARK_TRACK_UTILISATION
    benchmark_utilisation_switch(NODE_STATE(ksCurThread), thread);
#endif
    Arch_switchToThread(thread);/*Z 切换到指定线程页表 */
    tcbSchedDequeue(thread);/*Z 从ready队列中摘出线程 */
    NODE_STATE(ksCurThread) = thread;   /*Z 当前CPU的全局变量指向新线程 */
}
/*Z 切换到idle线程页表，设置ksCurThread */
void switchToIdleThread(void)
{
#ifdef CONFIG_BENCHMARK_TRACK_UTILISATION
    benchmark_utilisation_switch(NODE_STATE(ksCurThread), NODE_STATE(ksIdleThread));/*Z 未考察 */
#endif
    Arch_switchToIdleThread();/*Z 切换到idle线程页表 */
    NODE_STATE(ksCurThread) = NODE_STATE(ksIdleThread);
}
/*Z 设置线程的调度域 */
void setDomain(tcb_t *tptr, dom_t dom)
{
    tcbSchedDequeue(tptr);
    tptr->tcbDomain = dom;
    if (isSchedulable(tptr)) {
        SCHED_ENQUEUE(tptr);
    }
    if (tptr == NODE_STATE(ksCurThread)) {
        rescheduleRequired();
    }
}
/*Z 设置线程最大可控优先级 */
void setMCPriority(tcb_t *tptr, prio_t mcp)
{
    tptr->tcbMCP = mcp;
}
#ifdef CONFIG_KERNEL_MCS
void setPriority(tcb_t *tptr, prio_t prio)
{
    switch (thread_state_get_tsType(tptr->tcbState)) {
    case ThreadState_Running:
    case ThreadState_Restart:
        if (thread_state_get_tcbQueued(tptr->tcbState) || tptr == NODE_STATE(ksCurThread)) {
            tcbSchedDequeue(tptr);
            tptr->tcbPriority = prio;
            SCHED_ENQUEUE(tptr);
            rescheduleRequired();
        } else {
            tptr->tcbPriority = prio;
        }
        break;
    case ThreadState_BlockedOnReceive:
    case ThreadState_BlockedOnSend:
        tptr->tcbPriority = prio;
        reorderEP(EP_PTR(thread_state_get_blockingObject(tptr->tcbState)), tptr);
        break;
    case ThreadState_BlockedOnNotification:
        tptr->tcbPriority = prio;
        reorderNTFN(NTFN_PTR(thread_state_get_blockingObject(tptr->tcbState)), tptr);
        break;
    default:
        tptr->tcbPriority = prio;
        break;
    }
}
#else
/*Z 设置线程优先级 */
void setPriority(tcb_t *tptr, prio_t prio)
{   /*Z 从ready队列中摘出线程 */
    tcbSchedDequeue(tptr);
    tptr->tcbPriority = prio;
    if (isRunnable(tptr)) {
        if (tptr == NODE_STATE(ksCurThread)) {
            rescheduleRequired();
        } else {
            possibleSwitchTo(tptr);
        }
    }
}
#endif
/*Z 对不属于当前调度域、不是亲和cpu的线程，加入ready队列头(优先)并视情标记重调度IPI；否则设置重调度动作，视情入队头(优先) */
/* Note that this thread will possibly continue at the end of this kernel
 * entry. Do not queue it yet, since a queue+unqueue operation is wasteful
 * if it will be picked. Instead, it waits in the 'ksSchedulerAction' site
 * on which the scheduler will take action. */
void possibleSwitchTo(tcb_t *target)
{
#ifdef CONFIG_KERNEL_MCS
    if (target->tcbSchedContext != NULL && !thread_state_get_tcbInReleaseQueue(target->tcbState)) {
#endif  /*Z 不属于当前调度域、不是亲和cpu不会直接调度，仅是将线程加入ready队列，视情标记重调度IPI位图 */
        if (ksCurDomain != target->tcbDomain
            SMP_COND_STATEMENT( || target->tcbAffinity != getCurrentCPUIndex())) {
            SCHED_ENQUEUE(target);  /*Z 对当前调度域的当前cpu的线程，原调度器无最终决策时，设置重调度动作并入队 */
        } else if (NODE_STATE(ksSchedulerAction) != SchedulerAction_ResumeCurrentThread) {
            /* Too many threads want special treatment, use regular queues. */
            rescheduleRequired();
            SCHED_ENQUEUE(target);
        } else {/*Z 否则有最终决策的，设置本线程为预选对象，实际还是设置重调度。这样做是为了减少可能的入队后紧接着出队的开销 */
            NODE_STATE(ksSchedulerAction) = target;
        }
#ifdef CONFIG_KERNEL_MCS
    }
#endif

}
/*Z 设置线程状态 */
void setThreadState(tcb_t *tptr, _thread_state_t ts)
{   /*Z 设置tcb_t中的运行状态变量 */
    thread_state_ptr_set_tsType(&tptr->tcbState, ts);
    scheduleTCB(tptr);
}
/*Z 如果线程是最终决策对象但不可调度执行，则设置调度器重新选择对象 */
void scheduleTCB(tcb_t *tptr)
{
    if (tptr == NODE_STATE(ksCurThread) &&
        NODE_STATE(ksSchedulerAction) == SchedulerAction_ResumeCurrentThread &&
        !isSchedulable(tptr)) {
        rescheduleRequired();
    }
}

#ifdef CONFIG_KERNEL_MCS
/*Z 出ready队列，入release队列 */
void postpone(sched_context_t *sc)
{
    tcbSchedDequeue(sc->scTcb);
    tcbReleaseEnqueue(sc->scTcb);
    NODE_STATE_ON_CORE(ksReprogram, sc->scCore) = true;
}
/*Z 取ksCurThread的预算到时、当前调度域到时、最早release预算启用到时的最小值，设定deadline中断 */
void setNextInterrupt(void)
{   /*Z ksCurTime + ksCurThread的头一个预算 */
    time_t next_interrupt = NODE_STATE(ksCurTime) +
                            REFILL_HEAD(NODE_STATE(ksCurThread)->tcbSchedContext).rAmount;
    /*Z 当前调度域剩余时间 */
    if (CONFIG_NUM_DOMAINS > 1) {                           
        next_interrupt = MIN(next_interrupt, NODE_STATE(ksCurTime) + ksDomainTime);
    }
    /*Z 等待预算启用时间 */
    if (NODE_STATE(ksReleaseHead) != NULL) {
        next_interrupt = MIN(REFILL_HEAD(NODE_STATE(ksReleaseHead)->tcbSchedContext).rTime, next_interrupt);
    }
    /*Z 设定TSC deadline */
    setDeadline(next_interrupt - getTimerPrecision());
}
/*Z 预算超支(refill队列满的也认为超支)：实施预算，调整充值，时间片结束处理 */
void chargeBudget(ticks_t capacity,     /*Z 抵扣后剩余 */
                  ticks_t consumed,     /*Z 已用=ksConsumed。以上两项加起来=refill头单元.rAmount */
                  bool_t canTimeoutFault, /*Z 是否允许超时错误 */
                  word_t core,          /*Z 所在CPU */
                  bool_t isCurCPU)      /*Z 是否当前CPU */
{
    /*Z 扣减预算，调整充值 */
    if (isRoundRobin(NODE_STATE_ON_CORE(ksCurSC, core))) {/*Z RoundRobin算法复位循环 */
        assert(refill_size(NODE_STATE_ON_CORE(ksCurSC, core)) == MIN_REFILLS);
        REFILL_HEAD(NODE_STATE_ON_CORE(ksCurSC, core)).rAmount += REFILL_TAIL(NODE_STATE_ON_CORE(ksCurSC, core)).rAmount;
        REFILL_TAIL(NODE_STATE_ON_CORE(ksCurSC, core)).rAmount = 0;
    } else {
        refill_budget_check(consumed, capacity);/*Z 根据参数，执行预算，更新refill队列 */
    }
    /*Z 累加消费，调度队列处理 */
    assert(REFILL_HEAD(NODE_STATE_ON_CORE(ksCurSC, core)).rAmount >= MIN_BUDGET);
    NODE_STATE_ON_CORE(ksCurSC, core)->scConsumed += consumed;/*Z 累加SC已消费 */
    NODE_STATE_ON_CORE(ksConsumed, core) = 0;
    if (isCurCPU && likely(isSchedulable(NODE_STATE_ON_CORE(ksCurThread, core)))) {
        assert(NODE_STATE(ksCurThread)->tcbSchedContext == NODE_STATE(ksCurSC));
        endTimeslice(canTimeoutFault);/*Z 时间片结束处理 */
        rescheduleRequired();
        NODE_STATE(ksReprogram) = true;
    }
}
/*Z 时间片结束处理：要么发送超时错误通知(参数指明)，要么加入ready/release队列 */
void endTimeslice(bool_t can_timeout_fault)
{
    if (can_timeout_fault && validTimeoutHandler(NODE_STATE(ksCurThread))) {
        current_fault = seL4_Fault_Timeout_new(NODE_STATE(ksCurSC)->scBadge);
        handleTimeout(NODE_STATE(ksCurThread));
    } else if (refill_ready(NODE_STATE(ksCurSC)) && refill_sufficient(NODE_STATE(ksCurSC), 0)) {
        /* apply round robin *//*Z 预算可用且足够进出内核(refill队列满的情况)：丢弃剩余，加入ready */
        assert(refill_sufficient(NODE_STATE(ksCurSC), 0));/*Z 不好：冗余 */
        assert(!thread_state_get_tcbQueued(NODE_STATE(ksCurThread)->tcbState));
        SCHED_APPEND_CURRENT_TCB;
    } else {
        /* postpone until ready *//*Z 加入release队列 */
        postpone(NODE_STATE(ksCurSC));
    }
}
#else
/*Z 定时器中断处理函数：递减当前线程和调度域时间片，达到时限时设置重调度 */
void timerTick(void)
{
    if (likely(thread_state_get_tsType(NODE_STATE(ksCurThread)->tcbState) ==
               ThreadState_Running)
#ifdef CONFIG_VTX
        || thread_state_get_tsType(NODE_STATE(ksCurThread)->tcbState) ==
        ThreadState_RunningVM
#endif
       ) {
        if (NODE_STATE(ksCurThread)->tcbTimeSlice > 1) {
            NODE_STATE(ksCurThread)->tcbTimeSlice--;
        } else {    /*Z 时间片是循环的 */
            NODE_STATE(ksCurThread)->tcbTimeSlice = CONFIG_TIME_SLICE;
            SCHED_APPEND_CURRENT_TCB;/*Z 队列尾 */
            rescheduleRequired();
        }
    }

    if (CONFIG_NUM_DOMAINS > 1) {
        ksDomainTime--;
        if (ksDomainTime == 0) {
            rescheduleRequired();
        }
    }
}
#endif
/*Z 设置调度器动作为选择新对象 */
void rescheduleRequired(void)
{
    if (NODE_STATE(ksSchedulerAction) != SchedulerAction_ResumeCurrentThread
        && NODE_STATE(ksSchedulerAction) != SchedulerAction_ChooseNewThread
#ifdef CONFIG_KERNEL_MCS
        && isSchedulable(NODE_STATE(ksSchedulerAction))
#endif
       ) {/*Z 原动作为预选对象，将其重新加入ready队列头(同队列优先执行) */
#ifdef CONFIG_KERNEL_MCS
        assert(refill_sufficient(NODE_STATE(ksSchedulerAction)->tcbSchedContext, 0));
        assert(refill_ready(NODE_STATE(ksSchedulerAction)->tcbSchedContext));
#endif
        SCHED_ENQUEUE(NODE_STATE(ksSchedulerAction));
    }
    NODE_STATE(ksSchedulerAction) = SchedulerAction_ChooseNewThread;
}

#ifdef CONFIG_KERNEL_MCS
/*Z 检查待充值链表，视情出表头、加入调度队列、标记重调度 */
void awaken(void)
{   /*Z 如果等待充值的队列不为空，且达到可以开始充值的时间 */
    while (unlikely(NODE_STATE(ksReleaseHead) != NULL && refill_ready(NODE_STATE(ksReleaseHead)->tcbSchedContext))) {
        tcb_t *awakened = tcbReleaseDequeue();/*Z 待充值链表出表头 */
        /* the currently running thread cannot have just woken up */
        assert(awakened != NODE_STATE(ksCurThread));
        /* round robin threads should not be in the release queue */
        assert(!isRoundRobin(awakened->tcbSchedContext));
        /* threads should wake up on the correct core */
        SMP_COND_STATEMENT(assert(awakened->tcbAffinity == getCurrentCPUIndex()));
        /* threads HEAD refill should always be > MIN_BUDGET */
        assert(refill_sufficient(awakened->tcbSchedContext, 0));
        possibleSwitchTo(awakened);/*Z 对不属于当前调度域、不是亲和cpu的线程，加入ready队列并视情标记重调度IPI；否则设置重调度动作，视情入队 */
        /* changed head of release queue -> need to reprogram */
        NODE_STATE(ksReprogram) = true;/*Z 标记重设调度计时器 */
    }
}
#endif
